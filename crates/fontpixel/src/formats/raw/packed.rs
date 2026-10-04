//! Explicit raw strike and packed-bit address arithmetic, with no model allocations.
use super::*;
#[derive(Clone, Copy)]
pub(super) struct Geometry {
    pub row: usize,
    pub group: usize,
    pub glyph_bits: usize,
    pub count: usize,
    pub payload: usize,
}
fn checked_mul(a: usize, b: usize) -> Result<usize> {
    a.checked_mul(b)
        .ok_or_else(|| invalid("raw layout multiplication overflow"))
}
fn checked_add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| invalid("raw layout addition overflow"))
}
impl Geometry {
    fn base(layout: &RawLayout, l: &ResourceLimits) -> Result<Self> {
        if layout.default_load_address.is_some() && layout.offset != 2 {
            return Err(invalid("raw PRG load address requires a two-byte offset"));
        }
        cell_size(layout.width, layout.height, l)?;
        check(layout.offset, l.max_table_bytes, "raw prefix bytes")?;
        if layout.strike_count == 0 {
            return Err(invalid("raw strike_count must be positive"));
        }
        let glyph_bits = checked_mul(layout.width as usize, layout.height as usize)?;
        if layout.packing == RawPacking::BitAligned {
            if layout.strike_count != 1
                || layout.row_padding != 0
                || layout.byte_order != RawByteOrder::RowMajor
            {
                return Err(invalid(
                    "bit-aligned raw requires one cell per strike, no row padding and row-major bytes",
                ));
            }
            return Ok(Self {
                row: 0,
                group: glyph_bits.div_ceil(8),
                glyph_bits,
                count: 0,
                payload: 0,
            });
        }
        let strike_bits = checked_mul(layout.width as usize, layout.strike_count)?;
        let row = checked_add(strike_bits.div_ceil(8), layout.row_padding)?;
        let group = checked_mul(row, layout.height as usize)?;
        check(group, l.max_table_bytes, "raw strike bytes")?;
        Ok(Self {
            row,
            group,
            glyph_bits,
            count: 0,
            payload: 0,
        })
    }
    pub fn for_count(layout: &RawLayout, count: usize, l: &ResourceLimits) -> Result<Self> {
        let mut g = Self::base(layout, l)?;
        if count == 0 {
            return Err(invalid("raw layout requires glyphs"));
        }
        check(count, l.max_glyphs, "raw glyph count")?;
        g.count = count;
        g.payload = if layout.packing == RawPacking::BitAligned {
            checked_mul(count, g.glyph_bits)?.div_ceil(8)
        } else {
            checked_mul(count.div_ceil(layout.strike_count), g.group)?
        };
        checked_mul(g.payload, 8)?;
        check(g.payload, l.max_table_bytes, "raw bitmap table")?;
        let last = u32::try_from(count - 1).map_err(|_| invalid("raw codepoint count overflow"))?;
        layout
            .first_codepoint
            .checked_add(last)
            .ok_or_else(|| invalid("raw positional code overflows u32"))?;
        Ok(g)
    }
    pub fn read(layout: &RawLayout, bytes: usize, l: &ResourceLimits) -> Result<Self> {
        let base = Self::base(layout, l)?;
        let count = if let Some(n) = layout.count {
            n
        } else if layout.packing == RawPacking::BitAligned {
            checked_mul(bytes, 8)? / base.glyph_bits
        } else {
            if !bytes.is_multiple_of(base.group) {
                return Err(invalid("raw input ends in a partial strike"));
            }
            checked_mul(bytes / base.group, layout.strike_count)?
        };
        let g = Self::for_count(layout, count, l)?;
        if bytes != g.payload {
            return Err(invalid("raw count must consume the payload exactly"));
        }
        Ok(g)
    }
    pub fn legacy(layout: &RawLayout) -> bool {
        layout.packing == RawPacking::ByteAlignedRows
            && layout.byte_order == RawByteOrder::RowMajor
            && layout.strike_count == 1
    }
    fn byte_at(&self, layout: &RawLayout, index: usize, wire_row: usize, column: usize) -> usize {
        let start = (index / layout.strike_count) * self.group;
        start
            + if layout.byte_order == RawByteOrder::RowMajor {
                wire_row * self.row + column
            } else {
                column * layout.height as usize + wire_row
            }
    }
    fn bit_at(&self, layout: &RawLayout, index: usize, y: usize, x: usize) -> usize {
        let wire_y = if layout.row_order == RowOrder::TopDown {
            y
        } else {
            layout.height as usize - 1 - y
        };
        if layout.packing == RawPacking::BitAligned {
            index * self.glyph_bits + wire_y * layout.width as usize + x
        } else {
            let sx = (index % layout.strike_count) * layout.width as usize + x;
            self.byte_at(layout, index, wire_y, sx / 8) * 8 + sx % 8
        }
    }
    pub fn decode(&self, layout: &RawLayout, data: &[u8], index: usize, out: &mut [u8]) -> bool {
        let stride = (layout.width as usize).div_ceil(8);
        out.fill(0);
        if layout.packing == RawPacking::ByteAlignedRows && layout.width.is_multiple_of(8) {
            let first_column = (index % layout.strike_count) * stride;
            for y in 0..layout.height as usize {
                let wire_y = if layout.row_order == RowOrder::TopDown {
                    y
                } else {
                    layout.height as usize - 1 - y
                };
                for x in 0..stride {
                    let v = data[self.byte_at(layout, index, wire_y, first_column + x)];
                    out[y * stride + x] = if layout.bit_order == BitOrder::MsbFirst {
                        v
                    } else {
                        v.reverse_bits()
                    };
                }
            }
        } else {
            for y in 0..layout.height as usize {
                for x in 0..layout.width as usize {
                    let at = self.bit_at(layout, index, y, x);
                    if bit(data, at, layout.bit_order) {
                        out[y * stride + x / 8] |= 0x80 >> (x % 8);
                    }
                }
            }
        }
        if layout.packing == RawPacking::BitAligned {
            return index + 1 == self.count
                && (self.count * self.glyph_bits..data.len() * 8)
                    .any(|at| bit(data, at, layout.bit_order));
        }
        let first = (index / layout.strike_count) * layout.strike_count;
        let valid = layout.strike_count.min(self.count - first);
        if index % layout.strike_count + 1 != valid {
            return false;
        }
        let used = valid * layout.width as usize;
        (0..layout.height as usize).any(|y| {
            (used..self.row * 8).any(|x| {
                bit(
                    data,
                    self.byte_at(layout, index, y, x / 8) * 8 + x % 8,
                    layout.bit_order,
                )
            })
        })
    }
    pub fn encode(&self, layout: &RawLayout, data: &mut [u8], index: usize, source: &[u8]) {
        let stride = (layout.width as usize).div_ceil(8);
        if layout.packing == RawPacking::ByteAlignedRows && layout.width.is_multiple_of(8) {
            let first_column = (index % layout.strike_count) * stride;
            for y in 0..layout.height as usize {
                let wire_y = if layout.row_order == RowOrder::TopDown {
                    y
                } else {
                    layout.height as usize - 1 - y
                };
                for x in 0..stride {
                    let v = source[y * stride + x];
                    let at = self.byte_at(layout, index, wire_y, first_column + x);
                    data[at] = if layout.bit_order == BitOrder::MsbFirst {
                        v
                    } else {
                        v.reverse_bits()
                    };
                }
            }
        } else {
            for y in 0..layout.height as usize {
                for x in 0..layout.width as usize {
                    if source[y * stride + x / 8] & (0x80 >> (x % 8)) != 0 {
                        let at = self.bit_at(layout, index, y, x);
                        let shift = if layout.bit_order == BitOrder::MsbFirst {
                            7 - at % 8
                        } else {
                            at % 8
                        };
                        data[at / 8] |= 1 << shift;
                    }
                }
            }
        }
    }
}
fn bit(data: &[u8], at: usize, order: BitOrder) -> bool {
    let shift = if order == BitOrder::MsbFirst {
        7 - at % 8
    } else {
        at % 8
    };
    data[at / 8] & (1 << shift) != 0
}
