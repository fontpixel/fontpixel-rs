use super::*;

/// Bit significance inside a stored byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitOrder {
    /// High bit is leftmost.
    MsbFirst,
    /// Low bit is leftmost.
    LsbFirst,
}
/// Significance order of bytes in a multi-byte scan unit (X11/PCF convention).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteOrder {
    /// High-significance byte first.
    Big,
    /// Low-significance byte first.
    Little,
}
/// Order of rows in a serialized bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowOrder {
    /// First row is the top.
    TopDown,
    /// First row is the bottom.
    BottomUp,
}
/// Packed wire layout, including X11's independent bit/byte order and scan units.
/// Swapping happens when bit and byte significance differ. For example, LSB bits
/// with little-endian bytes need bit reversal only. Inconsistent partial scan units
/// (`scan_unit > row_alignment` when swapping) are rejected, as SPEC-v3 requires.
/// Gray8 accepts only MSB/Big/scan_unit=1; row padding/order still apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteLayout {
    /// Significance within bytes.
    pub bit_order: BitOrder,
    /// Significance within scan units.
    pub byte_order: ByteOrder,
    /// Row padding in bytes: 1, 2, 4 or 8.
    pub row_alignment: u8,
    /// Bytes per scan unit: 1, 2, 4 or 8.
    pub scan_unit: u8,
    /// Top-down or bottom-up.
    pub row_order: RowOrder,
}
impl Default for ByteLayout {
    fn default() -> Self {
        Self {
            bit_order: BitOrder::MsbFirst,
            byte_order: ByteOrder::Big,
            row_alignment: 1,
            scan_unit: 1,
            row_order: RowOrder::TopDown,
        }
    }
}
impl ByteLayout {
    fn swap(self) -> bool {
        (self.bit_order == BitOrder::MsbFirst) != (self.byte_order == ByteOrder::Big)
    }
    fn stride<F: PixelFormat>(self, width: u32) -> Result<usize> {
        if ![1, 2, 4, 8].contains(&self.row_alignment) || ![1, 2, 4, 8].contains(&self.scan_unit) {
            return Err(invalid("row alignment and scan unit must be 1, 2, 4 or 8"));
        }
        if self.swap() && self.scan_unit > self.row_alignment {
            return Err(invalid(
                "scan unit exceeds row alignment with mismatched bit/byte order",
            ));
        }
        if !F::PACKED
            && (self.bit_order != BitOrder::MsbFirst
                || self.byte_order != ByteOrder::Big
                || self.scan_unit != 1)
        {
            return Err(invalid(
                "Gray8 has byte samples; bit/scan-unit order is not applicable",
            ));
        }
        let row = if F::PACKED {
            u64::from(width).div_ceil(8)
        } else {
            u64::from(width)
        };
        usize::try_from(row.div_ceil(u64::from(self.row_alignment)) * u64::from(self.row_alignment))
            .map_err(|_| invalid("serialized bitmap stride overflows"))
    }
    fn transform(self, row: &mut [u8]) {
        if self.bit_order == BitOrder::LsbFirst {
            for b in row.iter_mut() {
                *b = b.reverse_bits();
            }
        }
        if self.swap() {
            for unit in row.chunks_exact_mut(self.scan_unit as usize) {
                unit.reverse();
            }
        }
    }
}
impl<F: PixelFormat> Bitmap<F> {
    /// Read an exact-sized byte stream. Extra/truncated input fails; padding is
    /// ignored on input and normalized in memory. No thresholding occurs.
    pub fn from_bytes(
        width: u32,
        height: u32,
        data: &[u8],
        layout: ByteLayout,
        limits: BitmapLimits,
    ) -> Result<Self> {
        let stride = layout.stride::<F>(width)?;
        let len = stride
            .checked_mul(height as usize)
            .ok_or_else(|| invalid("serialized bitmap size overflows"))?;
        limits.check(width, height, len)?;
        if data.len() != len {
            return Err(invalid(
                "serialized bitmap length does not match dimensions and layout",
            ));
        }
        let mut out = Self::with_limits(width, height, limits)?;
        let canonical = out.stride();
        if stride == 0 {
            return Ok(out);
        }
        let mut row = zeros(stride, limits.max_bytes)?;
        for (i, source) in data.chunks_exact(stride).enumerate() {
            row.copy_from_slice(source);
            layout.transform(&mut row);
            let y = match layout.row_order {
                RowOrder::TopDown => i as u32,
                RowOrder::BottomUp => height - 1 - i as u32,
            };
            out.row_mut(y).copy_from_slice(&row[..canonical]);
        }
        out.clear_padding();
        Ok(out)
    }
    /// Export deterministic bytes in an explicit layout; every padding bit/byte
    /// is zero before bit/byte-order transformation.
    pub fn to_bytes(&self, layout: ByteLayout) -> Result<Vec<u8>> {
        let stride = layout.stride::<F>(self.width)?;
        let len = stride
            .checked_mul(self.height as usize)
            .ok_or_else(|| invalid("serialized bitmap size overflows"))?;
        let mut out = zeros(len, self.limits.max_bytes)?;
        if stride == 0 {
            return Ok(out);
        }
        for (i, row) in out.chunks_exact_mut(stride).enumerate() {
            let y = match layout.row_order {
                RowOrder::TopDown => i as u32,
                RowOrder::BottomUp => self.height - 1 - i as u32,
            };
            row[..self.stride()].copy_from_slice(self.row(y));
            layout.transform(row);
        }
        Ok(out)
    }
}
impl Bitmap<Mono> {
    /// Parse strict BDF-style, byte-padded hex rows. Rows determine height;
    /// hex can use either case. Low padding bits are normalized to zero.
    pub fn from_hex_rows(width: u32, rows: &[&str], limits: BitmapLimits) -> Result<Self> {
        let height = u32::try_from(rows.len()).map_err(|_| invalid("too many hexadecimal rows"))?;
        let mut out = Self::with_limits(width, height, limits)?;
        let digits = out
            .stride()
            .checked_mul(2)
            .ok_or_else(|| invalid("hex row too wide"))?;
        for (y, row) in rows.iter().enumerate() {
            if row.len() != digits {
                return Err(invalid("hexadecimal bitmap row length mismatch"));
            }
            for (byte, pair) in out
                .row_mut(y as u32)
                .iter_mut()
                .zip(row.as_bytes().as_chunks::<2>().0)
            {
                let high = (pair[0] as char)
                    .to_digit(16)
                    .ok_or_else(|| invalid("invalid hexadecimal bitmap digit"))?;
                let low = (pair[1] as char)
                    .to_digit(16)
                    .ok_or_else(|| invalid("invalid hexadecimal bitmap digit"))?;
                *byte = ((high << 4) | low) as u8;
            }
        }
        out.clear_padding();
        Ok(out)
    }
    /// Byte-padded, left-aligned uppercase hex rows for BDF glyph storage.
    pub fn to_hex_rows(&self) -> Result<Vec<String>> {
        self.hex_rows(true)
    }
    /// Lowercase numeric hex rows (right-aligned, ceil(width/4) digits), as SPEC
    /// `todata(4)`. Zero-width rows are empty strings.
    pub fn to_numeric_hex_rows(&self) -> Result<Vec<String>> {
        self.hex_rows(false)
    }
    fn hex_rows(&self, padded: bool) -> Result<Vec<String>> {
        let digits = if padded {
            self.stride() * 2
        } else {
            self.width.div_ceil(4) as usize
        };
        let total = digits
            .checked_mul(self.height as usize)
            .and_then(|n| {
                (self.height as usize)
                    .checked_mul(std::mem::size_of::<String>())
                    .and_then(|overhead| n.checked_add(overhead))
            })
            .ok_or_else(|| invalid("hexadecimal output too large"))?;
        if total > self.limits.max_bytes {
            return Err(limit("hexadecimal bitmap output", self.limits.max_bytes));
        }
        let alphabet = if padded {
            b"0123456789ABCDEF"
        } else {
            b"0123456789abcdef"
        };
        let mut rows = Vec::new();
        rows.try_reserve_exact(self.height as usize)
            .map_err(|_| limit("hex row allocation", self.limits.max_bytes))?;
        for y in 0..self.height {
            let mut row = String::new();
            row.try_reserve_exact(digits)
                .map_err(|_| limit("hex row allocation", self.limits.max_bytes))?;
            if padded {
                for &byte in self.row(y) {
                    row.push(alphabet[(byte >> 4) as usize] as char);
                    row.push(alphabet[(byte & 15) as usize] as char);
                }
            } else {
                for nibble in 0..digits {
                    let start = nibble as i64 * 4 - i64::from((4 - self.width % 4) % 4);
                    let mut value = 0;
                    for i in 0..4 {
                        value <<= 1;
                        let x = start + i;
                        if x >= 0 {
                            value |= self.pixel(x as u32, y);
                        }
                    }
                    row.push(alphabet[value as usize] as char);
                }
            }
            rows.push(row);
        }
        Ok(rows)
    }
    /// Numeric rows for widths up to 64, without right-hand padding. For wider
    /// rows use `to_numeric_hex_rows`, which has no machine-integer truncation.
    pub fn to_row_values(&self) -> Result<Vec<u64>> {
        if self.width > 64 {
            return Err(invalid(
                "numeric bitmap rows exceed u64; use hexadecimal rows",
            ));
        }
        let bytes = (self.height as usize)
            .checked_mul(std::mem::size_of::<u64>())
            .ok_or_else(|| invalid("numeric row count overflows"))?;
        if bytes > self.limits.max_bytes {
            return Err(limit("numeric bitmap rows", self.limits.max_bytes));
        }
        let mut out = Vec::new();
        out.try_reserve_exact(self.height as usize)
            .map_err(|_| limit("numeric rows allocation", self.limits.max_bytes))?;
        for y in 0..self.height {
            let mut value = 0;
            for &b in self.row(y) {
                value = (value << 8) | u64::from(b);
            }
            value >>= (8 - self.width % 8) % 8;
            out.push(value);
        }
        Ok(out)
    }
}
