//! Apple II MouseGraphics Toolkit / MousePaint seven-bit-strip bitmap fonts.
use super::{
    Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions,
    w5d_common::*,
};
use crate::bitmap::Bounds;
use crate::model::{
    Codepoint, Font, FontCharset, FontMetrics, Glyph, GlyphBitmap, Property, PropertyValue,
    Spacing, Vector,
};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Bitmap, FontSet, Report, ResourceLimits, Result};

/// Explicit legacy character interpretation.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct MgtkReadOptions {
    /// Optional charset label; byte codes are not remapped implicitly.
    pub charset: Option<String>,
}
/// Raster storage width.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Columns {
    /// Preserve native width, otherwise choose according to actual glyph widths.
    #[default]
    Auto,
    /// One seven-bit strip, at most seven pixels per glyph.
    Single,
    /// Two seven-bit strips, at most fourteen pixels per glyph.
    Double,
}
/// Bounded native output options.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct MgtkWriteOptions {
    /// One or two seven-bit strips.
    pub columns: Columns,
    /// Explicit Gray8 reduction; absent means Gray8 is rejected.
    pub gray_threshold: Option<u8>,
    /// Cumulative source/output/reread limits.
    pub limits: ResourceLimits,
    /// Maximum encoded output size.
    pub max_output_bytes: usize,
}
impl Default for MgtkWriteOptions {
    fn default() -> Self {
        Self {
            columns: Columns::Auto,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 << 20,
        }
    }
}
/// Registry reader/writer with typed format options.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct MgtkCodec {
    /// Input interpretation.
    pub read_options: MgtkReadOptions,
    /// Output policy.
    pub write_options: MgtkWriteOptions,
}
impl FormatDetector for MgtkCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for MgtkCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c.bytes(), &self.read_options, c.limits())
    }
}
impl FormatWriter<FontSet> for MgtkCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (bytes, report) = write(s, &self.write_options)?;
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}
fn layout(data: &[u8]) -> Result<(usize, usize, usize)> {
    let h = slice(data, 0, 3)?;
    let strips = match h[0] {
        0 => 1,
        0x80 => 2,
        _ => return Err(invalid("MGTK type must be 00 or 80")),
    };
    let count = usize::from(h[1]) + 1;
    let height = usize::from(h[2]);
    if !(1..=16).contains(&height) {
        return Err(invalid("MGTK height must be in 1..=16"));
    }
    let widths = slice(data, 3, count)?;
    if widths.iter().any(|&w| usize::from(w) > strips * 7) {
        return Err(invalid("MGTK width exceeds native strip count"));
    }
    slice(data, 3 + count, count * strips * height)?;
    Ok((count, height, strips))
}
/// Conservative bounded structural recognition; this format has no magic.
pub fn detect(data: &[u8]) -> Confidence {
    if layout(data).is_ok_and(|(n, h, s)| data.len() == 3 + n + n * h * s) {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
/// Decode native seven-bit rows and retain all original bytes.
pub fn read(data: &[u8], o: &MgtkReadOptions, l: &ResourceLimits) -> Result<(FontSet, Report)> {
    check(data.len(), l.max_input_bytes, "MGTK input bytes")?;
    let mut b = Budget::new(l);
    b.add(data.len())?;
    Ok((decode(data, o, &mut b)?, Report::default()))
}
fn decode(data: &[u8], o: &MgtkReadOptions, b: &mut Budget<'_>) -> Result<FontSet> {
    let (count, height, strips) = layout(data)?;
    check(count, b.limits.max_glyphs, "MGTK glyph count")?;
    b.table(data.len())?;
    b.add(4096)?;
    if let Some(charset) = &o.charset {
        b.table(charset.len())?;
    }
    let mut f = Font::default();
    f.source_format = Some("mgtk".into());
    f.charset = FontCharset {
        name: Some(o.charset.clone().unwrap_or_else(|| "unknown".into())),
        ..Default::default()
    };
    let widths = &data[3..3 + count];
    let maximum = widths.iter().copied().max().unwrap_or(0);
    f.metrics = FontMetrics {
        pixel_size: Some(height as u32),
        ascent: Some(height as i64),
        descent: Some(0),
        bounds: Some(Bounds::new(u32::from(maximum), height as u32, 0, 0)),
        spacing: Some(if widths.iter().all(|&w| w == widths[0]) {
            Spacing::Monospaced
        } else {
            Spacing::Proportional
        }),
        ..Default::default()
    };
    f.properties.push(Property::new(
        "mgtk.source",
        PropertyValue::Bytes(data.to_vec()),
    ));
    for (cp, &width) in widths.iter().enumerate() {
        let width = usize::from(width);
        b.glyph(width, height)?;
        let mut packed = [0u8; 32];
        let stride = width.div_ceil(8);
        for y in 0..height {
            for x in 0..width {
                let at = 3 + count + y * count * strips + (x / 7) * count + cp;
                if data[at] & (1 << (x % 7)) != 0 {
                    packed[y * stride + x / 8] |= 0x80 >> (x % 8);
                }
            }
        }
        let mut glyph = Glyph::new(Bitmap::<crate::bitmap::Mono>::from_raw(
            width as u32,
            height as u32,
            &packed[..stride * height],
            bitmap_limits(b.limits),
        )?);
        glyph.codepoints.push(Codepoint::Legacy(cp as u32));
        glyph.metrics.advance = Some(Vector::new(width as i64, 0));
        f.add_glyph(glyph)?;
    }
    let mut set = FontSet::from(f);
    set.source_format = Some("mgtk".into());
    Ok(set)
}
/// Write native cells, explicitly auditing every changed model field.
pub fn write(s: &FontSet, o: &MgtkWriteOptions) -> Result<(Vec<u8>, Report)> {
    let mut b = Budget::new(&o.limits);
    b.model(s)?;
    let f = s.single()?;
    let mut replay = None;
    if let Some(raw) = bytes(&f.properties, "mgtk.source") {
        let original = decode(
            raw,
            &MgtkReadOptions {
                charset: f.charset.name.clone(),
            },
            &mut b,
        )?;
        let strips = layout(raw)?.2;
        if original.single()? == f
            && (o.columns == Columns::Auto
                || o.columns == Columns::Single && strips == 1
                || o.columns == Columns::Double && strips == 2)
        {
            b.table(raw.len())?;
            replay = Some(raw.to_vec());
        }
    }
    let out = if let Some(raw) = replay {
        raw
    } else {
        encode(f, o, &mut b)?
    };
    check(
        out.len(),
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "MGTK output bytes",
    )?;
    let decoded = decode(&out, &MgtkReadOptions::default(), &mut b)?;
    let mut r = Report::default();
    compare(s, &decoded, "mgtk", &mut b, &mut r)?;
    Ok((out, r))
}
fn encode(f: &Font, o: &MgtkWriteOptions, b: &mut Budget<'_>) -> Result<Vec<u8>> {
    let mut slots = [None; 256];
    for (_, g) in f.glyphs() {
        for &cp in &g.codepoints {
            if let Codepoint::Legacy(n) = cp
                && n < 256
            {
                slots[n as usize] = Some(g);
            }
        }
    }
    let count = slots
        .iter()
        .rposition(Option::is_some)
        .ok_or_else(|| invalid("MGTK needs a legacy byte mapping"))?
        + 1;
    check(count, o.limits.max_glyphs, "MGTK output slots")?;
    let mut bottom = i64::MAX;
    let mut top = i64::MIN;
    let mut widths = [0usize; 256];
    let mut lefts = [0i64; 256];
    for (cp, g) in slots[..count]
        .iter()
        .enumerate()
        .filter_map(|(i, g)| g.map(|g| (i, g)))
    {
        if matches!(g.bitmap, GlyphBitmap::Gray8(_)) && o.gray_threshold.is_none() {
            return Err(invalid("MGTK Gray8 requires an explicit threshold"));
        }
        bottom = bottom.min(g.metrics.bounds.y);
        top = top.max(
            g.metrics
                .bounds
                .y
                .checked_add(i64::from(g.bitmap.height()))
                .ok_or_else(|| invalid("MGTK top overflow"))?,
        );
        let left = g.metrics.bounds.x.min(0);
        let advance = g
            .metrics
            .advance
            .or(f.metrics.glyph_defaults.advance)
            .map_or(i64::from(g.bitmap.width()), |v| v.x);
        if advance < 0 {
            return Err(invalid("MGTK cannot represent negative advance"));
        }
        let right = g
            .metrics
            .bounds
            .x
            .checked_add(i64::from(g.bitmap.width()))
            .ok_or_else(|| invalid("MGTK right overflow"))?
            .max(advance);
        let width = right
            .checked_sub(left)
            .ok_or_else(|| invalid("MGTK width overflow"))?;
        if !(0..=14).contains(&width) {
            return Err(invalid("MGTK glyph exceeds fourteen pixels"));
        }
        lefts[cp] = left;
        widths[cp] = width as usize;
    }
    if let Some(bounds) = f.metrics.bounds {
        bottom = bottom.min(bounds.y);
        top = top.max(
            bounds
                .y
                .checked_add(i64::from(bounds.height))
                .ok_or_else(|| invalid("MGTK font top overflow"))?,
        );
    }
    let height = top
        .checked_sub(bottom)
        .ok_or_else(|| invalid("MGTK height overflow"))?;
    if !(1..=16).contains(&height) {
        return Err(invalid("MGTK cell height must be in 1..=16"));
    }
    let height = height as usize;
    let wide = widths[..count].iter().any(|&w| w > 7);
    let strips = match o.columns {
        Columns::Single if wide => return Err(invalid("MGTK single strip cannot hold wide glyph")),
        Columns::Single => 1,
        Columns::Double => 2,
        Columns::Auto => {
            if wide {
                2
            } else {
                1
            }
        }
    };
    let size = 3 + count + count * strips * height;
    check(
        size,
        o.max_output_bytes.min(o.limits.max_input_bytes),
        "MGTK output bytes",
    )?;
    b.table(size)?;
    let mut out = vec![0u8; size];
    out[0] = if strips == 2 { 0x80 } else { 0 };
    out[1] = (count - 1) as u8;
    out[2] = height as u8;
    for cp in 0..count {
        out[3 + cp] = widths[cp] as u8;
        let Some(g) = slots[cp] else { continue };
        let dx = (g.metrics.bounds.x - lefts[cp]) as usize;
        let dy = (top - g.metrics.bounds.y - i64::from(g.bitmap.height())) as usize;
        check(
            widths[cp] * height,
            o.limits.max_pixels_per_glyph,
            "MGTK output glyph pixels",
        )?;
        for y in 0..g.bitmap.height() as usize {
            for x in 0..g.bitmap.width() as usize {
                let ink = match &g.bitmap {
                    GlyphBitmap::Mono(m) => {
                        m.as_bytes()[y * m.stride() + x / 8] & (0x80 >> (x % 8)) != 0
                    }
                    GlyphBitmap::Gray8(m) => {
                        m.as_bytes()[y * m.stride() + x]
                            >= o.gray_threshold.expect("validated threshold")
                    }
                };
                if ink {
                    let x = x + dx;
                    out[3 + count + (y + dy) * count * strips + (x / 7) * count + cp] |=
                        1 << (x % 7);
                }
            }
        }
    }
    Ok(out)
}
