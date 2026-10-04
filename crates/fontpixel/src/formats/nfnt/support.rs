use crate::bitmap::BitmapLimits;
use crate::model::{FontSet, Properties, PropertyValue};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::{Error, Report, ResourceLimits, Result};

pub(super) fn invalid(s: impl Into<String>) -> Error {
    Error::InvalidInput(format!("NFNT: {}", s.into()))
}
pub(super) fn check(n: usize, max: usize, resource: &'static str) -> Result<()> {
    if n > max {
        Err(Error::ResourceLimit {
            resource,
            limit: max,
        })
    } else {
        Ok(())
    }
}
pub(super) fn slice(b: &[u8], at: usize, n: usize) -> Result<&[u8]> {
    b.get(
        at..at
            .checked_add(n)
            .ok_or_else(|| invalid("offset overflow"))?,
    )
    .ok_or_else(|| invalid("truncated range"))
}
pub(super) fn u16be(b: &[u8], at: usize) -> Result<u16> {
    let s = slice(b, at, 2)?;
    Ok(u16::from_be_bytes([s[0], s[1]]))
}
pub(super) fn u32be(b: &[u8], at: usize) -> Result<u32> {
    let s = slice(b, at, 4)?;
    Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}
pub(super) fn put16(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_be_bytes());
}
pub(super) fn put32(b: &mut [u8], at: usize, n: u32) {
    b[at..at + 4].copy_from_slice(&n.to_be_bytes());
}
pub(super) fn word(n: i64) -> Result<u16> {
    u16::try_from(n).map_err(|_| invalid("metric outside unsigned 16-bit range"))
}
pub(super) fn size(n: usize) -> Result<u32> {
    u32::try_from(n).map_err(|_| invalid("size exceeds 32 bits"))
}
pub(super) fn align(n: usize, a: usize) -> Result<usize> {
    n.checked_add(a - 1)
        .map(|v| v / a * a)
        .ok_or_else(|| invalid("alignment overflow"))
}
pub(super) fn bytes<'a>(p: &'a Properties, key: &str) -> Option<&'a [u8]> {
    match p.get(key) {
        Some(PropertyValue::Bytes(v)) => Some(v),
        _ => None,
    }
}
pub(super) fn number(p: &Properties, key: &str) -> Option<i64> {
    match p.get(key) {
        Some(PropertyValue::Integer(v)) => Some(*v),
        _ => None,
    }
}
pub(super) fn bitmap_limits(l: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_dimension: u32::MAX,
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_table_bytes,
    }
}
pub(super) struct Budget {
    pub used: usize,
    pub glyphs: usize,
    pub resources: usize,
    max: usize,
}
impl Budget {
    pub fn new(n: usize, l: &ResourceLimits) -> Result<Self> {
        check(n, l.max_total_bytes, "NFNT aggregate bytes")?;
        Ok(Self {
            used: n,
            glyphs: 0,
            resources: 0,
            max: l.max_total_bytes,
        })
    }
    pub fn add(&mut self, n: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(n)
            .ok_or_else(|| invalid("budget overflow"))?;
        check(self.used, self.max, "NFNT aggregate bytes")
    }
    pub fn resource(&mut self, n: usize, l: &ResourceLimits) -> Result<()> {
        self.resources = self
            .resources
            .checked_add(n)
            .ok_or_else(|| invalid("resource count overflow"))?;
        check(self.resources, l.max_resources, "NFNT cumulative resources")
    }
    pub fn glyph(&mut self, n: usize, l: &ResourceLimits) -> Result<()> {
        self.glyphs = self
            .glyphs
            .checked_add(1)
            .ok_or_else(|| invalid("glyph count overflow"))?;
        check(self.glyphs, l.max_glyphs, "NFNT glyph count")?;
        self.add(n.saturating_add(1024))
    }
    pub fn diagnostic(
        &mut self,
        r: &mut Report,
        code: &str,
        fi: Option<usize>,
        gi: Option<usize>,
        cp: Option<u32>,
        msg: impl Into<String>,
    ) -> Result<()> {
        let msg = msg.into();
        self.add(256 + code.len() + msg.len())?;
        r.push(ReportEntry {
            level: Level::Warning,
            impact: Impact::Loss,
            code: format!("nfnt.{code}"),
            location: Location {
                font_index: fi,
                glyph_index: gi,
                codepoint: cp,
                asset: None,
            },
            message: msg,
        });
        Ok(())
    }
}
fn value_cost(v: &PropertyValue, b: &mut Budget, depth: usize, l: &ResourceLimits) -> Result<()> {
    if depth > 32 {
        return Err(invalid("property nesting exceeds 32"));
    }
    b.add(64)?;
    match v {
        PropertyValue::Text(s) | PropertyValue::Decimal(s) => {
            check(s.len(), l.max_table_bytes, "NFNT metadata record")?;
            b.add(s.len())
        }
        PropertyValue::Bytes(s) => {
            check(s.len(), l.max_table_bytes, "NFNT metadata record")?;
            b.add(s.len())
        }
        PropertyValue::List(v) => {
            for x in v {
                value_cost(x, b, depth + 1, l)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
pub(super) fn properties_cost(p: &Properties, b: &mut Budget, l: &ResourceLimits) -> Result<()> {
    for p in p.iter() {
        check(p.name.len(), l.max_table_bytes, "NFNT property name")?;
        b.add(64 + p.name.len())?;
        if let Some(raw) = &p.raw {
            b.add(raw.len())?;
        }
        value_cost(&p.value, b, 0, l)?;
    }
    Ok(())
}
pub(super) fn model_cost(set: &FontSet, b: &mut Budget, l: &ResourceLimits) -> Result<()> {
    set.validate(l)?;
    if let Some(format) = &set.source_format {
        b.add(format.len())?;
    }
    properties_cost(&set.properties, b, l)?;
    for f in &set.fonts {
        b.add(2048)?;
        b.add(f.slot_count().saturating_sub(f.len()).saturating_mul(1024))?;
        if let Some(format) = &f.source_format {
            b.add(format.len())?;
        }
        properties_cost(&f.headers, b, l)?;
        properties_cost(&f.properties, b, l)?;
        properties_cost(&f.names.localized, b, l)?;
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
        .chain(f.names.xlfd_fields.iter())
        {
            b.add(s.len())?;
        }
        b.add(f.charset.code_ranges.len().saturating_mul(8))?;
        for (_, g) in f.glyphs() {
            b.glyph(g.bitmap.as_bytes().len(), l)?;
            b.add(g.codepoints.len().saturating_mul(128))?;
            if let Some(s) = &g.name {
                b.add(s.len())?;
            }
            properties_cost(&g.properties, b, l)?;
            for seq in &g.unicode_sequences {
                b.add(128 + seq.len().saturating_mul(12))?;
            }
        }
    }
    Ok(())
}
/// Reject overlapping occupied intervals; adjacency is allowed.
pub(super) fn ranges(ranges: &mut [(usize, usize)]) -> Result<()> {
    ranges.sort_unstable();
    let mut end = 0;
    for &(start, len) in ranges.iter() {
        if len == 0 {
            continue;
        }
        if start < end {
            return Err(invalid("overlapping ranges"));
        }
        end = start
            .checked_add(len)
            .ok_or_else(|| invalid("range overflow"))?;
    }
    Ok(())
}
pub(super) fn crc16(b: &[u8]) -> u16 {
    let mut crc = 0u16;
    for &v in b {
        crc ^= u16::from(v) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}
