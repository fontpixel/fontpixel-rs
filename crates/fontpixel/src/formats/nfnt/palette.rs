use super::support::*;
use crate::{ResourceLimits, Result};

/// Bounded projection of a ColorTable's indexed RGB colors to ink coverage.
pub(super) struct Palette {
    pub samples: [u8; 256],
    pub missing: bool,
}
impl Palette {
    pub fn parse(data: Option<&[u8]>, depth: usize, l: &ResourceLimits) -> Result<Self> {
        let levels = 1usize << depth;
        let mut values = [None; 256];
        if let Some(data) = data {
            check(data.len(), l.max_resource_bytes, "NFNT color table bytes")?;
            check(data.len(), l.max_table_bytes, "NFNT color table bytes")?;
            let flags = u16be(data, 4)?;
            if flags & !0x8000 != 0 {
                return Err(invalid("unsupported fctb flags"));
            }
            let count = usize::from(u16be(data, 6)?.wrapping_add(1));
            if count > levels {
                return Err(invalid("fctb entry count exceeds font depth"));
            }
            slice(data, 8, count * 8)?;
            for i in 0..count {
                let at = 8 + i * 8;
                let index = if flags & 0x8000 != 0 {
                    i
                } else {
                    usize::from(u16be(data, at)?)
                };
                if index >= levels {
                    return Err(invalid("fctb color index exceeds font depth"));
                }
                if values[index].is_some() {
                    return Err(invalid("duplicate fctb color index"));
                }
                let red = u32::from(u16be(data, at + 2)? >> 8);
                let green = u32::from(u16be(data, at + 4)? >> 8);
                let blue = u32::from(u16be(data, at + 6)? >> 8);
                values[index] =
                    Some((255 - (299 * red + 587 * green + 114 * blue + 500) / 1000) as u8);
            }
        }
        let missing = values[..levels].iter().filter(|v| v.is_none()).count();
        let mut samples = [0; 256];
        let mut rank = 0;
        for i in 0..levels {
            samples[i] = if let Some(value) = values[i] {
                value
            } else {
                let value = if missing > 1 {
                    rank * 255 / (missing - 1)
                } else {
                    0
                };
                rank += 1;
                value as u8
            };
        }
        Ok(Self {
            samples,
            missing: missing > 0,
        })
    }
}
