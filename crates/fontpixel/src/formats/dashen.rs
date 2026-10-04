//! Native bounded Dashen screen fonts and embedded HP PCL strikes.
use super::hppcl::{self, HppclReadOptions, HppclWriteOptions};
use super::workstation_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::model::{Codepoint, FontSet, Property, PropertyValue};
use crate::report::{Impact, Level, Location};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ReportEntry, ResourceLimits, Result};
/// The observed three-byte character-header profiles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DashenVersion {
    /// Screen font, optionally followed by a code map.
    #[default]
    V3,
    /// Screen font, code map, and embedded printer fonts.
    V4,
}
/// Explicit input choices.
#[derive(Debug, Clone, Default)]
pub struct DashenReadOptions {
    /// Require a particular observed generation.
    pub version: Option<DashenVersion>,
    /// Embedded PCL compatibility policy.
    pub pcl: HppclReadOptions,
}
/// Native output choices.
#[derive(Debug, Clone)]
pub struct DashenWriteOptions {
    /// Screen-only or combined screen/printer profile.
    pub version: DashenVersion,
    /// Header top metric; absent preserves it or derives the maximum glyph top minus one.
    pub top: Option<u8>,
    /// Write a code map even for screen-only output.
    pub codepoint_map: bool,
    /// Embedded printer-font output choices (outer limits always apply).
    pub pcl: HppclWriteOptions,
    /// Permit numeric Unicode positions.
    pub unicode_as_legacy: bool,
    /// Explicit grayscale reduction.
    pub gray_threshold: Option<u8>,
    /// Shared ceilings.
    pub limits: ResourceLimits,
    /// Aggregate byte cap.
    pub max_output_bytes: usize,
}
impl Default for DashenWriteOptions {
    fn default() -> Self {
        Self {
            version: DashenVersion::V3,
            top: None,
            codepoint_map: false,
            pcl: HppclWriteOptions::default(),
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec.
#[derive(Debug, Clone, Default)]
pub struct DashenCodec {
    /// Input choices.
    pub read_options: DashenReadOptions,
    /// Output choices.
    pub write_options: DashenWriteOptions,
}
impl FormatDetector for DashenCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for DashenCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for DashenCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// Structural evidence only; there is no unique magic.
pub fn detect(b: &[u8]) -> Confidence {
    if b.len() >= 1070 && matches!(b.get(8..10), Some([3, 0] | [4, 0])) && b[14..18] == [255; 4] {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}
/// Read screen and printer strikes with one cumulative allocation budget.
pub fn read(c: &mut ReadContext<'_>, o: &DashenReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    parse(c.bytes(), o, c.limits(), &mut budget)
}
fn parse(
    b: &[u8],
    o: &DashenReadOptions,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    slice(b, 0, 1070)?;
    check(b.len(), l.max_table_bytes, "Dashen table bytes")?;
    let version = match u16_at(b, 8, ByteOrder::Little)? {
        3 => DashenVersion::V3,
        4 => DashenVersion::V4,
        _ => {
            return Err(invalid(
                "Dashen one-byte or unknown character-header generation is not defined by the available specification",
            ));
        }
    };
    if o.version.is_some_and(|v| v != version) {
        return Err(invalid("Dashen generation differs from requested profile"));
    }
    let mut records = [None; 256];
    let mut end = 1070;
    let mut count = 0;
    for (i, record) in records.iter_mut().enumerate() {
        let offset = u32_at(b, 14 + i * 4, ByteOrder::Little)? as i32;
        if offset == -1 {
            continue;
        }
        if offset < 0 {
            return Err(invalid("Dashen negative glyph offset"));
        }
        let at = 1070usize
            .checked_add(offset as usize)
            .ok_or_else(|| invalid("Dashen offset overflow"))?;
        let gh = slice(b, at, 3)?;
        let len = usize::from(gh[0]) * usize::from(gh[2]).div_ceil(8);
        slice(b, at + 3, len)?;
        end = end.max(at + 3 + len);
        *record = Some(at);
        count += 1;
    }
    check(count, l.max_glyphs, "Dashen screen glyphs")?;
    let mapped = end < b.len();
    let map = if mapped {
        Some(slice(b, end, 256)?)
    } else {
        None
    };
    let pcl_at = end + if mapped { 256 } else { 0 };
    if version == DashenVersion::V4 && pcl_at == b.len() {
        return Err(invalid("Dashen v4 contains no embedded printer font"));
    }
    let mut f = source("dashen", &b[..pcl_at], count, l, budget)?;
    f.properties.push(Property::new(
        "dashen.header",
        PropertyValue::Bytes(b[..14].to_vec()),
    ));
    f.properties.push(Property::new(
        "dashen.opaque",
        PropertyValue::Bytes(b[1038..1070].to_vec()),
    ));
    if let Some(map) = map {
        f.properties.push(Property::new(
            "dashen.codepoint_map",
            PropertyValue::Bytes(map.to_vec()),
        ));
    }
    let mut report = Report::default();
    for (slot, at) in records.into_iter().enumerate() {
        let Some(at) = at else { continue };
        let w = u32::from(b[at]);
        let shift = b[at + 1];
        let h = u32::from(b[at + 2]);
        let row = (w as usize).div_ceil(8);
        check(
            (w as usize) * (h as usize),
            l.max_pixels_per_glyph,
            "Dashen pixels",
        )?;
        budget.add(row * h as usize + 512)?;
        let mut packed = vec![0; row * h as usize];
        for y in 0..h as usize {
            for x in 0..w as usize {
                if b[at + 3 + (y / 8) * w as usize + x] & (128 >> (y % 8)) != 0 {
                    packed[y * row + x / 8] |= 128 >> (x % 8);
                }
            }
        }
        let cp = map.map_or(slot as u32, |m| u32::from(m[slot]));
        if f.glyph_id(Codepoint::Legacy(cp)).is_some() {
            return Err(invalid(
                "Dashen code map assigns multiple present slots to the same character",
            ));
        }
        let bitmap = raster(w, h, &packed, l, budget)?;
        glyph(
            &mut f,
            bitmap,
            cp,
            0,
            i64::from(b[12]) - i64::from(h) - i64::from(shift) + 1,
            i64::from(w),
        )?;
        let id = f
            .glyph_id(Codepoint::Legacy(cp))
            .ok_or_else(|| invalid("Dashen glyph index"))?;
        f.edit_glyph(id, |g| {
            g.properties.push(Property::new(
                "dashen.slot",
                PropertyValue::Unsigned(slot as u64),
            ));
            Ok(())
        })?;
    }
    metrics(&mut f, i64::from(u16_at(b, 4, ByteOrder::Little)?));
    let mut fonts = vec![f];
    if pcl_at < b.len() {
        let mut remaining = l.clone();
        remaining.max_glyphs = remaining.max_glyphs.saturating_sub(count);
        let (pcl, r) = hppcl::parse(&b[pcl_at..], &o.pcl, &remaining, budget)?;
        report.extend(r);
        budget.items::<crate::model::Font>(pcl.fonts.len())?;
        fonts.extend(pcl.fonts);
        if !pcl.properties.is_empty() {
            budget.add(b.len() - pcl_at + 256)?;
            fonts[0].properties.push(Property::new(
                "dashen.pcl_stream",
                PropertyValue::Bytes(b[pcl_at..].to_vec()),
            ));
        }
    }
    Ok((set(fonts, "dashen"), report))
}
/// Encode a screen directory and optional native PCL stream; audit a same-limit reread.
pub fn write(s: &FontSet, o: &WriteOptions, v: &DashenWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let converted = convert_gray(s, v.gray_threshold, &v.limits, &mut budget)?;
        if converted.fonts.is_empty() {
            return Err(invalid("Dashen needs a screen font"));
        }
        if (v.version == DashenVersion::V3 && converted.fonts.len() != 1)
            || (v.version == DashenVersion::V4 && converted.fonts.len() < 2)
        {
            return Err(invalid(
                "Dashen v3 needs one screen font; v4 also needs printer strikes",
            ));
        }
        let f = &converted.fonts[0];
        let slots = select(
            f,
            255,
            v.unicode_as_legacy,
            "dashen",
            &mut report,
            &mut budget,
        )?;
        let old = bytes(f, "dashen.header").filter(|b| b.len() == 14);
        let top = match v.top.or_else(|| old.map(|b| b[12])) {
            Some(t) => t,
            None => u8::try_from(
                f.glyphs()
                    .map(|(_, g)| g.metrics.bounds.y + i64::from(g.bitmap.height()) - 1)
                    .max()
                    .unwrap_or(0),
            )
            .map_err(|_| invalid("Dashen top metric exceeds u8"))?,
        };
        let original_map = bytes(f, "dashen.codepoint_map").filter(|b| b.len() == 256);
        let mapped = v.codepoint_map || original_map.is_some() || v.version == DashenVersion::V4;
        let mut mapping = [0; 256];
        for (i, value) in mapping.iter_mut().enumerate() {
            *value = i as u8;
        }
        if let Some(m) = original_map {
            mapping.copy_from_slice(m);
        }
        let mut arranged = [None; 256];
        let mut total = 1070usize;
        for (&cp, g) in &slots {
            let preferred = int(&g.properties, "dashen.slot", i64::from(cp))?;
            let index = if (0..256).contains(&preferred)
                && arranged[preferred as usize].is_none()
                && (!mapped || mapping[preferred as usize] == cp as u8)
            {
                preferred as usize
            } else if arranged[cp as usize].is_none() {
                cp as usize
            } else {
                arranged
                    .iter()
                    .position(Option::is_none)
                    .ok_or_else(|| invalid("Dashen directory is full"))?
            };
            if !mapped && index != cp as usize {
                return Err(invalid("Dashen reordered slots require a code map"));
            }
            mapping[index] = cp as u8;
            arranged[index] = Some(g);
            if g.bitmap.width() > 255 || g.bitmap.height() > 255 {
                return Err(invalid("Dashen glyph dimensions exceed u8"));
            }
            total = total
                .checked_add(
                    3 + g.bitmap.width() as usize * (g.bitmap.height() as usize).div_ceil(8),
                )
                .ok_or_else(|| invalid("Dashen size overflow"))?;
        }
        if mapped {
            total += 256;
        }
        output_limit(total, v.max_output_bytes, &v.limits, &mut budget)?;
        let mut out = vec![0; total];
        if let Some(h) = old {
            out[..14].copy_from_slice(h);
        }
        put16(
            &mut out,
            4,
            u16::try_from(f.metrics.line_height.unwrap_or(i64::from(top) + 1))
                .map_err(|_| invalid("Dashen line height exceeds u16"))?,
            ByteOrder::Little,
        );
        put16(
            &mut out,
            6,
            slots.values().map(|g| g.bitmap.width()).max().unwrap_or(0) as u16,
            ByteOrder::Little,
        );
        put16(
            &mut out,
            8,
            if v.version == DashenVersion::V3 { 3 } else { 4 },
            ByteOrder::Little,
        );
        out[12] = top;
        out[14..1038].fill(255);
        if let Some(opaque) = bytes(f, "dashen.opaque").filter(|b| b.len() == 32) {
            out[1038..1070].copy_from_slice(opaque);
        }
        let mut at = 1070;
        for (slot, g) in arranged.into_iter().enumerate() {
            let Some(g) = g else { continue };
            if g.metrics.bounds.x != 0 {
                return Err(invalid("Dashen cannot encode nonzero horizontal bearing"));
            }
            let shift = u8::try_from(
                i64::from(top) - i64::from(g.bitmap.height()) + 1 - g.metrics.bounds.y,
            )
            .map_err(|_| invalid("Dashen vertical shift exceeds u8"))?;
            put32(
                &mut out,
                14 + slot * 4,
                (at - 1070) as u32,
                ByteOrder::Little,
            );
            let bitmap = mono(g)?;
            out[at] = bitmap.width() as u8;
            out[at + 1] = shift;
            out[at + 2] = bitmap.height() as u8;
            for y in 0..bitmap.height() as usize {
                for x in 0..bitmap.width() as usize {
                    if bitmap.as_bytes()[y * bitmap.stride() + x / 8] & (128 >> (x % 8)) != 0 {
                        out[at + 3 + (y / 8) * bitmap.width() as usize + x] |= 128 >> (y % 8);
                    }
                }
            }
            at += 3 + bitmap.width() as usize * (bitmap.height() as usize).div_ceil(8);
        }
        if mapped {
            out[at..at + 256].copy_from_slice(&mapping);
        }
        if converted.fonts.len() > 1 {
            let mut pcl = v.pcl.clone();
            pcl.limits = v.limits.clone();
            pcl.limits.max_glyphs = pcl.limits.max_glyphs.saturating_sub(slots.len());
            pcl.max_output_bytes = v.max_output_bytes.saturating_sub(out.len());
            let data = hppcl::encode(&converted.fonts[1..], &pcl, &mut budget, &mut report)?;
            output_limit(
                out.len().saturating_add(data.len()),
                v.max_output_bytes,
                &v.limits,
                &mut budget,
            )?;
            out.extend_from_slice(&data);
        }
        let (actual, r) = parse(&out, &DashenReadOptions::default(), &v.limits, &mut budget)?;
        report.extend(r);
        audit(s, &actual, "dashen", &mut report, &mut budget)?;
        // Existing source coordinates may encode a different device advance.
        if s.fonts[0]
            .glyphs()
            .any(|(_, g)| g.metrics.advance.is_some_and(|a| a.y != 0))
        {
            report.push(ReportEntry {
                level: Level::Warning,
                impact: Impact::Loss,
                code: "dashen.vertical_advance".into(),
                location: Location {
                    font_index: Some(0),
                    ..Default::default()
                },
                message: "Dashen screen records do not carry a vertical advance.".into(),
            });
        }
        Ok(OutputBundle::single(o.primary_name.clone(), out))
    })();
    finish(result, report, "dashen", o.strict)
}
