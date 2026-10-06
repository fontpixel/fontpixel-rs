use crate::bitmap::{Alignment, Bitmap as Raster, BitmapLimits, Bounds, Direction, Gray8};
use crate::{Error, Result};

/// Mutable/chaining SPEC-v3 bitmap, backed by the native labelled Gray8 raster.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bitmap {
    pub(crate) raster: Raster<Gray8>,
}
impl Bitmap {
    /// Construct a rectangular raster of labels 0, 1 and 2.
    pub fn new(rows: &[String]) -> Result<Self> {
        let w = rows.first().map_or(0, String::len);
        if rows
            .iter()
            .any(|r| r.len() != w || r.bytes().any(|b| !matches!(b, b'0'..=b'2')))
        {
            return Err(Error::InvalidInput(
                "Bitmap rows must be rectangular labels 0, 1, 2".into(),
            ));
        }
        let w = u32::try_from(w).map_err(|_| invalid("bitmap width exceeds u32"))?;
        let h = u32::try_from(rows.len()).map_err(|_| invalid("bitmap height exceeds u32"))?;
        let mut raster = Raster::new(w, h)?;
        for (y, r) in rows.iter().enumerate() {
            for (x, p) in r.bytes().enumerate() {
                raster.set_pixel(x as u32, y as u32, p - b'0')?;
            }
        }
        Ok(Self { raster })
    }
    /// Parse #, ., &, 0, 1 and 2 text; only leading/trailing empty lines are removed.
    pub fn fromstring(text: &str) -> Result<Self> {
        let s = text.replace('\r', "");
        let lines = s.split('\n').collect::<Vec<_>>();
        let start = lines
            .iter()
            .position(|r| !r.trim().is_empty())
            .unwrap_or(lines.len());
        let end = lines
            .iter()
            .rposition(|r| !r.trim().is_empty())
            .map(|i| i + 1)
            .unwrap_or(start);
        let rows = lines[start..end]
            .iter()
            .map(|r| r.replace('#', "1").replace('.', "0").replace('&', "2"))
            .collect::<Vec<_>>();
        Self::new(&rows)
    }
    /// Make an all-zero bitmap. Python loses width when height is zero.
    pub fn blank(w: i64, h: i64) -> Result<Self> {
        Ok(Self {
            raster: {
                let w = dim(w)?;
                Raster::new(if h == 0 { 0 } else { w }, dim(h)?)?
            },
        })
    }
    /// Width in pixels; an empty row list always has width zero.
    pub fn width(&self) -> u32 {
        self.raster.width()
    }
    /// Number of rows.
    pub fn height(&self) -> u32 {
        self.raster.height()
    }
    /// Materialize the compatibility row strings.
    pub fn bindata(&self) -> Vec<String> {
        self.label_rows()
            .map(|r| r.iter().map(|v| char::from(b'0' + v)).collect())
            .collect()
    }
    /// Pixel labels (0, 1, 2) row by row, top to bottom, without copying.
    pub fn label_rows(&self) -> impl Iterator<Item = &[u8]> {
        let w = self.width() as usize;
        let data = self.raster.as_bytes();
        (0..self.height() as usize).map(move |y| &data[y * w..(y + 1) * w])
    }
    /// bdfparser `todata(4)`/`todata(5)` row values: lower-case hex digits of
    /// `int(row, 2)` zero-filled to `ceil(width / 4)` (empty for an empty row),
    /// or the row as a big-endian bit string's decimal digits. A row with a
    /// label 2 is not binary.
    pub fn row_number(row: &[u8], hex: bool) -> Result<String> {
        if row.contains(&2) {
            return Err(invalid("invalid literal for int() with base 2"));
        }
        if hex {
            let pad = row.len().div_ceil(4) * 4 - row.len();
            return Ok(std::iter::repeat_n(&0, pad)
                .chain(row)
                .copied()
                .collect::<Vec<_>>()
                .chunks(4)
                .map(|b| {
                    let v = b.iter().fold(0u32, |a, c| a * 2 + u32::from(*c));
                    char::from_digit(v, 16).unwrap_or('0')
                })
                .collect());
        }
        if row.len() <= 128 {
            let v = row.iter().fold(0u128, |a, c| a << 1 | u128::from(*c));
            return Ok(v.to_string());
        }
        // Arbitrary width: repeated doubling of base-10^18 limbs.
        const BASE: u64 = 1_000_000_000_000_000_000;
        let mut limbs = vec![0u64];
        for &bit in row {
            let mut carry = u64::from(bit);
            for l in &mut limbs {
                let x = *l * 2 + carry;
                *l = x % BASE;
                carry = x / BASE;
            }
            if carry != 0 {
                limbs.push(carry);
            }
        }
        let mut out = limbs.last().copied().unwrap_or(0).to_string();
        for l in limbs.iter().rev().skip(1) {
            out.push_str(&format!("{l:018}"));
        }
        Ok(out)
    }
    /// Normalize Python's zero-height shape after a native operation.
    pub(crate) fn from_raster(raster: Raster<Gray8>) -> Self {
        if raster.height() == 0 {
            Self::default()
        } else {
            Self { raster }
        }
    }
    /// Crop or extend using a bottom-left origin.
    pub fn crop(&mut self, w: i64, h: i64, x: i64, y: i64) -> Result<&mut Self> {
        if w < 0 || h < 0 {
            return Err(invalid(&format!(
                "Bitmap.crop: negative width or height ({w}, {h})"
            )));
        }
        *self = Self::from_raster(self.raster.crop(dim(w)?, dim(h)?, x, y)?);
        Ok(self)
    }
    /// Overlay a top-left aligned bitmap, retaining this bitmap's size.
    pub fn overlay(&mut self, other: &Self) -> &mut Self {
        self.raster.overlay(&other.raster);
        self
    }
    /// Concatenate using SPEC's signed directions and alignment flag.
    pub fn concatall(bitmaps: &[Self], direction: i64, align: i64, gaps: &[i64]) -> Result<Self> {
        let horizontal = direction > 0;
        let d = match direction {
            1 => Direction::Right,
            n if n > 0 => Direction::Left,
            0 => Direction::Down,
            _ => Direction::Up,
        };
        let a = if (horizontal && align == 0) || (!horizontal && align != 0) {
            Alignment::Start
        } else {
            Alignment::End
        };
        let refs = bitmaps.iter().map(|b| &b.raster).collect::<Vec<_>>();
        Ok(Self::from_raster(Raster::concat_all(
            &refs,
            d,
            a,
            if gaps.is_empty() {
                gaps
            } else {
                gaps.get(..bitmaps.len().saturating_sub(1))
                    .ok_or_else(|| invalid("missing concatenation offset"))?
            },
            BitmapLimits::default(),
        )?))
    }
    /// Concatenate in place.
    pub fn concat(
        &mut self,
        other: &Self,
        direction: i64,
        align: i64,
        gap: i64,
    ) -> Result<&mut Self> {
        *self = Self::concatall(&[self.clone(), other.clone()], direction, align, &[gap])?;
        Ok(self)
    }
    /// Integer nearest-neighbor enlarge; factors <= 1 leave that axis unchanged.
    pub fn enlarge(&mut self, x: i64, y: i64) -> Result<&mut Self> {
        *self = Self::from_raster(self.raster.enlarge(dim(x.max(1))?, dim(y.max(1))?)?);
        Ok(self)
    }
    /// Replace label substrings; invalid/ragged results are refused explicitly.
    pub fn replace(&mut self, old: &str, new: &str) -> Result<&mut Self> {
        let rows = self
            .bindata()
            .iter()
            .map(|r| r.replace(old, new))
            .collect::<Vec<_>>();
        *self = Self::new(&rows)?;
        Ok(self)
    }
    /// Add label-2 shadow behind the original image.
    pub fn shadow(&mut self, x: i64, y: i64) -> Result<&mut Self> {
        *self = Self::from_raster(self.raster.shadow(x, y, 2)?);
        Ok(self)
    }
    /// Add four- or eight-neighbor label-2 glow around label-1 pixels.
    pub fn glow(&mut self, mode: i64) -> Result<&mut Self> {
        *self = Self::from_raster(self.raster.glow(mode == 1, 1, 2)?);
        Ok(self)
    }
    /// Pad row width to a multiple of bits.
    pub fn bytepad(&mut self, bits: i64) -> Result<&mut Self> {
        if bits <= 0 {
            return Err(invalid("bytepad requires positive bits"));
        }
        *self = Self::from_raster(self.raster.byte_pad(dim(bits)?)?);
        Ok(self)
    }
    /// Read a label at a top-left coordinate.
    pub fn getpixel(&self, x: i64, y: i64) -> Result<u8> {
        self.raster.get_pixel(dim(x)?, dim(y)?)
    }
    /// Write a label at a top-left coordinate.
    pub fn setpixel(&mut self, x: i64, y: i64, v: i64) -> Result<&mut Self> {
        if !(0..=2).contains(&v) {
            return Err(invalid("pixel must be 0, 1 or 2"));
        }
        self.raster.set_pixel(dim(x)?, dim(y)?, v as u8)?;
        Ok(self)
    }
    /// Flip horizontally.
    pub fn fliph(&mut self) -> Result<&mut Self> {
        self.raster = self.raster.flip_horizontal()?;
        Ok(self)
    }
    /// Flip vertically.
    pub fn flipv(&mut self) -> Result<&mut Self> {
        self.raster = self.raster.flip_vertical()?;
        Ok(self)
    }
    /// Rotate clockwise by a multiple of 90 degrees.
    pub fn rotate(&mut self, angle: i64) -> Result<&mut Self> {
        *self = Self::from_raster(
            self.raster
                .rotate(i32::try_from(angle).map_err(|_| invalid("angle too large"))?)?,
        );
        Ok(self)
    }
    /// Invert zero/nonzero labels.
    pub fn invert(&mut self) -> &mut Self {
        let _ = self.raster.invert_with(1);
        self
    }
    /// Shift content while keeping dimensions.
    pub fn shift(&mut self, x: i64, y: i64) -> Result<&mut Self> {
        self.raster = self.raster.shift(x, y)?;
        Ok(self)
    }
    /// Bottom-left ink bounding box.
    pub fn bbox(&self) -> Bounds {
        self.raster.bbox()
    }
    /// Trim blank outer rows and columns.
    pub fn trim(&mut self) -> Result<&mut Self> {
        *self = Self::from_raster(self.raster.trim()?);
        Ok(self)
    }
    /// Widen by overlaying n copies to the right.
    pub fn bold(&mut self, n: i64) -> Result<&mut Self> {
        if n < 0 {
            return Err(invalid("n must not be negative"));
        }
        if n > 0 {
            *self = Self::from_raster(self.raster.bold(dim(n)?)?);
        }
        Ok(self)
    }
    /// Shear rows relative to the baseline.
    pub fn italic(&mut self, step: i64, base: i64) -> Result<&mut Self> {
        *self = Self::from_raster(self.raster.italic(step, base)?);
        Ok(self)
    }
    /// Paste using top-left coordinates and nonzero-source compositing.
    pub fn paste(&mut self, other: &Self, x: i64, y: i64) -> &mut Self {
        self.raster.paste(&other.raster, x, y);
        self
    }
    /// Return whether the labelled rows are identical.
    pub fn equals(&self, other: &Self) -> bool {
        self == other
    }
    /// Export labels as Python's data type 0 through 5. Integer rows wider than
    /// 64 bits remain exact decimal JSON numbers rather than floating point.
    pub fn todata(&self, datatype: i64) -> Result<serde_json::Value> {
        use serde_json::{Value, json};
        let rows = self.bindata();
        Ok(match datatype {
            0 => json!(rows.join("\n")),
            1 => json!(rows),
            2 => json!(
                rows.iter()
                    .map(|r| r.bytes().map(|v| v - b'0').collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            ),
            3 => json!(
                rows.iter()
                    .flat_map(|r| r.bytes().map(|v| v - b'0'))
                    .collect::<Vec<_>>()
            ),
            4 | 5 => {
                let mut out = Vec::new();
                for row in self.label_rows() {
                    let s = Self::row_number(row, datatype == 4)?;
                    out.push(if datatype == 4 {
                        json!(s)
                    } else {
                        serde_json::from_str::<Value>(&s).map_err(|e| invalid(&e.to_string()))?
                    });
                }
                Value::Array(out)
            }
            _ => return Err(invalid("unknown bitmap datatype")),
        })
    }
}
impl std::fmt::Display for Bitmap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.bindata()
                .join("\n")
                .replace('0', ".")
                .replace('1', "#")
                .replace('2', "&")
        )
    }
}
pub(crate) fn dim(v: i64) -> Result<u32> {
    u32::try_from(v).map_err(|_| invalid("negative or overflowing dimension"))
}
pub(crate) fn invalid(s: &str) -> Error {
    Error::InvalidInput(s.into())
}
impl Bitmap {
    /// Export Pillow-compatible RGB, RGBA, L or packed 1-bit pixels. A custom
    /// palette maps labels 0/1/2 to equal-size byte records, or single 0/1 bits.
    pub fn tobytes(&self, mode: &str, palette: Option<&[Vec<u8>; 3]>) -> Result<Vec<u8>> {
        if mode == "1" {
            let map = if let Some(p) = palette {
                if p.iter().any(|b| b.len() != 1 || b[0] > 1) {
                    return Err(invalid(
                        "mode 1 requires one binary sample per palette entry",
                    ));
                }
                [p[0][0], p[1][0], p[2][0]]
            } else {
                [1, 0, 0]
            };
            let padded = self.width().div_ceil(8) * 8;
            let mut b = crate::bitmap::Bitmap::<crate::bitmap::Mono>::new(padded, self.height())?;
            for y in 0..self.height() {
                for x in 0..padded {
                    let label = if x < self.width() {
                        self.raster.get_pixel(x, y)?
                    } else {
                        0
                    };
                    b.set_pixel(x, y, map[label as usize])?;
                }
            }
            return Ok(b.as_bytes().to_vec());
        }
        let defaults = match mode {
            "L" => [vec![255], vec![0], vec![127]],
            "RGBA" => [
                vec![255, 255, 255, 0],
                vec![0, 0, 0, 255],
                vec![255, 0, 0, 255],
            ],
            _ => [vec![255, 255, 255], vec![0, 0, 0], vec![255, 0, 0]],
        };
        let p = palette.unwrap_or(&defaults);
        self.raster
            .to_palette_bytes(&[(0, &p[0]), (1, &p[1]), (2, &p[2])])
    }
}
impl Bitmap {
    /// Python-style developer representation (Display remains the #/. picture).
    pub fn repr(&self) -> String {
        format!("Bitmap(['{}'])", self.bindata().join("',\n        '"))
    }
}
