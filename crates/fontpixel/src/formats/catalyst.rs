//! Native bounded Quark Catalyst SYSTEM.FONTS collections.
use super::workstation_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::Bounds;
use crate::model::{Codepoint, Font, FontSet, Property, PropertyValue};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
/// Reference origin for the empirical glyph directory.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CatalystOffsetBase {
    /// Relative to the resource body after its size word.
    #[default]
    ResourceBody,
    /// Explicit compatibility interpretation relative to raster data.
    GlyphData,
}
/// Explicit directory interpretation.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct CatalystReadOptions {
    /// Directory reference origin.
    pub offset_base: CatalystOffsetBase,
}
/// Bounded native collection output.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct CatalystWriteOptions {
    /// Directory origin.
    pub offset_base: CatalystOffsetBase,
    /// Explicit common raster height; absent preserves source or uses maximum glyph height.
    pub height: Option<u8>,
    /// Inter-line leading; absent preserves source or derives it.
    pub leading: Option<u8>,
    /// Common trailing bearing; absent preserves source or uses zero.
    pub sidebearing: Option<u8>,
    /// Permit numeric Unicode code positions.
    pub unicode_as_legacy: bool,
    /// Explicit grayscale reduction.
    pub gray_threshold: Option<u8>,
    /// Shared resource ceilings.
    pub limits: ResourceLimits,
    /// Aggregate output cap.
    pub max_output_bytes: usize,
}
impl Default for CatalystWriteOptions {
    fn default() -> Self {
        Self {
            offset_base: CatalystOffsetBase::ResourceBody,
            height: None,
            leading: None,
            sidebearing: None,
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct CatalystCodec {
    /// Input interpretation.
    pub read_options: CatalystReadOptions,
    /// Output choices.
    pub write_options: CatalystWriteOptions,
}
impl FormatDetector for CatalystCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for CatalystCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for CatalystCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Size-delimited structural evidence only; the format has no unique signature.
pub fn detect(b: &[u8]) -> Confidence {
    let Some(&n) = b.first() else {
        return Confidence::NoMatch;
    };
    if n == 0 {
        return Confidence::NoMatch;
    }
    let mut at = 1;
    for _ in 0..n {
        let Ok(size) = u16_at(b, at, ByteOrder::Little) else {
            return Confidence::NoMatch;
        };
        if size < 387 {
            return Confidence::NoMatch;
        }
        at += 2 + usize::from(size);
        if at > b.len() {
            return Confidence::NoMatch;
        }
    }
    if at == b.len() {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
/// Decode every bounded resource, following its authoritative offsets.
pub fn read(c: &mut ReadContext<'_>, o: &CatalystReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &CatalystReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    let n = usize::from(slice(b, 0, 1)?[0]);
    if n == 0 {
        return Err(invalid("Catalyst collection has no resources"));
    }
    check(n * 128, l.max_glyphs, "Catalyst aggregate glyphs")?;
    budget.items::<Font>(n)?;
    let mut fonts = Vec::with_capacity(n);
    let mut at = 1;
    for _ in 0..n {
        let size = usize::from(u16_at(b, at, ByteOrder::Little)?);
        at += 2;
        let body = slice(b, at, size)?;
        at += size;
        slice(body, 0, 387)?;
        let h = u32::from(body[384]);
        let leading = body[385];
        let side = body[386];
        let mut f = source("catalyst", body, 128, l, budget)?;
        f.properties.push(Property::new(
            "catalyst.metrics",
            PropertyValue::Bytes(body[384..387].to_vec()),
        ));
        for cp in 0..128 {
            let stored = body[256 + cp] & 127;
            let shift = body[256 + cp] >> 7;
            let width = u32::from(stored - stored / 8);
            let row = usize::from(stored).div_ceil(8);
            let off = usize::from(u16_at(body, cp * 2, ByteOrder::Little)?)
                + if o.offset_base == CatalystOffsetBase::GlyphData {
                    387
                } else {
                    0
                };
            let len = row * h as usize;
            if len > 0 && off < 387 {
                return Err(invalid("Catalyst glyph offset overlaps directory"));
            }
            let raw = slice(body, off, len)?;
            let stride = (width as usize).div_ceil(8);
            check(
                width as usize * h as usize,
                l.max_pixels_per_glyph,
                "Catalyst pixels",
            )?;
            budget.add(stride * h as usize + raw.len() + 256)?;
            let mut packed = vec![0; stride * h as usize];
            for y in 0..h as usize {
                for x in 0..width as usize {
                    if raw[y * row + x / 7] & (1 << (x % 7)) != 0 {
                        packed[y * stride + x / 8] |= 128 >> (x % 8);
                    }
                }
            }
            let bitmap = raster(width, h, &packed, l, budget)?;
            glyph(
                &mut f,
                bitmap,
                cp as u32,
                0,
                -i64::from(shift),
                i64::from(width) + i64::from(side),
            )?;
            let id = f
                .glyph_id(Codepoint::Legacy(cp as u32))
                .ok_or_else(|| invalid("Catalyst glyph index"))?;
            f.edit_glyph(id, |g| {
                g.properties.push(Property::new(
                    "catalyst.stored_width",
                    PropertyValue::Unsigned(u64::from(stored)),
                ));
                g.properties.push(Property::new(
                    "catalyst.raster",
                    PropertyValue::Bytes(raw.to_vec()),
                ));
                Ok(())
            })?;
        }
        metrics(&mut f, i64::from(h) + i64::from(leading));
        fonts.push(f);
    }
    if at != b.len() {
        return Err(invalid(
            "Catalyst trailing bytes outside declared resources",
        ));
    }
    Ok((set(fonts, "catalyst"), Report::default()))
}
/// Encode a complete collection and audit a same-limit native reread.
pub fn write(s: &FontSet, o: &WriteOptions, v: &CatalystWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        let n = u8::try_from(converted.fonts.len())
            .map_err(|_| invalid("Catalyst allows at most 255 resources"))?;
        if n == 0 {
            return Err(invalid("Catalyst needs a font"));
        }
        check(
            usize::from(n) * 128,
            v.limits.max_glyphs,
            "Catalyst aggregate glyphs",
        )?;
        budget.items::<Vec<u8>>(usize::from(n))?;
        let mut parts = Vec::with_capacity(usize::from(n));
        let mut total = 1usize;
        for (fi, f) in converted.fonts.iter().enumerate() {
            let mut local = Report::default();
            let selection = select(
                f,
                127,
                v.unicode_as_legacy,
                "catalyst",
                &mut local,
                &mut budget,
            );
            for entry in &mut local.entries {
                if entry.location.font_index.is_some() {
                    entry.location.font_index = Some(fi)
                }
            }
            report.extend(local);
            let slots = selection?;
            let old = bytes(f, "catalyst.metrics");
            let h = v
                .height
                .or_else(|| old.and_then(|b| b.first()).copied())
                .unwrap_or(
                    u8::try_from(
                        f.glyphs()
                            .map(|(_, g)| g.bitmap.height())
                            .max()
                            .unwrap_or(0),
                    )
                    .map_err(|_| invalid("Catalyst height exceeds 255"))?,
                );
            let leading = match v.leading.or_else(|| old.and_then(|b| b.get(1)).copied()) {
                Some(n) => n,
                None => u8::try_from(
                    f.metrics
                        .line_height
                        .unwrap_or(i64::from(h))
                        .checked_sub(i64::from(h))
                        .ok_or_else(|| invalid("Catalyst leading overflow"))?,
                )
                .map_err(|_| invalid("Catalyst leading exceeds u8"))?,
            };
            let side = v
                .sidebearing
                .or_else(|| old.and_then(|b| b.get(2)).copied())
                .unwrap_or(0);
            let mut widths = [0u8; 128];
            let mut size = 387usize;
            for (&cp, g) in &slots {
                let w = g.bitmap.width();
                if w > 112 {
                    return Err(invalid("Catalyst logical width exceeds112"));
                }
                let minimal = w + if w == 0 { 0 } else { (w - 1) / 7 };
                let old = int(&g.properties, "catalyst.stored_width", i64::from(minimal))?;
                let stored = if (0..=127).contains(&old) && old - old / 8 == i64::from(w) {
                    old as u8
                } else {
                    minimal as u8
                };
                widths[cp as usize] = stored;
                size += usize::from(stored).div_ceil(8) * usize::from(h);
            }
            let short = u16::try_from(size)
                .map_err(|_| invalid("Catalyst resource exceeds 65535 bytes"))?;
            total = total
                .checked_add(size + 2)
                .ok_or_else(|| invalid("Catalyst collection size overflow"))?;
            check(
                total,
                v.max_output_bytes
                    .min(v.limits.max_input_bytes)
                    .min(v.limits.max_table_bytes),
                "Catalyst output bytes",
            )?;
            budget.add(size + 2)?;
            let mut part = vec![0; size + 2];
            put16(&mut part, 0, short, ByteOrder::Little);
            let body = &mut part[2..];
            body[384..387].copy_from_slice(&[h, leading, side]);
            let mut at = 387;
            for cp in 0..128 {
                let g = slots.get(&(cp as u32)).copied();
                let stored = widths[cp];
                let width = u32::from(stored - stored / 8);
                let shift = g.map_or(0, |g| -g.metrics.bounds.y);
                if shift != 0 && shift != 1 {
                    return Err(invalid("Catalyst y offset must be zero or minus one"));
                }
                body[256 + cp] = stored + (shift as u8) * 128;
                let off = at
                    - if v.offset_base == CatalystOffsetBase::GlyphData {
                        387
                    } else {
                        0
                    };
                put16(body, cp * 2, off as u16, ByteOrder::Little);
                let row = usize::from(stored).div_ceil(8);
                let len = row * usize::from(h);
                let packed = project(
                    g,
                    Bounds::new(width, u32::from(h), 0, -shift),
                    &v.limits,
                    &mut budget,
                )?;
                let stride = (width as usize).div_ceil(8);
                let original = g.and_then(|g| {
                    if let Some(PropertyValue::Bytes(b)) = g.properties.get("catalyst.raster") {
                        Some(b.as_slice())
                    } else {
                        None
                    }
                });
                for y in 0..usize::from(h) {
                    for x in 0..row {
                        body[at + y * row + x] = original
                            .and_then(|b| b.get(y * row + x))
                            .copied()
                            .unwrap_or(0)
                            & 128;
                    }
                    for x in 0..width as usize {
                        if packed[y * stride + x / 8] & (128 >> (x % 8)) != 0 {
                            body[at + y * row + x / 7] |= 1 << (x % 7);
                        }
                    }
                }
                at += len;
            }
            parts.push(part);
        }
        output_limit(total, v.max_output_bytes, &v.limits, &mut budget)?;
        let mut out = Vec::with_capacity(total);
        out.push(n);
        for part in parts {
            out.extend_from_slice(&part);
        }
        let (actual, r) = parse(
            &out,
            &CatalystReadOptions {
                offset_base: v.offset_base,
            },
            &v.limits,
            &mut budget,
        )?;
        report.extend(r);
        audit(s, &actual, "catalyst", &mut report, &mut budget)?;
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "catalyst", o.strict)
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::{defaults, rejects, round_trip};

    #[test]
    fn options_serde_defaults_and_rejections() {
        assert_eq!(
            round_trip(&CatalystOffsetBase::ResourceBody),
            "resource_body"
        );
        assert_eq!(round_trip(&CatalystOffsetBase::GlyphData), "glyph_data");
        rejects::<CatalystOffsetBase>(r#""not_a_variant""#);
        defaults::<CatalystReadOptions>();
        defaults::<CatalystWriteOptions>();
        defaults::<CatalystCodec>();
    }
}
