//! Native Sabriel Bitmap Font directory and packed rasters.
use super::device_common::*;
use super::{Confidence, WriteOptions};
use crate::model::{Codepoint, Font, FontSet, Property};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
/// SBF native positions remain Legacy codepoints.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct SbfReadOptions {}
/// Bounded native SBF output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct SbfWriteOptions {
    /// Permit numerical Unicode-to-native byte mapping.
    pub unicode_as_legacy: bool,
    /// Explicit grayscale-to-monochrome threshold.
    pub gray_threshold: Option<u8>,
    /// Cumulative source, output, scratch and readback limits.
    pub limits: ResourceLimits,
    /// Additional primary output cap.
    pub max_output_bytes: usize,
}
impl Default for SbfWriteOptions {
    fn default() -> Self {
        Self {
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
codec!(SbfCodec, SbfReadOptions, SbfWriteOptions);
const MAGIC: &[u8] = &[0x10, 0xb1, 0x7e, 0x5c];
/// Detect the native four-byte signature.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(MAGIC) {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
/// Read the sparse directory, signed bearings and exact raster widths.
pub fn read(c: &mut ReadContext<'_>, _: &SbfReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), c.limits(), &mut budget)
}
fn parse(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    slice(b, 0, 2048)?;
    if !b.starts_with(MAGIC) {
        return Err(invalid("invalid SBF magic"));
    }
    check(b.len(), l.max_table_bytes, "SBF table bytes")?;
    let count = (1..=255)
        .filter(|i| b[i * 8] != 0 || b[i * 8 + 1] != 0)
        .count();
    allocation(count, l, budget)?;
    let mut report = Report::default();
    let mut f = Font::default();
    f.metrics.ascent = Some(i64::from(b[6]));
    f.metrics.descent = Some(i64::from(b[7]));
    f.metrics.line_height = Some(i64::from(b[5]));
    f.metrics.pixel_size = Some(u32::from(b[5]));
    f.properties
        .push(Property::new("sbf.fixed_advance", i64::from(b[4])));
    f.charset.name = Some("SuperLatin".into());
    for code in 1..=255 {
        let at = code * 8;
        let off = usize::from(r16(b, at)?);
        if off == 0 {
            continue;
        }
        if off < 2048 {
            return Err(invalid("SBF raster overlaps directory"));
        }
        let stride = usize::from(b[at + 2]);
        let h = u32::from(b[at + 3]);
        let overhang = usize::from(b[at + 7]);
        if overhang > 7 || overhang > stride * 8 {
            return Err(invalid("invalid SBF row overhang"));
        }
        let w = (stride * 8 - overhang) as u32;
        check(
            w as usize * h as usize,
            l.max_pixels_per_glyph,
            "SBF glyph pixels",
        )?;
        let data = slice(b, off, stride * h as usize)?;
        budget.add(data.len())?;
        let advance = if b[4] == 0 { b[at + 6] } else { b[4] };
        let mut g = glyph(
            w,
            h,
            i64::from(b[at + 4] as i8),
            i64::from(b[at + 5] as i8) - i64::from(h),
            i64::from(advance),
            data,
            l,
        )?;
        if g.bitmap.as_bytes() != data {
            budget.loss(
                &mut report,
                "sbf",
                "read.padding",
                Some(0),
                Some(code),
                "Nonzero row padding is removed",
            )?;
        }
        if b[4] != 0 {
            g.properties
                .push(Property::new("sbf.record_advance", i64::from(b[at + 6])));
        }
        g.codepoints.push(Codepoint::Legacy(code as u32));
        f.add_glyph(g)?;
    }
    Ok((set(f, "sbf"), report))
}
/// Write the native 255-entry directory and audit native readback.
pub fn write(s: &FontSet, o: &WriteOptions, v: &SbfWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let mut map = select(f, 255, false, v.unicode_as_legacy, &mut budget)?;
        map.remove(&0);
        if map.is_empty() {
            return Err(invalid("SBF has no mappings in 1..255"));
        }
        let mut size = 2048usize;
        for g in map.values() {
            size = size
                .checked_add(mono(g)?.as_bytes().len())
                .ok_or_else(|| invalid("SBF size overflow"))?;
        }
        // The last raster may extend beyond 64 KiB: only its start is u16.
        // If necessary place the largest raster last so every start still fits.
        let largest = map
            .iter()
            .max_by_key(|(_, g)| g.bitmap.as_bytes().len())
            .unwrap();
        check(
            size - largest.1.bitmap.as_bytes().len(),
            65535,
            "SBF glyph offsets",
        )?;
        let last = (size > 65535).then_some(*largest.0);
        let mut b = output(size, v.max_output_bytes, &v.limits, &mut budget)?;
        b[..4].copy_from_slice(MAGIC);
        b[4] = byte(int(&f.properties, "sbf.fixed_advance", 0)?)?;
        b[5] = byte(
            f.metrics
                .line_height
                .or(f.metrics.pixel_size.map(i64::from))
                .unwrap_or(0),
        )?;
        b[6] = byte(f.metrics.ascent.unwrap_or(0))?;
        b[7] = byte(f.metrics.descent.unwrap_or(0))?;
        let mut off = 2048;
        for (code, g) in map
            .iter()
            .filter(|(code, _)| Some(**code) != last)
            .chain(last.and_then(|code| map.get_key_value(&code)))
        {
            let bm = mono(g)?;
            let bb = g.metrics.bounds;
            let at = *code as usize * 8;
            p16(&mut b, at, off as u16);
            b[at + 2] = byte(bm.stride() as i64)?;
            b[at + 3] = byte(i64::from(bm.height()))?;
            b[at + 4] = i8::try_from(bb.x).map_err(|_| invalid("SBF left exceeds i8"))? as u8;
            b[at + 5] = i8::try_from(bb.y + i64::from(bb.height))
                .map_err(|_| invalid("SBF top exceeds i8"))? as u8;
            let adv = g
                .metrics
                .advance
                .or(f.metrics.glyph_defaults.advance)
                .map_or(i64::from(bb.width), |a| a.x);
            b[at + 6] = byte(if b[4] == 0 {
                adv
            } else {
                int(&g.properties, "sbf.record_advance", adv)?
            })?;
            b[at + 7] = (bm.stride() * 8 - bm.width() as usize) as u8;
            b[off..off + bm.as_bytes().len()].copy_from_slice(bm.as_bytes());
            off += bm.as_bytes().len();
        }
        let (target, r) = parse(&b, &v.limits, &mut budget)?;
        report.extend(r);
        audit(s, &target, "sbf", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), b))
    })();
    finish(result, report, "sbf", o.strict)
}
