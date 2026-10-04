use crate::bitmap::BitmapLimits;
use crate::model::{Font, PropertyValue};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::{Error, Report, ResourceLimits, Result};
pub(crate) fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(s.into())
}
pub(crate) fn check(n: usize, max: usize, resource: &'static str) -> Result<()> {
    if n > max {
        Err(Error::ResourceLimit {
            resource,
            limit: max,
        })
    } else {
        Ok(())
    }
}
pub(crate) fn slice(b: &[u8], at: usize, n: usize) -> Result<&[u8]> {
    b.get(
        at..at
            .checked_add(n)
            .ok_or_else(|| invalid("Windows font offset overflow"))?,
    )
    .ok_or_else(|| invalid("truncated Windows font range"))
}
pub(crate) fn get16(b: &[u8], at: usize) -> Result<u16> {
    let s = slice(b, at, 2)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}
pub(crate) fn get32(b: &[u8], at: usize) -> Result<u32> {
    let s = slice(b, at, 4)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
pub(crate) fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
pub(crate) fn put32(b: &mut [u8], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
pub(crate) fn word(v: i64, field: &str) -> Result<u16> {
    u16::try_from(v).map_err(|_| invalid(format!("{field} is outside FNT u16 range")))
}
pub(crate) fn size32(v: usize) -> Result<u32> {
    u32::try_from(v).map_err(|_| invalid("Windows font size exceeds u32"))
}
pub(crate) fn loss(
    r: &mut Report,
    code: &str,
    index: Option<usize>,
    cp: Option<u32>,
    message: impl Into<String>,
) {
    diag(r, code, Impact::Loss, index, cp, message);
}
pub(crate) fn diag(
    r: &mut Report,
    code: &str,
    impact: Impact,
    index: Option<usize>,
    cp: Option<u32>,
    message: impl Into<String>,
) {
    r.push(ReportEntry {
        level: Level::Warning,
        impact,
        code: format!("fnt.{code}"),
        location: Location {
            font_index: Some(0),
            glyph_index: index,
            codepoint: cp,
            asset: None,
        },
        message: message.into(),
    });
}
pub(crate) struct Budget {
    pub(crate) used: usize,
    pub(crate) glyphs: usize,
    max: usize,
}
impl Budget {
    pub(crate) fn new(input: usize, l: &ResourceLimits) -> Result<Self> {
        let mut b = Self {
            used: 0,
            glyphs: 0,
            max: l.max_total_bytes,
        };
        b.add(input)?;
        Ok(b)
    }
    pub(crate) fn add(&mut self, n: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(n)
            .ok_or_else(|| invalid("Windows font budget overflow"))?;
        check(self.used, self.max, "Windows font aggregate bytes")
    }
    pub(crate) fn glyph(&mut self, n: usize, l: &ResourceLimits) -> Result<()> {
        self.glyphs = self
            .glyphs
            .checked_add(1)
            .ok_or_else(|| invalid("FNT glyph count overflow"))?;
        check(self.glyphs, l.max_glyphs, "Windows font aggregate glyphs")?;
        self.add(n + std::mem::size_of::<crate::Glyph>() + 160)
    }
}
pub(crate) fn bitmap_limits(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_total_bytes,
        max_dimension: u32::MAX,
    }
}
pub(crate) fn bitmap_size(w: u32, h: u32, l: &ResourceLimits) -> Result<usize> {
    let pixels = u64::from(w) * u64::from(h);
    if pixels > l.max_pixels_per_glyph as u64 {
        return Err(Error::ResourceLimit {
            resource: "FNT glyph pixels",
            limit: l.max_pixels_per_glyph,
        });
    }
    let n = usize::try_from(u64::from(w).div_ceil(8) * u64::from(h))
        .map_err(|_| invalid("FNT bitmap size overflow"))?;
    check(n, l.max_table_bytes, "FNT bitmap table")?;
    Ok(n)
}
pub(crate) fn integer(f: &Font, key: &str, default: i64) -> Result<i64> {
    match f.properties.get(key) {
        None => Ok(default),
        Some(PropertyValue::Integer(n)) => Ok(*n),
        Some(PropertyValue::Unsigned(n)) => {
            i64::try_from(*n).map_err(|_| invalid(format!("{key} exceeds i64")))
        }
        _ => Err(invalid(format!("{key} requires an integer"))),
    }
}
pub(crate) fn text_property<'a>(f: &'a Font, key: &str) -> Result<Option<&'a str>> {
    match f.properties.get(key) {
        None => Ok(None),
        Some(PropertyValue::Text(s)) => Ok(Some(s)),
        _ => Err(invalid(format!("{key} requires text"))),
    }
}
pub(crate) const SCALARS: &[(&str, usize, usize)] = &[
    ("fnt.dfType", 66, 2),
    ("fnt.dfInternalLeading", 76, 2),
    ("fnt.dfItalic", 80, 1),
    ("fnt.dfUnderline", 81, 1),
    ("fnt.dfStrikeOut", 82, 1),
    ("fnt.dfWeight", 83, 2),
    ("fnt.dfCharSet", 85, 1),
    ("fnt.dfPitchAndFamily", 90, 1),
    ("fnt.dfAvgWidth", 91, 2),
    ("fnt.dfMaxWidth", 93, 2),
    ("fnt.dfBreakChar", 98, 1),
    ("fnt.dfFirstChar", 95, 1),
    ("fnt.dfLastChar", 96, 1),
    ("fnt.dfBitsPointer", 109, 4),
];
pub(crate) fn decode_text(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}
pub(crate) fn cstr<'a>(b: &'a [u8], at: usize, l: &ResourceLimits) -> Result<&'a [u8]> {
    if at == 0 {
        return Ok(&[]);
    }
    let s = b
        .get(at..)
        .ok_or_else(|| invalid("invalid FNT string offset"))?;
    let n = s
        .iter()
        .take(l.max_table_bytes.saturating_add(1))
        .position(|&c| c == 0)
        .ok_or_else(|| invalid("unterminated or oversized FNT string"))?;
    check(n, l.max_table_bytes, "FNT string bytes")?;
    Ok(&s[..n])
}

