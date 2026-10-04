//! Native bounded Printshop printer fonts.
use super::historical_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::model::{FontSet, Property, PropertyValue};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
/// Explicit Printshop input interpretation.
#[derive(Debug, Clone, Default)]
pub struct PrintshopReadOptions {
    /// Baseline override; absent uses the documented format convention.
    pub baseline: Option<i64>,
}
/// Bounded Printshop output choices.
#[derive(Debug, Clone)]
pub struct PrintshopWriteOptions {
    /// Explicitly allow Unicode numbers as legacy code positions.
    pub unicode_as_legacy: bool,
    /// Explicit Gray8 reduction threshold.
    pub gray_threshold: Option<u8>,
    /// Input/model/pixel/table budgets shared with rereading.
    pub limits: ResourceLimits,
    /// Output cap, additionally bounded by reader input limits.
    pub max_output_bytes: usize,
}
impl Default for PrintshopWriteOptions {
    fn default() -> Self {
        Self {
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec for Printshop.
#[derive(Debug, Clone, Default)]
pub struct PrintshopCodec {
    /// Typed reading options.
    pub read_options: PrintshopReadOptions,
    /// Typed writing options.
    pub write_options: PrintshopWriteOptions,
}
impl FormatDetector for PrintshopCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for PrintshopCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for PrintshopCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
fn layout(b: &[u8]) -> Result<(usize, usize)> {
    slice(b, 0, 601)?;
    let logo = usize::from(b[590]) * usize::from(b[591]);
    let mut size = 601 + logo;
    for i in 0..95 {
        size += usize::from(b[4 + i]).div_ceil(8) * usize::from(b[99 + i]);
    }
    if size != b.len() {
        return Err(invalid("Print Shop contiguous raster length mismatch"));
    }
    Ok((logo, size))
}
/// Recognize only the documented contiguous-raster structural profile.
pub fn detect(b: &[u8]) -> Confidence {
    if layout(b).is_ok() {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
/// Read the contiguous bitmap profile, retaining header, logo and unknown arrays.
pub fn read(c: &mut ReadContext<'_>, o: &PrintshopReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &PrintshopReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    let (logo, _) = layout(b)?;
    let mut f = source("printshop", b, 95, l, budget)?;
    budget.add(601 + logo)?;
    f.properties.push(Property::new(
        "printshop.header_logo",
        PropertyValue::Bytes(b[..601 + logo].to_vec()),
    ));
    f.names.family = Some(latin(&b[581..590]).trim_end_matches(['\0', ' ']).into());
    let baseline = o.baseline.unwrap_or(i64::from(b[592]));
    if !(-65535..=65535).contains(&baseline) {
        return Err(invalid("Printshop baseline exceeds bounded range"));
    }
    let mut off = 601 + logo;
    for i in 0..95 {
        let w = u32::from(b[4 + i]);
        let h = u32::from(b[99 + i]);
        let n = (w as usize).div_ceil(8) * h as usize;
        let bitmap = raster(w, h, slice(b, off, n)?, l, budget)?;
        glyph(
            &mut f,
            bitmap,
            32 + i as u32,
            0,
            baseline - i64::from(h),
            i64::from(w),
        )?;
        off += n;
    }
    metrics(&mut f, baseline, i64::from(b[592]));
    Ok((set(f, "printshop"), Report::default()))
}
/// Write native contiguous PNF rasters, preserving opaque source header bytes.
pub fn write(s: &FontSet, o: &WriteOptions, v: &PrintshopWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let slots = select(
            f,
            126,
            v.unicode_as_legacy,
            "printshop",
            &mut report,
            &mut budget,
        )?;
        let original = bytes(f, "printshop.header_logo");
        let headlen = original.map_or(601, |b| b.len());
        if headlen < 601 {
            return Err(invalid("Print Shop preserved header too short"));
        }
        let mut size = headlen;
        for (&cp, g) in &slots {
            if cp >= 32 {
                size = size
                    .checked_add(mono(g)?.as_bytes().len())
                    .ok_or_else(|| invalid("Print Shop size overflow"))?;
            }
        }
        output_limit(size, v.max_output_bytes, &v.limits, &mut budget)?;
        let mut out = vec![0; size];
        if let Some(h) = original {
            out[..headlen].copy_from_slice(h);
        }
        out[581..590].fill(0);
        if let Some(name) = &f.names.family {
            if name.chars().count() > 9 {
                return Err(invalid("Print Shop family exceeds nine bytes"));
            }
            for (i, c) in name.chars().enumerate() {
                out[581 + i] = u8::try_from(u32::from(c))
                    .map_err(|_| invalid("Print Shop name is not Latin-1"))?;
            }
        }
        let baseline = f
            .metrics
            .ascent
            .or(f.metrics.line_height)
            .unwrap_or_else(|| {
                slots
                    .values()
                    .map(|g| g.metrics.bounds.y + i64::from(g.bitmap.height()))
                    .max()
                    .unwrap_or(1)
            });
        out[592] = u8::try_from(baseline)
            .map_err(|_| invalid("Print Shop alignment height exceeds u8"))?;
        let keep_offsets = original.is_some_and(|old| {
            (32..127u32).all(|cp| {
                let i = (cp - 32) as usize;
                let g = slots.get(&cp);
                u32::from(old[4 + i]) == g.map_or(0, |g| g.bitmap.width())
                    && u32::from(old[99 + i]) == g.map_or(0, |g| g.bitmap.height())
            })
        });
        if !keep_offsets {
            budget.loss(&mut report,"printshop","write.offset_convention",Some(0),None,"New contiguous layout uses glyph-section-relative offsets; proprietary Printshop offset/kerning semantics are not established")?;
        }
        let mut off = headlen;
        let mut maxw = 0;
        let mut maxh = 0;
        for code in 32..127 {
            let i = (code - 32) as usize;
            let g = slots.get(&code).copied();
            let w = g.map_or(0, |g| g.bitmap.width());
            let h = g.map_or(0, |g| g.bitmap.height());
            out[4 + i] = u8::try_from(w).map_err(|_| invalid("Print Shop width exceeds u8"))?;
            out[99 + i] = u8::try_from(h).map_err(|_| invalid("Print Shop height exceeds u8"))?;
            maxw = maxw.max(out[4 + i]);
            maxh = maxh.max(out[99 + i]);
            if !keep_offsets {
                put16(
                    &mut out,
                    194 + i * 2,
                    u16::try_from(off - headlen)
                        .map_err(|_| invalid("Print Shop raster offset exceeds u16"))?,
                    ByteOrder::Little,
                );
            }
            if let Some(g) = g {
                let raw = mono(g)?.as_bytes();
                out[off..off + raw.len()].copy_from_slice(raw);
                off += raw.len();
            }
        }
        out[0] = maxw;
        out[1] = maxh;
        let (target, rr) = parse(&out, &Default::default(), &v.limits, &mut budget)?;
        report.extend(rr);
        audit(s, &target, "printshop", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "printshop", o.strict)
}
