//! Native Processing PFont VLW versions 8 through 11.
use super::image_bundle_common::*;
use super::{Confidence, WriteOptions};
use crate::model::{Codepoint, Font, FontSet, Property, PropertyValue, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
/// Native VLW wire version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VlwVersion {
    /// Legacy bitmap-only version 8.
    V8,
    /// Legacy bitmap-only version 9.
    V9,
    /// Names use Java modified UTF-8.
    V10,
    /// Names and smooth flag.
    #[default]
    V11,
}
impl VlwVersion {
    fn value(self) -> u32 {
        match self {
            Self::V8 => 8,
            Self::V9 => 9,
            Self::V10 => 10,
            Self::V11 => 11,
        }
    }
}
/// Native reader options.
#[derive(Debug, Clone, Default)]
pub struct VlwReadOptions {}
/// Native writer options.
#[derive(Debug, Clone)]
pub struct VlwWriteOptions {
    /// Output version, default 11.
    pub version: VlwVersion,
    /// Override native smooth flag; otherwise preserve its property.
    pub smooth: Option<bool>,
    /// Cumulative source/output/readback budget.
    pub limits: ResourceLimits,
    /// Additional output cap.
    pub max_output_bytes: usize,
}
impl Default for VlwWriteOptions {
    fn default() -> Self {
        Self {
            version: VlwVersion::V11,
            smooth: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
codec!(VlwCodec, VlwReadOptions, VlwWriteOptions);
/// Recognize the bounded fixed header and directory prefix.
pub fn detect(b: &[u8]) -> Confidence {
    if b.len() >= 24
        && matches!(r32(b, 4), Ok(8..=11))
        && r32(b, 0)
            .is_ok_and(|n| n > 0 && (n as usize).saturating_mul(28).saturating_add(24) <= b.len())
    {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
fn modified_utf(b: &[u8], at: &mut usize, budget: &mut Budget) -> Result<String> {
    let n = usize::from(r16(b, *at)?);
    *at += 2;
    let bytes = slice(b, *at, n)?;
    *at += n;
    budget.add(n.saturating_mul(4))?;
    let mut units = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        let c = bytes[i];
        i += 1;
        let v = match c {
            1..=0x7f => u16::from(c),
            0xc0..=0xdf => {
                let d = *bytes
                    .get(i)
                    .ok_or_else(|| invalid("truncated modified UTF-8"))?;
                i += 1;
                if d & 0xc0 != 0x80 {
                    return Err(invalid("invalid modified UTF-8 continuation"));
                }
                let v = (u16::from(c & 31) << 6) | u16::from(d & 63);
                if v < 128 && !(c == 0xc0 && d == 0x80) {
                    return Err(invalid("overlong modified UTF-8"));
                }
                v
            }
            0xe0..=0xef => {
                let d = slice(bytes, i, 2)?;
                i += 2;
                if d[0] & 0xc0 != 0x80 || d[1] & 0xc0 != 0x80 {
                    return Err(invalid("invalid modified UTF-8 continuation"));
                }
                let v =
                    (u16::from(c & 15) << 12) | (u16::from(d[0] & 63) << 6) | u16::from(d[1] & 63);
                if v < 2048 {
                    return Err(invalid("overlong modified UTF-8"));
                }
                v
            }
            _ => return Err(invalid("invalid Java modified UTF-8 leading byte")),
        };
        units.push(v);
    }
    String::from_utf16(&units).map_err(|_| invalid("unpaired modified UTF-8 surrogate"))
}
fn put_utf(out: &mut Vec<u8>, s: &str) -> Result<()> {
    let n = s
        .encode_utf16()
        .map(|c| match c {
            1..=127 => 1,
            0..=2047 => 2,
            _ => 3,
        })
        .sum::<usize>();
    check(n, 65535, "VLW modified UTF-8 bytes")?;
    out.extend((n as u16).to_be_bytes());
    for c in s.encode_utf16() {
        match c {
            1..=127 => out.push(c as u8),
            0..=2047 => out.extend([0xc0 | (c >> 6) as u8, 0x80 | (c & 63) as u8]),
            _ => out.extend([
                0xe0 | (c >> 12) as u8,
                0x80 | ((c >> 6) & 63) as u8,
                0x80 | (c & 63) as u8,
            ]),
        }
    }
    Ok(())
}
/// Read native directory, coverage and Java strings.
pub fn read(c: &mut ReadContext<'_>, _: &VlwReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), c.limits(), &mut budget)
}
fn parse(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    check(b.len(), l.max_table_bytes, "VLW bytes")?;
    slice(b, 0, 24)?;
    let n = r32(b, 0)? as usize;
    let version = r32(b, 4)?;
    if !(8..=11).contains(&version) || n == 0 {
        return Err(invalid("invalid VLW version or count"));
    }
    allocation(n, l, budget)?;
    let directory = n
        .checked_mul(28)
        .ok_or_else(|| invalid("VLW directory overflow"))?;
    slice(b, 24, directory)?;
    let size = i32be(b, 8)?;
    if size <= 0 {
        return Err(invalid("VLW size must be positive"));
    }
    let mut f = Font::default();
    f.metrics.pixel_size = Some(size as u32);
    let mut ascent = i64::from(i32be(b, 16)?);
    let mut descent = i64::from(i32be(b, 20)?);
    f.properties
        .push(Property::new("vlw.mbox", i64::from(i32be(b, 12)?)));
    let mut at = 24 + directory;
    let mut last = None;
    for i in 0..n {
        let p = 24 + i * 28;
        let cp = r32(b, p)?;
        if char::from_u32(cp).is_none() || last.is_some_and(|c| c >= cp) {
            return Err(invalid(
                "VLW codepoints must be sorted unique Unicode scalars",
            ));
        }
        last = Some(cp);
        let (h, w) = (i32be(b, p + 4)?, i32be(b, p + 8)?);
        if w < 0 || h < 0 {
            return Err(invalid("negative VLW bitmap dimension"));
        }
        let (h, w) = (h as u32, w as u32);
        let pixels = area(w, h)?;
        check(pixels, l.max_pixels_per_glyph, "VLW glyph pixels")?;
        let data = slice(b, at, pixels)?;
        at += pixels;
        let top = i64::from(i32be(b, p + 16)?);
        let x = i64::from(i32be(b, p + 20)?);
        let advance = i64::from(i32be(b, p + 12)?);
        if cp == 100 && ascent == 0 {
            ascent = top
        }
        if cp == 112 && descent == 0 {
            descent = i64::from(h) - top
        }
        let mut g = glyph(
            w,
            h,
            x,
            top - i64::from(h),
            Vector::new(advance, 0),
            data,
            true,
            l,
            budget,
        )?;
        g.codepoints.push(Codepoint::Unicode(cp));
        g.properties
            .push(Property::new("vlw.pointer", i64::from(i32be(b, p + 24)?)));
        f.add_glyph(g)?;
    }
    if ascent == 0 && descent == 0 {
        return Err(invalid("VLW has no usable ascent/descent"));
    }
    f.metrics.ascent = Some(ascent);
    f.metrics.descent = Some(descent);
    if version >= 10 {
        f.names.full = Some(modified_utf(b, &mut at, budget)?);
        f.names.postscript = Some(modified_utf(b, &mut at, budget)?)
    }
    if version == 11 {
        let s = *b
            .get(at)
            .ok_or_else(|| invalid("truncated VLW smooth flag"))?;
        at += 1;
        if s > 1 {
            return Err(invalid("invalid VLW smooth flag"));
        }
        f.properties.push(Property::new(
            "vlw.smooth",
            PropertyValue::Integer(i64::from(s)),
        ));
    }
    if at != b.len() {
        return Err(invalid("trailing VLW bytes"));
    }
    Ok((set(f, "vlw"), Report::default()))
}
/// Write native coverage with deterministic directory order and fidelity audit.
pub fn write(s: &FontSet, o: &WriteOptions, v: &VlwWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        budget.add(o.primary_name.as_str().len().saturating_mul(4))?;
        let f = s.single()?;
        let map = select(f, true, 0x10ffff, false, &mut budget)?;
        check(map.len(), v.limits.max_glyphs, "VLW output glyphs")?;
        let size = f
            .metrics
            .pixel_size
            .ok_or_else(|| invalid("VLW needs positive pixel size"))?;
        if size == 0 || size > i32::MAX as u32 {
            return Err(invalid("invalid VLW size"));
        }
        let ascent = f.metrics.ascent.unwrap_or(0);
        let descent = f.metrics.descent.unwrap_or(0);
        let mut header = [0u8; 24];
        for (i, value) in [
            map.len() as i64,
            i64::from(v.version.value()),
            i64::from(size),
            int(&f.properties, "vlw.mbox", 0)?,
            ascent,
            descent,
        ]
        .into_iter()
        .enumerate()
        {
            let v = i32::try_from(value).map_err(|_| invalid("VLW header exceeds i32"))?;
            header[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
        }
        let mut strings = Vec::new();
        if v.version.value() >= 10 {
            let full = f
                .names
                .full
                .as_deref()
                .or(f.names.family.as_deref())
                .unwrap_or("");
            let ps = f.names.postscript.as_deref().unwrap_or("");
            budget.add(
                full.len()
                    .saturating_add(ps.len())
                    .saturating_mul(6)
                    .saturating_add(4),
            )?;
            put_utf(&mut strings, full)?;
            put_utf(&mut strings, ps)?
        }
        if v.version == VlwVersion::V11 {
            let smooth = if let Some(v) = v.smooth {
                v
            } else {
                match f.properties.get("vlw.smooth") {
                    None => true,
                    Some(PropertyValue::Integer(v @ 0..=1)) => *v != 0,
                    _ => return Err(invalid("vlw.smooth must be 0 or 1")),
                }
            };
            strings.push(u8::from(smooth));
        }
        let payload = map.values().try_fold(0usize, |n, g| {
            n.checked_add(area(g.bitmap.width(), g.bitmap.height())?)
                .ok_or_else(|| invalid("VLW payload overflow"))
        })?;
        let len = 24usize
            .saturating_add(map.len().saturating_mul(28))
            .saturating_add(payload)
            .saturating_add(strings.len());
        let mut out = output(len, v.max_output_bytes, &v.limits, &mut budget)?;
        out[..24].copy_from_slice(&header);
        let mut bitmap_at = 24 + map.len() * 28;
        for (i, (cp, g)) in map.iter().enumerate() {
            let adv = g
                .metrics
                .advance
                .or(f.metrics.glyph_defaults.advance)
                .map_or(i64::from(g.bitmap.width()), |a| a.x);
            let values = [
                i64::from(*cp),
                i64::from(g.bitmap.height()),
                i64::from(g.bitmap.width()),
                adv,
                g.metrics.bounds.y + i64::from(g.bitmap.height()),
                g.metrics.bounds.x,
                int(&g.properties, "vlw.pointer", 0)?,
            ];
            for (j, n) in values.into_iter().enumerate() {
                let value = i32::try_from(n).map_err(|_| invalid("VLW glyph field exceeds i32"))?;
                let at = 24 + i * 28 + j * 4;
                out[at..at + 4].copy_from_slice(&value.to_be_bytes());
            }
            for y in 0..if g.bitmap.width() == 0 {
                0
            } else {
                g.bitmap.height()
            } {
                for x in 0..g.bitmap.width() {
                    out[bitmap_at] = pixel(g, x, y);
                    bitmap_at += 1;
                }
            }
        }
        out[bitmap_at..].copy_from_slice(&strings);
        let (target, r) = parse(&out, &v.limits, &mut budget)?;
        report.extend(r);
        audit(s, &target, "vlw", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "vlw", o.strict)
}