// Charge metadata and potential diagnostics before iterating user-built models.
// Recursive PropertyValue trees are bounded independently of the input's origin.
pub(crate) fn model_cost(f: &Font, l: &ResourceLimits) -> Result<usize> {
    fn value(v: &PropertyValue, depth: usize, b: &mut Budget, l: &ResourceLimits) -> Result<()> {
        if depth > 32 {
            return Err(invalid("FNT source property nesting exceeds 32"));
        }
        b.add(64)?;
        match v {
            PropertyValue::Text(s) | PropertyValue::Decimal(s) => {
                check(s.len(), l.max_table_bytes, "FNT source property bytes")?;
                b.add(s.len())?;
            }
            PropertyValue::Bytes(s) => {
                check(s.len(), l.max_table_bytes, "FNT source property bytes")?;
                b.add(s.len())?;
            }
            PropertyValue::List(items) => {
                check(
                    items.len(),
                    l.max_table_bytes / 8,
                    "FNT source property elements",
                )?;
                for v in items {
                    value(v, depth + 1, b, l)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn props(p: &crate::model::Properties, b: &mut Budget, l: &ResourceLimits) -> Result<()> {
        for p in p.iter() {
            b.add(512 + p.name.len().saturating_mul(6))?;
            value(&p.value, 0, b, l)?;
            if let Some(raw) = &p.raw {
                b.add(raw.len())?;
            }
        }
        Ok(())
    }
    let mut b = Budget::new(std::mem::size_of::<Font>(), l)?;
    for s in [
        &f.names.family,
        &f.names.style,
        &f.names.full,
        &f.names.postscript,
        &f.names.version,
        &f.names.copyright,
        &f.names.license,
        &f.names.license_url,
        &f.names.foundry,
        &f.names.xlfd,
        &f.charset.name,
        &f.charset.registry,
        &f.charset.encoding,
    ]
    .into_iter()
    .flatten()
    {
        check(s.len(), l.max_table_bytes, "FNT source name bytes")?;
        b.add(s.len())?;
    }
    for s in &f.names.xlfd_fields {
        b.add(s.len())?;
    }
    props(&f.names.localized, &mut b, l)?;
    props(&f.headers, &mut b, l)?;
    props(&f.properties, &mut b, l)?;
    for (_, g) in f.glyphs() {
        b.add(std::mem::size_of::<crate::Glyph>() + 4096)?;
        b.add(g.bitmap.as_bytes().len().saturating_mul(3))?;
        if let Some(name) = &g.name {
            b.add(name.len())?;
        }
        b.add(g.codepoints.len().saturating_mul(1024))?;
        for seq in &g.unicode_sequences {
            b.add(seq.len().saturating_mul(4) + 64)?;
        }
        props(&g.properties, &mut b, l)?;
    }
    Ok(b.used)
}
