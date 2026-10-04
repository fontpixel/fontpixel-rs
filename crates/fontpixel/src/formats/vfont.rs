//! Berkeley vfont, with both byte orders and signed baseline-relative metrics.
use super::legacy_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
pub use crate::bitmap::ByteOrder;
use crate::bitmap::{Bitmap, Bounds, Mono};
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Spacing, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};

/// Explicit vfont input interpretation.
#[derive(Debug, Clone, Default)]
pub struct VfontReadOptions {
    /// Require this integer order; absent detects it from the magic.
    pub byte_order: Option<ByteOrder>,
}
/// Bounded vfont output choices.
#[derive(Debug, Clone)]
pub struct VfontWriteOptions {
    /// Integer byte order, default little endian.
    pub byte_order: ByteOrder,
    /// Permit Unicode numbers as legacy byte positions.
    pub unicode_as_legacy: bool,
    /// Explicit grayscale reduction threshold.
    pub gray_threshold: Option<u8>,
    /// Input, model, table and pixel budgets.
    pub limits: ResourceLimits,
    /// Output cap, additionally bounded by the reader input cap.
    pub max_output_bytes: usize,
}
impl Default for VfontWriteOptions {
    fn default() -> Self {
        Self {
            byte_order: ByteOrder::Little,
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec for Berkeley vfont.
#[derive(Debug, Clone, Default)]
pub struct VfontCodec {
    /// Typed reading choices.
    pub read_options: VfontReadOptions,
    /// Typed writing choices.
    pub write_options: VfontWriteOptions,
}
impl FormatDetector for VfontCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for VfontCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for VfontCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Recognize either byte order of octal magic 0436.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(&[0x1e, 1]) || b.starts_with(&[1, 0x1e]) {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
/// Read one font, retaining source bytes and historical extrema.
pub fn read(c: &mut ReadContext<'_>, o: &VfontReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &VfontReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    slice(b, 0, 2570)?;
    check(b.len(), l.max_table_bytes, "vfont source bytes")?;
    let e = if b.starts_with(&[0x1e, 1]) {
        ByteOrder::Little
    } else if b.starts_with(&[1, 0x1e]) {
        ByteOrder::Big
    } else {
        return Err(invalid("invalid vfont magic"));
    };
    if o.byte_order.is_some_and(|v| v != e) {
        return Err(invalid("vfont byte order disagrees with magic"));
    }
    let size = usize::from(u16_at(b, 2, e)?);
    let data = slice(b, 2570, size)?;
    budget.items::<Font>(1)?;
    budget.add(b.len().saturating_add(1024))?;
    let mut f = Font::default();
    f.source_format = Some("vfont".into());
    for (key, at) in [("vfont.maxx", 4), ("vfont.maxy", 6), ("vfont.xtend", 8)] {
        f.properties
            .push(Property::new(key, i64::from(u16_at(b, at, e)? as i16)));
    }
    f.properties.push(Property::new(
        "vfont.source",
        PropertyValue::Bytes(b.to_vec()),
    ));
    let mut report = Report::default();
    for code in 0..256 {
        let d = slice(b, 10 + code * 10, 10)?;
        let n = i32::from(u16_at(d, 2, e)? as i16);
        if n < 0 {
            return Err(invalid("negative vfont glyph byte length"));
        }
        let up = i64::from(d[4] as i8);
        let down = i64::from(d[5] as i8);
        let left = i64::from(d[6] as i8);
        let right = i64::from(d[7] as i8);
        let advance = i64::from(u16_at(d, 8, e)? as i16);
        if n == 0 && up == 0 && down == 0 && left == 0 && right == 0 && advance == 0 {
            continue;
        }
        check(f.len() + 1, l.max_glyphs, "vfont glyphs")?;
        let w = u32::try_from(left + right).map_err(|_| invalid("negative vfont glyph width"))?;
        let h = u32::try_from(up + down).map_err(|_| invalid("negative vfont glyph height"))?;
        check(
            w as usize * h as usize,
            l.max_pixels_per_glyph,
            "vfont glyph pixels",
        )?;
        let bytes = (w as usize).div_ceil(8) * h as usize;
        if n != 0 && n as usize != bytes {
            return Err(invalid("vfont glyph byte count does not match geometry"));
        }
        budget.items::<Option<Glyph>>(1)?;
        budget.add(256 + bytes * 2)?;
        let bitmap = if n == 0 {
            Bitmap::<Mono>::with_limits(w, h, bitmap_limits(l))?
        } else {
            let raw = slice(data, usize::from(u16_at(d, 0, e)?), bytes)?;
            let bitmap = Bitmap::<Mono>::from_raw(w, h, raw, bitmap_limits(l))?;
            if bitmap.as_bytes() != raw {
                budget.loss(
                    &mut report,
                    "vfont",
                    "read.padding",
                    Some(0),
                    Some(f.len()),
                    "Nonzero row padding is retained only in source bytes",
                )?;
            }
            bitmap
        };
        let mut g = Glyph::new(bitmap);
        g.codepoints.push(Codepoint::Legacy(code as u32));
        g.metrics.bounds = Bounds::new(w, h, -left, -down);
        g.metrics.advance = Some(Vector::new(advance, 0));
        g.metrics.left_bearing = Some(-left);
        g.metrics.right_bearing = Some(advance - right);
        f.add_glyph(g)?;
    }
    derive_metrics(&mut f)?;
    let mut set = FontSet::from(f);
    set.source_format = Some("vfont".into());
    Ok((set, report))
}
fn derive_metrics(f: &mut Font) -> Result<()> {
    let mut bounds: Option<Bounds> = None;
    let mut asc = 0;
    let mut desc = 0;
    let mut advance = None;
    let mut mono = true;
    for (_, g) in f.glyphs() {
        let b = g.metrics.bounds;
        asc = asc.max(b.y + i64::from(b.height));
        desc = desc.max(-b.y);
        bounds = Some(match bounds {
            None => b,
            Some(a) => {
                let x = a.x.min(b.x);
                let y = a.y.min(b.y);
                Bounds::new(
                    u32::try_from((a.x + i64::from(a.width)).max(b.x + i64::from(b.width)) - x)
                        .map_err(|_| invalid("vfont bounds overflow"))?,
                    u32::try_from((a.y + i64::from(a.height)).max(b.y + i64::from(b.height)) - y)
                        .map_err(|_| invalid("vfont bounds overflow"))?,
                    x,
                    y,
                )
            }
        });
        if let Some(a) = advance {
            mono &= g.metrics.advance == Some(a);
        } else {
            advance = g.metrics.advance;
        }
    }
    f.metrics.ascent = Some(asc);
    f.metrics.descent = Some(desc);
    f.metrics.line_height = Some(asc + desc);
    f.metrics.pixel_size = Some((asc + desc) as u32);
    f.metrics.bounds = bounds;
    f.metrics.spacing = Some(if mono {
        Spacing::Monospaced
    } else {
        Spacing::Proportional
    });
    f.metrics.space_width = f
        .glyph_for_codepoint(Codepoint::Legacy(32))
        .and_then(|g| g.metrics.advance.map(|a| a.x));
    Ok(())
}
/// Write one vfont; strict mode rejects any audited model change.
pub fn write(s: &FontSet, o: &WriteOptions, v: &VfontWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let f = converted.single()?;
        let slots = select(
            f,
            255,
            v.unicode_as_legacy,
            "vfont",
            &mut report,
            &mut budget,
        )?;
        let size = slots.values().try_fold(0usize, |n, g| -> Result<_> {
            n.checked_add(mono(g)?.as_bytes().len())
                .ok_or_else(|| invalid("vfont size overflow"))
        })?;
        if size > 65535 {
            return Err(invalid("vfont raster section exceeds u16"));
        }
        let len = 2570 + size;
        check(
            len,
            v.max_output_bytes
                .min(v.limits.max_input_bytes)
                .min(v.limits.max_table_bytes),
            "vfont output bytes",
        )?;
        budget.add(len)?;
        let mut out = vec![0; len];
        let e = v.byte_order;
        put16(&mut out, 0, 0o436, e);
        put16(&mut out, 2, size as u16, e);
        for (key, at, default) in [
            (
                "vfont.maxx",
                4,
                slots.values().map(|g| g.bitmap.width()).max().unwrap_or(0),
            ),
            (
                "vfont.maxy",
                6,
                slots.values().map(|g| g.bitmap.height()).max().unwrap_or(0),
            ),
            ("vfont.xtend", 8, 0),
        ] {
            let val = i16::try_from(int(&f.properties, key, i64::from(default))?)
                .map_err(|_| invalid("vfont header metric exceeds i16"))?;
            put16(&mut out, at, val as u16, e);
        }
        let mut off = 0;
        for (code, g) in slots {
            let b = mono(g)?;
            let bb = g.metrics.bounds;
            let at = 10 + code as usize * 10;
            let n = i16::try_from(b.as_bytes().len())
                .map_err(|_| invalid("vfont glyph bytes exceed i16"))?;
            put16(&mut out, at, if n == 0 { 0 } else { off as u16 }, e);
            put16(&mut out, at + 2, n as u16, e);
            for (i, val) in [
                bb.y + i64::from(bb.height),
                -bb.y,
                -bb.x,
                bb.x + i64::from(bb.width),
            ]
            .into_iter()
            .enumerate()
            {
                out[at + 4 + i] =
                    i8::try_from(val).map_err(|_| invalid("vfont bearing exceeds i8"))? as u8;
            }
            let a = g
                .metrics
                .advance
                .or(f.metrics.glyph_defaults.advance)
                .unwrap_or(Vector::new(i64::from(bb.width), 0));
            put16(
                &mut out,
                at + 8,
                i16::try_from(a.x).map_err(|_| invalid("vfont advance exceeds i16"))? as u16,
                e,
            );
            out[2570 + off..2570 + off + b.as_bytes().len()].copy_from_slice(b.as_bytes());
            off += b.as_bytes().len();
        }
        let (target, rr) = parse(&out, &VfontReadOptions::default(), &v.limits, &mut budget)?;
        report.extend(rr);
        audit(s, &target, "vfont", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "vfont", o.strict)
}
