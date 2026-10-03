//! PostScript common header and explicit named/nameless serialization.
use super::{Limits, buffer, data::POST_NAMES, invalid, p16, p32, pi16, to16};
use crate::Result;
use read_fonts::{FontData, FontRead, tables::post::Post};
use std::collections::BTreeMap;
/// PostScript table header (version is selected by `PostTable::names`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    /// Italic angle as signed 16.16 fixed bits.
    pub italic_angle: i32,
    /// Top of underline, in font units (OpenType post convention).
    pub underline_position: i16,
    /// Underline thickness, in font units.
    pub underline_thickness: i16,
    /// Nonzero means monospaced (original numeric value retained).
    pub is_fixed_pitch: u32,
    /// Minimum Type 42 memory usage (normally zero).
    pub min_mem_type42: u32,
    /// Maximum Type 42 memory usage (normally zero).
    pub max_mem_type42: u32,
    /// Minimum Type 1 memory usage (normally zero).
    pub min_mem_type1: u32,
    /// Maximum Type 1 memory usage (normally zero).
    pub max_mem_type1: u32,
}
/// Editable post data. Omission of names explicitly requests format 3.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PostTable {
    /// Shared header fields.
    pub header: Header,
    /// One name per glyph for format 2, or `None` for nameless format 3.
    pub names: Option<Vec<String>>,
}
fn valid_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 255
        && s.bytes()
            .all(|b| (33..=126).contains(&b) && !b"[](){}<>/%".contains(&b))
}
impl PostTable {
    /// Read formats 1, 2 and 3. Historical format 2.5/4 require raw preservation.
    pub fn decode(data: &[u8], num_glyphs: u16, limits: &Limits) -> Result<Self> {
        limits.table(data.len())?;
        limits.glyphs(num_glyphs as usize)?;
        super::bytes(data, 0, 32)?;
        let t = Post::read(FontData::new(data)).map_err(invalid)?;
        let version = super::u32_at(data, 0)?;
        if ![0x10000, 0x20000, 0x30000].contains(&version) {
            return Err(invalid("unsupported post version"));
        }
        let h = Header {
            italic_angle: t.italic_angle().to_bits(),
            underline_position: t.underline_position().into(),
            underline_thickness: t.underline_thickness().into(),
            is_fixed_pitch: t.is_fixed_pitch(),
            min_mem_type42: t.min_mem_type42(),
            max_mem_type42: t.max_mem_type42(),
            min_mem_type1: t.min_mem_type1(),
            max_mem_type1: t.max_mem_type1(),
        };
        let names = if version == 0x30000 {
            None
        } else {
            if (version == 0x20000 && t.num_glyphs() != Some(num_glyphs))
                || (version == 0x10000 && num_glyphs as usize > POST_NAMES.len())
            {
                return Err(invalid("post glyph count differs from maxp"));
            }
            let mut names = Vec::new();
            let mut total = 0;
            for n in t.glyph_names().take(num_glyphs as usize) {
                let (gid, s) = n;
                if gid.to_u32() as usize != names.len() {
                    return Err(invalid("invalid post name index or string"));
                }
                if !valid_name(s) {
                    return Err(invalid("invalid PostScript glyph name"));
                }
                total += s.len();
                limits.table(total)?;
                names.push(s.to_string());
            }
            if names.len() != num_glyphs as usize {
                return Err(invalid("truncated post names"));
            }
            Some(names)
        };
        Ok(Self { header: h, names })
    }
    /// Encode format 2 with shared standard/custom names, or explicit format 3.
    pub fn encode(&self, num_glyphs: u16, limits: &Limits) -> Result<Vec<u8>> {
        limits.glyphs(num_glyphs as usize)?;
        let mut strings = Vec::<&str>::new();
        let mut indices = Vec::new();
        let mut used = BTreeMap::new();
        let mut size = 32;
        if let Some(names) = &self.names {
            if names.len() != num_glyphs as usize {
                return Err(invalid("post name count differs from maxp"));
            }
            size += 2 + names.len() * 2;
            limits.table(size)?;
            for name in names {
                if !valid_name(name)
                    || name.len() > 63
                    || !name
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_')
                {
                    return Err(invalid("invalid PostScript glyph name"));
                }
                let idx = if let Some(i) = POST_NAMES.iter().position(|s| s == name) {
                    i as u16
                } else if let Some(&i) = used.get(name.as_str()) {
                    i
                } else {
                    let idx = to16(258 + strings.len())?;
                    used.insert(name.as_str(), idx);
                    strings.push(name);
                    size += 1 + name.len();
                    limits.table(size)?;
                    idx
                };
                indices.push(idx);
            }
        }
        limits.table(size)?;
        let mut out = buffer(size)?;
        p32(
            &mut out,
            if self.names.is_some() {
                0x20000
            } else {
                0x30000
            },
        );
        let h = &self.header;
        p32(&mut out, h.italic_angle as u32);
        pi16(&mut out, h.underline_position);
        pi16(&mut out, h.underline_thickness);
        for n in [
            h.is_fixed_pitch,
            h.min_mem_type42,
            h.max_mem_type42,
            h.min_mem_type1,
            h.max_mem_type1,
        ] {
            p32(&mut out, n);
        }
        if self.names.is_some() {
            p16(&mut out, num_glyphs);
            for i in indices {
                p16(&mut out, i);
            }
            for s in strings {
                out.push(s.len() as u8);
                out.extend(s.as_bytes());
            }
        }
        Ok(out)
    }
}
