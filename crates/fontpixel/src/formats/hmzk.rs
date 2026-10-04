//! Native fixed-cell HMZK device fonts.
use super::device_common::*;
use super::{Confidence, WriteOptions};
use crate::model::{Codepoint, Font, FontSet, Property, PropertyValue, Spacing};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
/// HMZK has one native wire layout.
#[derive(Debug, Clone, Default)]
pub struct HmzkReadOptions {}
/// Explicit coverage reduction and cumulative output limits.
#[derive(Debug, Clone)]
pub struct HmzkWriteOptions {
    /// Grayscale-to-monochrome threshold; absent means no implicit conversion.
    pub gray_threshold: Option<u8>,
    /// Cumulative source, scratch, output and readback budget.
    pub limits: ResourceLimits,
    /// Additional primary output byte cap.
    pub max_output_bytes: usize,
}
impl Default for HmzkWriteOptions {
    fn default() -> Self {
        Self {
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
codec!(HmzkCodec, HmzkReadOptions, HmzkWriteOptions);
/// Identify the native magic.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(b"HMZK") {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
/// Decode the directory and every fixed-size cell.
pub fn read(c: &mut ReadContext<'_>, _: &HmzkReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), c.limits(), &mut budget)
}
fn parse(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    if !b.starts_with(b"HMZK") {
        return Err(invalid("invalid HMZK signature"));
    }
    let dir = usize::from(r16(b, 14)?);
    if dir % 2 != 0 {
        return Err(invalid("odd HMZK directory length"));
    }
    let n = dir / 2;
    if b.len() != 16 + dir + n * 32 {
        return Err(invalid("HMZK length does not match directory"));
    }
    check(b.len(), l.max_table_bytes, "HMZK table bytes")?;
    check(256, l.max_pixels_per_glyph, "HMZK glyph pixels")?;
    allocation(n, l, budget)?;
    budget.add(n * 32 + 10)?;
    let mut f = Font::default();
    f.properties.push(Property::new(
        "hmzk.header",
        PropertyValue::Bytes(b[4..14].to_vec()),
    ));
    f.metrics.ascent = Some(16);
    f.metrics.descent = Some(0);
    f.metrics.pixel_size = Some(16);
    f.metrics.line_height = Some(16);
    f.metrics.spacing = Some(Spacing::Monospaced);
    for i in 0..n {
        let cp = u32::from(r16(b, 16 + i * 2)?);
        if char::from_u32(cp).is_none() || f.glyph_id(Codepoint::Unicode(cp)).is_some() {
            return Err(invalid("invalid or duplicate HMZK character"));
        }
        let mut g = glyph(
            16,
            16,
            0,
            0,
            16,
            &b[16 + dir + i * 32..16 + dir + (i + 1) * 32],
            l,
        )?;
        g.codepoints.push(Codepoint::Unicode(cp));
        f.add_glyph(g)?;
    }
    Ok((set(f, "hmzk"), Report::default()))
}
/// Write canonical fixed cells and audit every decoded model field.
pub fn write(s: &FontSet, o: &WriteOptions, v: &HmzkWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let map = select(f, 0xffff, true, false, &mut budget)?;
        check(map.len(), 32767, "HMZK directory entries")?;
        check(map.len(), v.limits.max_glyphs, "HMZK output glyphs")?;
        let mut b = output(
            16 + map.len() * 34,
            v.max_output_bytes,
            &v.limits,
            &mut budget,
        )?;
        b[..4].copy_from_slice(b"HMZK");
        match f.properties.get("hmzk.header") {
            Some(PropertyValue::Bytes(h)) if h.len() == 10 => b[4..14].copy_from_slice(h),
            None => {
                b[4..14].fill(255);
                b[4] = 1;
            }
            _ => return Err(invalid("hmzk.header must contain ten bytes")),
        }
        p16(&mut b, 14, (map.len() * 2) as u16);
        for (i, (cp, g)) in map.iter().enumerate() {
            p16(&mut b, 16 + i * 2, *cp as u16);
            let raster = canvas(g, 16, 16, 16, &v.limits, &mut budget)?;
            let at = 16 + map.len() * 2 + i * 32;
            b[at..at + 32].copy_from_slice(&raster);
        }
        let (target, r) = parse(&b, &v.limits, &mut budget)?;
        report.extend(r);
        audit(s, &target, "hmzk", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), b))
    })();
    finish(result, report, "hmzk", o.strict)
}
