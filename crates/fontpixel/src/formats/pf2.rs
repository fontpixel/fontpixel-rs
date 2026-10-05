//! GRUB PFF2 sections and continuous MSB-first bitmap records.
mod writer;
use super::w42_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::Bounds;
use crate::model::{
    Codepoint, Font, FontCharset, FontSet, Glyph, Property, PropertyValue, Rational, Vector,
};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::BTreeMap;
pub use writer::{Pf2WriteOptions, write};
/// PF2 currently has one defined, uncompressed reader representation.
#[derive(Debug, Clone, Default)]
pub struct Pf2ReadOptions {}
/// Native PF2 registry codec.
#[derive(Debug, Clone, Default)]
pub struct Pf2Codec {
    /// Reader configuration.
    pub read_options: Pf2ReadOptions,
    /// Bounded writer configuration.
    pub write_options: Pf2WriteOptions,
}
impl FormatDetector for Pf2Codec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for Pf2Codec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let (font, mut report) = parse(
            c.bytes(),
            c.limits(),
            c.retained_input_bytes(),
            c.name().map_or(0, str::len),
        )?;
        for e in &mut report.entries {
            e.location.asset = c.name().map(str::to_owned);
        }
        let mut set = FontSet::from(font);
        set.source_format = Some("pf2".into());
        Ok((set, report))
    }
}
impl FormatWriter<FontSet> for Pf2Codec {
    fn write(&self, set: &FontSet, o: &WriteOptions) -> Result<Written> {
        self.write_charged(set, o, super::ModelCharge::Estimate)
    }
}
impl Pf2Codec {
    /// [`write`](FormatWriter::write) with the model reserved by the caller.
    pub(crate) fn write_reserved(&self, set: &FontSet, o: &WriteOptions) -> Result<Written> {
        self.write_charged(set, o, super::ModelCharge::Reserved)
    }
    fn write_charged(
        &self,
        set: &FontSet,
        o: &WriteOptions,
        charge: super::ModelCharge,
    ) -> Result<Written> {
        let mut d = Diagnostics::new("pf2", &self.write_options.limits);
        container_charged(set, &mut d, charge)?;
        let mut options = self.write_options.clone();
        options.limits.max_total_bytes = options.limits.max_total_bytes.saturating_sub(d.cost());
        let (bytes, r) = match writer::write_charged(set.single()?, &options, charge) {
            Ok(out) => out,
            Err(crate::Error::ReportedErrors(report)) => {
                d.report.extend(report);
                return Err(crate::Error::ReportedErrors(d.report));
            }
            Err(e) => return Err(e),
        };
        d.report.extend(r);
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), bytes),
            report: d.report,
        })
    }
}
/// Recognize the exact mandatory twelve-byte leading section.
pub fn detect(bytes: &[u8]) -> Confidence {
    if bytes.starts_with(b"FILE\0\0\0\x04PFF2") {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
/// Read uncompressed PFF2 with bounded sections, aliases, and exact metrics.
pub fn read(bytes: &[u8], _: &Pf2ReadOptions, l: &ResourceLimits) -> Result<(Font, Report)> {
    check(bytes.len(), l.max_input_bytes, "PF2 input")?;
    parse(bytes, l, bytes.len(), 0)
}
fn u16be(b: &[u8], p: usize) -> Result<u16> {
    let a = b
        .get(p..p.saturating_add(2))
        .ok_or_else(|| invalid("Truncated PF2 u16"))?;
    Ok(u16::from_be_bytes([a[0], a[1]]))
}
fn u32be(b: &[u8], p: usize) -> Result<u32> {
    let a = b
        .get(p..p.saturating_add(4))
        .ok_or_else(|| invalid("Truncated PF2 u32"))?;
    Ok(u32::from_be_bytes([a[0], a[1], a[2], a[3]]))
}
fn text_value(bytes: &[u8]) -> Result<String> {
    let bytes = bytes.strip_suffix(&[0]).unwrap_or(bytes);
    if bytes.contains(&0) || !bytes.is_ascii() {
        return Err(invalid(
            "PF2 name must be an ASCII string with optional final NUL",
        ));
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| invalid("PF2 name is not ASCII"))
}
fn parse(
    bytes: &[u8],
    l: &ResourceLimits,
    retained: usize,
    asset_len: usize,
) -> Result<(Font, Report)> {
    if detect(bytes) != Confidence::Certain {
        return Err(invalid("Missing PF2 FILE/PFF2 signature"));
    }
    let mut budget = Budget::new(retained, 0, l)?;
    budget.add(1024)?;
    let mut d = Diagnostics::new("pf2", l);
    let mut f = Font::default();
    f.source_format = Some("pf2".into());
    f.charset = FontCharset {
        name: Some("unicode".into()),
        ..Default::default()
    };
    let mut pos: usize = 12;
    let mut index = None;
    let data_start;
    let mut known = std::collections::BTreeSet::new();
    loop {
        let head = bytes
            .get(pos..pos.saturating_add(8))
            .ok_or_else(|| invalid("PF2 missing DATA section"))?;
        let tag = &head[..4];
        let size = u32be(head, 4)? as usize;
        pos += 8;
        if tag == b"DATA" {
            if size != u32::MAX as usize {
                return Err(invalid("PF2 DATA must use sentinel length"));
            }
            data_start = pos;
            break;
        }
        check(size, l.max_table_bytes, "PF2 section")?;
        let body = bytes
            .get(
                pos..pos
                    .checked_add(size)
                    .ok_or_else(|| invalid("PF2 section overflow"))?,
            )
            .ok_or_else(|| invalid("Truncated PF2 section"))?;
        if matches!(
            tag,
            b"FILE" | b"NAME" | b"FAMI" | b"PTSZ" | b"ASCE" | b"DESC" | b"CHIX" | b"MAXW" | b"MAXH"
        ) && !known.insert(tag.to_vec())
        {
            return Err(invalid("Duplicate singleton PF2 section"));
        }
        match tag {
            b"FILE" => return Err(invalid("PF2 FILE section must occur once at the start")),
            b"NAME" => {
                budget.add(body.len() * 2)?;
                f.names.full = Some(text_value(body)?);
            }
            b"FAMI" => {
                budget.add(body.len() * 2)?;
                f.names.family = Some(text_value(body)?);
            }
            b"PTSZ" | b"ASCE" | b"DESC" => {
                if size != 2 {
                    return Err(invalid("PF2 numeric section must be two bytes"));
                }
                let n = i64::from(u16be(body, 0)?);
                match tag {
                    b"PTSZ" => f.metrics.point_size = Some(Rational::integer(n)),
                    b"ASCE" => f.metrics.ascent = Some(n),
                    _ => f.metrics.descent = Some(n),
                }
            }
            b"CHIX" => {
                if !size.is_multiple_of(9) {
                    return Err(invalid("PF2 CHIX size must be a multiple of nine"));
                }
                index = Some(body);
            }
            _ => {
                if matches!(tag, b"MAXW" | b"MAXH") && size != 2 {
                    return Err(invalid("PF2 maximum dimension must be two bytes"));
                }
                budget.add(size.saturating_mul(2).saturating_add(256))?;
                f.headers.push(Property::new(
                    section_key(tag),
                    PropertyValue::Bytes(body.to_vec()),
                ));
            }
        }
        pos += size;
    }
    let index = index.ok_or_else(|| invalid("PF2 has no CHIX"))?;
    let count = index.len() / 9;
    check(count, l.max_glyphs, "PF2 mappings")?;
    budget.add(count.saturating_mul(2048))?;
    let mut by_offset: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut previous = None;
    for rec in index.as_chunks::<9>().0 {
        let cp = u32be(rec, 0)?;
        if char::from_u32(cp).is_none() && !joined_code(cp) {
            return Err(invalid("PF2 code is not a Unicode scalar"));
        }
        if joined_code(cp) {
            d.emit(
                "read.joining_variant",
                crate::report::Impact::None,
                None,
                Some(cp),
                "GRUB joined-form source number retained; this is not a Unicode scalar",
            )?;
        }
        if !seen.insert(cp) {
            return Err(invalid("Duplicate PF2 Unicode code"));
        }
        if previous.is_some_and(|p| p > cp) {
            d.emit(
                "read.unsorted",
                crate::report::Impact::Repair,
                None,
                Some(cp),
                "CHIX entries were not sorted",
            )?;
        }
        previous = Some(cp);
        if rec[4] != 0 {
            return Err(invalid(
                "PF2 compressed or reserved storage flags are unsupported",
            ));
        }
        let off = u32be(rec, 5)? as usize;
        if off < data_start {
            return Err(invalid("PF2 glyph offset precedes DATA"));
        }
        by_offset.entry(off).or_default().push(cp);
    }
    let mut previous_end = data_start;
    for (off, mut cps) in by_offset {
        if off < previous_end {
            return Err(invalid("Overlapping PF2 glyph records"));
        }
        let record = bytes
            .get(off..off.saturating_add(10))
            .ok_or_else(|| invalid("Truncated PF2 glyph header"))?;
        let w = u32::from(u16be(record, 0)?);
        let h = u32::from(u16be(record, 2)?);
        let pixels = w as usize * h as usize;
        check(pixels, l.max_pixels_per_glyph, "PF2 glyph pixels")?;
        let size = pixels.div_ceil(8);
        let raw = bytes
            .get(
                off + 10
                    ..(off + 10)
                        .checked_add(size)
                        .ok_or_else(|| invalid("PF2 bitmap overflow"))?,
            )
            .ok_or_else(|| invalid("Truncated PF2 bitmap"))?;
        let decoded = (w as usize).div_ceil(8) * h as usize;
        budget.add(decoded.saturating_mul(3).saturating_add(cps.len() * 16))?;
        let bitmap = unpack(raw, w, h, l)?;
        let mut g = Glyph::new(bitmap);
        cps.sort_unstable();
        g.codepoints = cps.into_iter().map(Codepoint::Unicode).collect();
        g.metrics.bounds = Bounds::new(
            w,
            h,
            i64::from(u16be(record, 4)? as i16),
            i64::from(u16be(record, 6)? as i16),
        );
        g.metrics.advance = Some(Vector::new(i64::from(u16be(record, 8)? as i16), 0));
        if !pixels.is_multiple_of(8)
            && raw
                .last()
                .is_some_and(|b| b & ((1 << (8 - pixels % 8)) - 1) != 0)
        {
            d.loss(
                "read.padding",
                Some(f.len()),
                g.codepoints.first().map(|c| c.value()),
                "Nonzero final bitmap padding was cleared",
            )?;
        }
        if off > previous_end {
            d.loss(
                "read.data_gap",
                Some(f.len()),
                None,
                format!(
                    "Discarded {} bytes between glyph records",
                    off - previous_end
                ),
            )?;
        }
        f.add_glyph(g)?;
        previous_end = off + 10 + size;
    }
    if previous_end < bytes.len() {
        d.loss(
            "read.trailing",
            None,
            None,
            format!(
                "Discarded {} unreferenced DATA bytes",
                bytes.len() - previous_end
            ),
        )?;
    }
    budget.add(
        d.cost()
            .saturating_add(d.report.entries.len().saturating_mul(asset_len)),
    )?;
    normalize_metrics(&mut f)?;
    Ok((f, d.report))
}
fn section_key(tag: &[u8]) -> String {
    let mut s = String::from("pf2.section.");
    for b in tag {
        use std::fmt::Write;
        let _ = write!(s, "{b:02X}");
    }
    s
}

fn normalize_metrics(f: &mut Font) -> Result<()> {
    f.update_bounds()?;
    let b = f.metrics.bounds.unwrap_or_default();
    let height = f
        .metrics
        .point_size
        .map(|p| p.numerator() / i64::from(p.denominator()))
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(b.height);
    f.metrics.pixel_size = Some(height);
    if f.metrics.point_size.is_none() {
        f.metrics.point_size = Some(Rational::integer(i64::from(height)));
    }
    f.metrics.resolution = Some(Vector::new(72, 72));
    if f.metrics.ascent.is_none() {
        f.metrics.ascent = Some((b.y + i64::from(b.height)).max(0));
    }
    if f.metrics.descent.is_none() {
        f.metrics.descent = Some((-b.y).max(0));
    }
    if f.names.full.is_none() {
        f.names.full = Some("Unknown".into());
    }
    for (tag, n) in [
        (
            b"MAXW",
            f.glyphs().map(|(_, g)| g.bitmap.width()).max().unwrap_or(0),
        ),
        (
            b"MAXH",
            f.glyphs()
                .map(|(_, g)| g.bitmap.height())
                .max()
                .unwrap_or(0),
        ),
    ] {
        let key = section_key(tag);
        if f.headers.get(&key).is_none() {
            f.headers.push(Property::new(
                key,
                PropertyValue::Bytes((n as u16).to_be_bytes().to_vec()),
            ));
        }
    }
    Ok(())
}
