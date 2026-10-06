//! Native bounded RISC OS bitmap fonts and explicit IntMetrics resources.
mod bitmap;
mod metrics;
mod writer;
use super::workstation_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Bounds, Gray8};
use crate::model::{
    Codepoint, Font, FontSet, Glyph, GlyphBitmap, Property, PropertyValue, Rational, Vector,
};
use crate::report::{Impact, Level, Location};
use crate::resources::{OutputAsset, OutputBundle, ReadContext, ResourcePath, Written};
use crate::{Report, ReportEntry, ResourceLimits, Result};
use std::collections::{BTreeMap, BTreeSet};
/// Native bitmap file generation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RiscosVersion {
    /// Original x90y45 multi-strike container.
    Xy,
    /// FONT v4.
    V4,
    /// FONT v5.
    V5,
    /// FONT v6.
    #[default]
    V6,
    /// FONT v7, per-chunk flags.
    V7,
    /// FONT v8, relocated and expandable chunk directory.
    V8,
}
/// Native bitmap depth.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RiscosDepth {
    /// Packed one-bit ink.
    #[default]
    Mono,
    /// Sixteen grayscale levels.
    Gray4,
}
/// Native one-bit storage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RiscosCompression {
    /// Continuous bottom-up pixels.
    #[default]
    Raw,
    /// Packed run numbers (f=12 on output).
    Packed,
}
/// Optional native scalable-metrics output version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RiscosMetricsVersion {
    /// Original mapped eight-bit directory.
    V0,
    /// Version 2 with direct character indexing.
    #[default]
    V2,
}
/// Input profile and scoped sidecar.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct RiscosReadOptions {
    /// Require a file generation.
    pub version: Option<RiscosVersion>,
    /// Explicit scoped IntMetrics resource.
    pub metrics_path: Option<ResourcePath>,
}
/// Native output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct RiscosWriteOptions {
    /// Bitmap file generation.
    pub version: RiscosVersion,
    /// Native depth; old XY files always require Gray4.
    pub depth: RiscosDepth,
    /// Packed encoding is available only for new one-bit files.
    pub compression: RiscosCompression,
    /// Optional IntMetrics output resource.
    pub metrics_path: Option<ResourcePath>,
    /// IntMetrics generation.
    pub metrics_version: RiscosMetricsVersion,
    /// Permit numeric Unicode codes.
    pub unicode_as_legacy: bool,
    /// Explicit Gray8-to-Mono threshold.
    pub gray_threshold: Option<u8>,
    /// Explicit rounding to sixteen grayscale levels.
    pub quantize: bool,
    /// Shared ceilings.
    pub limits: ResourceLimits,
    /// Aggregate output cap.
    pub max_output_bytes: usize,
}
impl Default for RiscosWriteOptions {
    fn default() -> Self {
        Self {
            version: RiscosVersion::V6,
            depth: RiscosDepth::Mono,
            compression: RiscosCompression::Raw,
            metrics_path: None,
            metrics_version: RiscosMetricsVersion::V2,
            unicode_as_legacy: false,
            gray_threshold: None,
            quantize: false,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct RiscosCodec {
    /// Reading choices.
    pub read_options: RiscosReadOptions,
    /// Writing choices.
    pub write_options: RiscosWriteOptions,
}
impl FormatDetector for RiscosCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for RiscosCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for RiscosCodec {
    fn write(&self, s: &FontSet, o: &WriteOptions) -> Result<Written> {
        write(s, o, &self.write_options)
    }
}
/// FONT plus a valid bitmap-generation header identifies this format; XY remains structural evidence.
pub fn detect(b: &[u8]) -> Confidence {
    if b.starts_with(b"FONT") {
        if b.len() < 62 || !matches!(b[4], 1 | 4) || !(4..=8).contains(&b[5]) {
            return Confidence::NoMatch;
        }
        let word = |at| u16::from_le_bytes([b[at], b[at + 1]]);
        let flags = word(6);
        if flags & !0x43 != 0
            || (flags & 64 != 0) != (b[5] >= 7)
            || word(52) != 10
            || [54, 56, 58, 60].into_iter().any(|at| word(at) == 0)
        {
            return Confidence::NoMatch;
        }
        let dword = |at| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize;
        let (directory, chunks) = if b[5] == 8 {
            (dword(16), dword(20))
        } else {
            (16, 8)
        };
        if chunks == 0 || chunks > 2048 {
            return Confidence::NoMatch;
        }
        let Some(end) = directory
            .checked_add((chunks + 1) * 4)
            .filter(|end| *end <= b.len())
        else {
            return Confidence::NoMatch;
        };
        let first = dword(directory);
        let last = dword(end - 4);
        if first < 62
            || first > last
            || last != b.len()
            || !first.is_multiple_of(4)
            || !last.is_multiple_of(4)
            || b[5] == 8 && (directory < 62 || end > first)
        {
            return Confidence::NoMatch;
        }
        Confidence::Certain
    } else if b.len() >= 256 && b[0] > 0 && b[1] == 4 && b[2] > 0 && b[3] > 0 {
        Confidence::Possible
    } else {
        Confidence::NoMatch
    }
}

fn loss(r: &mut Report, code: &str, message: &str, fi: usize) {
    r.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: format!("riscos.{code}"),
        location: Location {
            font_index: Some(fi),
            ..Default::default()
        },
        message: message.into(),
    });
}
fn profile(v: u8) -> Result<RiscosVersion> {
    match v {
        4 => Ok(RiscosVersion::V4),
        5 => Ok(RiscosVersion::V5),
        6 => Ok(RiscosVersion::V6),
        7 => Ok(RiscosVersion::V7),
        8 => Ok(RiscosVersion::V8),
        _ => Err(invalid("RISC OS FONT version must be 4..8")),
    }
}
/// Read all strikes, phases and explicitly resolved metrics.
pub fn read(c: &mut ReadContext<'_>, o: &RiscosReadOptions) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(c.retained_input_bytes(), c.limits())?;
    let mut loaded: Option<Vec<u8>> = None;
    let mut seen = BTreeSet::new();
    loop {
        let b = loaded.as_deref().unwrap_or(c.bytes());
        if b.starts_with(b"FONT") || b.len() >= 256 {
            break;
        }
        let name = std::str::from_utf8(b)
            .map_err(|_| invalid("RISC OS master reference is not UTF-8"))?
            .trim_end_matches(['\0', '\r', '\n'])
            .to_string();
        if name.is_empty() {
            return Err(invalid("RISC OS empty master reference"));
        }
        budget.add(name.len() + 128)?;
        if !seen.insert(name.clone()) {
            return Err(invalid("RISC OS master reference cycle"));
        }
        let path = ResourcePath::new(name)?;
        let data = c.resource_limited(&path, budget.max.saturating_sub(budget.used))?;
        budget.add(data.len())?;
        loaded = Some(data);
    }
    let rawmetrics = if let Some(path) = &o.metrics_path {
        let raw = c.resource_limited(path, budget.max.saturating_sub(budget.used))?;
        budget.add(raw.len())?;
        Some(raw)
    } else {
        None
    };
    let (mut s, mut report) = parse(
        loaded.as_deref().unwrap_or(c.bytes()),
        o.version,
        c.limits(),
        &mut budget,
    )?;
    if let Some(raw) = rawmetrics {
        metrics::apply(&mut s, &raw, c.limits(), &mut budget, &mut report)?;
    }
    if !seen.is_empty() {
        budget.add(c.bytes().len() + 256)?;
        s.properties.push(Property::new(
            "riscos.master_reference",
            PropertyValue::Bytes(c.bytes().to_vec()),
        ));
    }
    Ok((s, report))
}
fn add_glyph(
    f: &mut Font,
    bitmap: GlyphBitmap,
    cp: u32,
    x: i64,
    y: i64,
    budget: &mut Budget,
) -> Result<()> {
    budget.items::<Glyph>(1)?;
    budget.add(512)?;
    let mut g = Glyph::new(bitmap);
    g.codepoints.push(Codepoint::Legacy(cp));
    g.metrics.bounds.x = x;
    g.metrics.bounds.y = y;
    g.metrics.advance = Some(Vector::new(x + i64::from(g.bitmap.width()), 0));
    g.metrics.left_bearing = Some(x);
    g.metrics.right_bearing = Some(0);
    f.add_glyph(g)?;
    Ok(())
}
fn parse(
    b: &[u8],
    requested: Option<RiscosVersion>,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    check(b.len(), l.max_table_bytes, "RISC OS tables")?;
    let (mut s, report) = if b.starts_with(b"FONT") {
        parse_new(b, requested, l, budget)?
    } else {
        if requested.is_some_and(|v| v != RiscosVersion::Xy) {
            return Err(invalid("RISC OS expected FONT signature"));
        }
        parse_xy(b, l, budget)?
    };
    budget.add(b.len() + 256)?;
    s.properties.push(Property::new(
        "riscos.file",
        PropertyValue::Bytes(b.to_vec()),
    ));
    Ok((s, report))
}
fn parse_xy(b: &[u8], l: &ResourceLimits, budget: &mut Budget) -> Result<(FontSet, Report)> {
    let mut index = Vec::new();
    let mut at = 0;
    while slice(b, at, 1)?[0] != 0 {
        let record = slice(b, at, 16)?;
        if record[1] != 4 || record[2] == 0 || record[3] == 0 {
            return Err(invalid("RISC OS XY needs 4bpp and nonzero resolution"));
        }
        budget.add(64)?;
        index.push((
            at,
            u32_at(b, at + 8, ByteOrder::Little)? as usize,
            u32_at(b, at + 12, ByteOrder::Little)? as usize,
        ));
        at += 16;
    }
    if index.is_empty() {
        return Err(invalid("RISC OS XY has no strikes"));
    }
    budget.items::<Font>(index.len())?;
    let mut fonts = Vec::with_capacity(index.len());
    let mut count = 0;
    for (header, offset, size) in index {
        if size > 65536 || offset < at + 1 || offset % 4 != 0 {
            return Err(invalid("RISC OS XY strike span or alignment is invalid"));
        }
        let body = slice(b, offset, size)?;
        slice(body, 0, 532)?;
        let mut f = source("riscos", body, 0, l, budget)?;
        let point = b[header];
        let dx = u32_at(body, 8, ByteOrder::Little)?;
        let dy = u32_at(body, 12, ByteOrder::Little)?;
        if dx == 0 || dy == 0 {
            return Err(invalid("RISC OS XY body resolution is zero"));
        }
        f.metrics.point_size = Some(Rational::integer(i64::from(point)));
        f.metrics.resolution = Some(Vector::new(i64::from(dx), i64::from(dy)));
        f.properties
            .push(Property::new("riscos.version", PropertyValue::Integer(0)));
        f.properties.push(Property::new(
            "riscos.xy_index",
            PropertyValue::Bytes(b[header..header + 16].to_vec()),
        ));
        for cp in 0..256 {
            let off = usize::from(u16_at(body, 20 + cp * 2, ByteOrder::Little)?);
            if off == 0 {
                continue;
            }
            if off < 512 {
                return Err(invalid("RISC OS XY glyph overlaps offset table"));
            }
            let gh = slice(body, 20 + off, 4)?;
            let w = gh[2] as i8;
            let h = gh[3] as i8;
            if w < 0 || h < 0 {
                return Err(invalid("RISC OS XY has negative bitmap dimensions"));
            }
            count += 1;
            check(count, l.max_glyphs, "RISC OS aggregate glyphs")?;
            let len = (w as usize * h as usize).div_ceil(2);
            let raw = slice(body, 24 + off, len)?;
            let bitmap = bitmap::decode(raw, w as u32, h as u32, 4, l, budget)?;
            add_glyph(
                &mut f,
                bitmap,
                cp as u32,
                i64::from(gh[0] as i8),
                i64::from(gh[1] as i8),
                budget,
            )?;
        }
        let x = i64::from(body[16] as i8);
        let y = i64::from(body[17] as i8);
        let right = i64::from(body[18] as i8);
        let top = i64::from(body[19] as i8);
        if right < x || top < y {
            return Err(invalid("RISC OS XY font bbox is inverted"));
        }
        super::workstation_common::metrics(&mut f, top - y);
        f.metrics.bounds = Some(Bounds::new((right - x) as u32, (top - y) as u32, x, y));
        fonts.push(f);
    }
    Ok((set(fonts, "riscos"), Report::default()))
}
fn pair12(b: &[u8]) -> (i64, i64) {
    // Original BSD TTF2f producer: continuous little-endian 12-bit pairs.
    let x = u16::from(b[0]) | (u16::from(b[1] & 15) << 8);
    let y = u16::from(b[1] >> 4) | (u16::from(b[2]) << 4);
    let signed = |v: u16| {
        if v & 2048 != 0 {
            i64::from(v) - 4096
        } else {
            i64::from(v)
        }
    };
    (signed(x), signed(y))
}
fn parse_new(
    b: &[u8],
    requested: Option<RiscosVersion>,
    l: &ResourceLimits,
    budget: &mut Budget,
) -> Result<(FontSet, Report)> {
    slice(b, 0, 62)?;
    let version = profile(b[5])?;
    if requested.is_some_and(|v| v != version) {
        return Err(invalid("RISC OS version differs from requested profile"));
    }
    if !matches!(b[4], 1 | 4) {
        return Err(invalid(
            "RISC OS outline or unknown-depth FONT is not a bitmap",
        ));
    }
    let flags = u16_at(b, 6, ByteOrder::Little)?;
    if flags & !0x43 != 0 || ((flags & 64 != 0) != (b[5] >= 7)) {
        return Err(invalid("RISC OS header flags disagree with version"));
    }
    let (directory, chunks) = if b[5] == 8 {
        (
            u32_at(b, 16, ByteOrder::Little)? as usize,
            u32_at(b, 20, ByteOrder::Little)? as usize,
        )
    } else {
        (16, 8)
    };
    if chunks == 0 || chunks > 2048 {
        return Err(invalid(
            "RISC OS chunk count must fit 16-bit character codes",
        ));
    }
    slice(b, directory, (chunks + 1) * 4)?;
    if u16_at(b, 52, ByteOrder::Little)? != 10 {
        return Err(invalid("RISC OS bitmap table size is not ten"));
    }
    let xp = u16_at(b, 54, ByteOrder::Little)?;
    let dx = u16_at(b, 56, ByteOrder::Little)?;
    let yp = u16_at(b, 58, ByteOrder::Little)?;
    let dy = u16_at(b, 60, ByteOrder::Little)?;
    if xp == 0 || yp == 0 || dx == 0 || dy == 0 {
        return Err(invalid("RISC OS point size and resolution must be nonzero"));
    }
    let first = u32_at(b, directory, ByteOrder::Little)? as usize;
    let end = u32_at(b, directory + chunks * 4, ByteOrder::Little)? as usize;
    if first < 62
        || end != b.len()
        || b[5] == 8 && (directory < 62 || directory + (chunks + 1) * 4 > first)
    {
        return Err(invalid("RISC OS chunk-directory bounds are invalid"));
    }
    let description_end = if b[5] == 8 { directory } else { first };
    let description = slice(b, 62, description_end - 62)?;
    let mut fonts: BTreeMap<(u8, u8), Font> = BTreeMap::new();
    let mut count = 0;
    for chunk in 0..chunks {
        let start = u32_at(b, directory + chunk * 4, ByteOrder::Little)? as usize;
        let stop = u32_at(b, directory + (chunk + 1) * 4, ByteOrder::Little)? as usize;
        if stop < start || stop > b.len() || !start.is_multiple_of(4) || !stop.is_multiple_of(4) {
            return Err(invalid("RISC OS chunks must be ordered and word-aligned"));
        }
        if start == stop {
            continue;
        }
        let raw = slice(b, start, stop - start)?;
        let (index, phase_flags) = if b[5] >= 7 {
            let word = u32_at(raw, 0, ByteOrder::Little)?;
            if word & 0x8000_0000 == 0 || word & !0x8000_0083 != 0 || word & 128 != 0 {
                return Err(invalid(
                    "RISC OS bitmap chunk has invalid or outline-dependency flags",
                ));
            }
            (4, word & 3)
        } else {
            (0, u32::from(flags & 3))
        };
        let nx = if phase_flags & 1 != 0 { 4 } else { 1 };
        let ny = if phase_flags & 2 != 0 { 4 } else { 1 };
        let table_size = 128 * nx * ny;
        slice(raw, index, table_size)?;
        budget.items::<usize>(32 * nx * ny + 1)?;
        let mut offsets = Vec::with_capacity(32 * nx * ny + 1);
        for i in 0..32 * nx * ny {
            let off = u32_at(raw, index + i * 4, ByteOrder::Little)? as usize;
            if off != 0 {
                if off < table_size || off >= raw.len() - index {
                    return Err(invalid("RISC OS glyph offset outside chunk data"));
                }
                offsets.push(off);
            }
        }
        offsets.push(raw.len() - index);
        offsets.sort_unstable();
        offsets.dedup();
        for px in 0..nx {
            for py in 0..ny {
                let phase = (px as u8, py as u8);
                if let std::collections::btree_map::Entry::Vacant(entry) = fonts.entry(phase) {
                    let mut f = source("riscos", &b[..62], 0, l, budget)?;
                    budget.add(description.len() + 512)?;
                    f.metrics.point_size = Some(Rational::new(i64::from(yp), 16)?);
                    f.metrics.resolution = Some(Vector::new(i64::from(dx), i64::from(dy)));
                    f.properties.push(Property::new(
                        "riscos.version",
                        PropertyValue::Integer(i64::from(b[5])),
                    ));
                    f.properties.push(Property::new(
                        "riscos.x_size",
                        PropertyValue::Integer(i64::from(xp)),
                    ));
                    f.properties.push(Property::new(
                        "riscos.phase_x",
                        PropertyValue::Integer(px as i64),
                    ));
                    f.properties.push(Property::new(
                        "riscos.phase_y",
                        PropertyValue::Integer(py as i64),
                    ));
                    f.properties.push(Property::new(
                        "riscos.description",
                        PropertyValue::Bytes(description.to_vec()),
                    ));
                    if let Some(name) = description
                        .split(|v| *v == 0)
                        .next()
                        .filter(|n| !n.is_empty())
                    {
                        f.names.family = Some(latin(name));
                    }
                    entry.insert(f);
                }
                let f = fonts
                    .get_mut(&phase)
                    .ok_or_else(|| invalid("RISC OS phase index"))?;
                for ci in 0..32 {
                    let off = u32_at(
                        raw,
                        index + ((px * ny + py) * 32 + ci) * 4,
                        ByteOrder::Little,
                    )? as usize;
                    if off == 0 {
                        continue;
                    }
                    let oi = offsets
                        .binary_search(&off)
                        .map_err(|_| invalid("RISC OS glyph boundary lookup"))?;
                    let next = *offsets
                        .get(oi + 1)
                        .ok_or_else(|| invalid("RISC OS missing terminal glyph boundary"))?;
                    let record = slice(raw, index + off, next - off)?;
                    let tag = slice(record, 0, 1)?[0];
                    if tag & 8 != 0 {
                        return Err(invalid(
                            "RISC OS outline glyph cannot be decoded as a bitmap",
                        ));
                    }
                    let (x, y, w, h, skip) = if tag & 1 != 0 {
                        slice(record, 1, 6)?;
                        let (x, y) = pair12(&record[1..4]);
                        let (w, h) = pair12(&record[4..7]);
                        (x, y, w, h, 7)
                    } else {
                        let d = slice(record, 1, 4)?;
                        (
                            i64::from(d[0] as i8),
                            i64::from(d[1] as i8),
                            i64::from(d[2] as i8),
                            i64::from(d[3] as i8),
                            5,
                        )
                    };
                    if w < 0 || h < 0 {
                        return Err(invalid("RISC OS negative bitmap dimensions"));
                    }
                    count += 1;
                    check(count, l.max_glyphs, "RISC OS aggregate phase glyphs")?;
                    let data = &record[skip..];
                    let depth = if tag & 2 != 0 { 1 } else { 4 };
                    let bitmap = if tag >> 4 == 0 {
                        let needed = (w as usize * h as usize * depth as usize).div_ceil(8);
                        slice(data, 0, needed)?;
                        if data.len() - needed > 3 || data[needed..].iter().any(|v| *v != 0) {
                            return Err(invalid("RISC OS uncompressed glyph has excess data"));
                        }
                        bitmap::decode(data, w as u32, h as u32, depth, l, budget)?
                    } else {
                        if depth != 1 {
                            return Err(invalid("RISC OS packed glyph must be one-bit"));
                        }
                        bitmap::unpack(data, w as u32, h as u32, tag >> 4, tag & 4 != 0, l, budget)?
                    };
                    add_glyph(f, bitmap, (chunk * 32 + ci) as u32, x, y, budget)?;
                }
            }
        }
    }
    if fonts.is_empty() {
        let mut f = source("riscos", &b[..62], 0, l, budget)?;
        f.metrics.point_size = Some(Rational::new(i64::from(yp), 16)?);
        f.metrics.resolution = Some(Vector::new(i64::from(dx), i64::from(dy)));
        fonts.insert((0, 0), f);
    }
    let x = i64::from(u16_at(b, 8, ByteOrder::Little)? as i16);
    let y = i64::from(u16_at(b, 10, ByteOrder::Little)? as i16);
    let w = u16_at(b, 12, ByteOrder::Little)? as i16;
    let h = u16_at(b, 14, ByteOrder::Little)? as i16;
    if w < 0 || h < 0 {
        return Err(invalid("RISC OS FONT header bbox is inverted"));
    }
    for f in fonts.values_mut() {
        super::workstation_common::metrics(f, i64::from(h));
        f.metrics.bounds = Some(Bounds::new(w as u32, h as u32, x, y));
    }
    Ok((
        set(fonts.into_values().collect(), "riscos"),
        Report::default(),
    ))
}
/// Encode all requested native profiles and optional metrics; audit actual output.
pub fn write(s: &FontSet, o: &WriteOptions, v: &RiscosWriteOptions) -> Result<Written> {
    let mut report = Report::default();
    let result = (|| {
        let mut budget = source_budget(s, &v.limits)?;
        let out = writer::encode(s, v, &mut budget, &mut report)?;
        let rawmetrics = if v.metrics_path.is_some() {
            Some(metrics::encode(s, v, &mut budget, &mut report)?)
        } else {
            None
        };
        let (mut actual, r) = parse(&out, Some(v.version), &v.limits, &mut budget)?;
        report.extend(r);
        if let Some(raw) = &rawmetrics {
            check(
                raw.len(),
                v.limits.max_resource_bytes,
                "RISC OS metrics resource bytes",
            )?;
            check(1, v.limits.max_resources, "RISC OS output resources")?;
            check(
                out.len().saturating_add(raw.len()),
                v.max_output_bytes.min(v.limits.max_total_bytes),
                "RISC OS output bundle bytes",
            )?;
            metrics::apply(&mut actual, raw, &v.limits, &mut budget, &mut report)?;
        }
        audit(s, &actual, "riscos", &mut report, &mut budget)?;
        if let (Some(path), Some(raw)) = (&v.metrics_path, rawmetrics) {
            OutputBundle::new(
                o.primary_name.clone(),
                vec![
                    OutputAsset {
                        path: o.primary_name.clone(),
                        bytes: out,
                    },
                    OutputAsset {
                        path: path.clone(),
                        bytes: raw,
                    },
                ],
            )
        } else {
            Ok(OutputBundle::single(o.primary_name.clone(), out))
        }
    })();
    finish(result, report, "riscos", o.strict)
}
