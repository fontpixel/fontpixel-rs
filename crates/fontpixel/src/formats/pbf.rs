//! Bounded Pebble PBF v1/v2/v3 fonts with independent glyph aliases.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Bounds};
use crate::model::{
    Codepoint, Font, FontCharset, FontMetrics, FontSet, Glyph, Property, PropertyValue, Rational,
    Vector,
};
use crate::report::Impact;
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
mod writer;
pub use writer::write;

/// PBF wire version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Version {
    /// Six-byte metadata, no buckets, 16-bit word offsets and eight-byte glyph headers.
    #[cfg_attr(feature = "serde", serde(rename = "1"))]
    One,
    /// Eight-byte metadata and 32-bit byte offsets.
    #[cfg_attr(feature = "serde", serde(rename = "2"))]
    Two,
    /// Ten-byte metadata with selectable offsets and compression.
    #[default]
    #[cfg_attr(feature = "serde", serde(rename = "3"))]
    Three,
}
/// Bitmap storage; RLE4 is available only in v3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Compression {
    /// Continuous LSB-first pixels.
    #[default]
    Raw,
    /// Low-nibble-first runs of one through eight pixels.
    Rle4,
}
/// Glyph offset field width (v1 always u16 words, v2 always u32 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum OffsetWidth {
    /// Smallest field that fits, for v3.
    #[default]
    Auto,
    /// Two-byte field.
    #[cfg_attr(feature = "serde", serde(rename = "16"))]
    U16,
    /// Four-byte field.
    #[cfg_attr(feature = "serde", serde(rename = "32"))]
    U32,
}
/// Optional catalogue-compatible trimming.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PbfReadOptions {
    /// Crop blank margins, reporting changed rectangles as loss.
    pub trim: bool,
}
/// Explicit, bounded Pebble output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PbfWriteOptions {
    /// Wire version, default v3.
    pub version: Version,
    /// Raw or v3 RLE4.
    pub compression: Compression,
    /// Glyph offset field width.
    pub offset_width: OffsetWidth,
    /// Hash bucket count for v2/v3, between one and 255.
    pub hash_buckets: u8,
    /// Unicode field width, None chooses 2 or 4; v1 only accepts 2.
    pub codepoint_bytes: Option<u8>,
    /// Override line-top baseline; otherwise ascent then pixel size.
    pub max_height: Option<u8>,
    /// Override BMP fallback codepoint.
    pub wildcard: Option<u16>,
    /// Required explicit threshold for gray input.
    pub gray_threshold: Option<u8>,
    /// Aggregate and geometry limits.
    pub limits: ResourceLimits,
    /// Maximum generated file size.
    pub max_output_bytes: usize,
}
impl Default for PbfWriteOptions {
    fn default() -> Self {
        Self {
            version: Version::Three,
            compression: Compression::Raw,
            offset_width: OffsetWidth::Auto,
            hash_buckets: 255,
            codepoint_bytes: None,
            max_height: None,
            wildcard: None,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 256 * 1024 * 1024,
        }
    }
}
/// Registry codec; no filesystem or external resources are accessed.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PbfCodec {
    /// Reader normalization.
    pub read_options: PbfReadOptions,
    /// Writer choices.
    pub write_options: PbfWriteOptions,
}
impl FormatDetector for PbfCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for PbfCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        let (font, mut report) = parse(
            c.bytes(),
            &self.read_options,
            c.limits(),
            c.retained_input_bytes(),
            c.name().map_or(0, str::len),
        )?;
        for e in &mut report.entries {
            e.location.asset = c.name().map(str::to_owned);
        }
        let mut set = FontSet::from(font);
        set.source_format = Some("pbf".into());
        Ok((set, report))
    }
}
impl FormatWriter<FontSet> for PbfCodec {
    fn write(&self, set: &FontSet, o: &WriteOptions) -> Result<Written> {
        let (bytes, mut report) = write(set.single()?, &self.write_options)?;
        let mut cost = bytes.len().saturating_add(
            report
                .entries
                .capacity()
                .saturating_mul(std::mem::size_of::<crate::report::ReportEntry>()),
        );
        for entry in &report.entries {
            cost = cost
                .saturating_add(entry.code.capacity())
                .saturating_add(entry.message.capacity());
        }
        check(
            cost,
            self.write_options.limits.max_total_bytes,
            "PBF diagnostics",
        )?;
        for p in set.properties.iter() {
            cost = cost
                .checked_add(p.name.len().saturating_mul(8).saturating_add(512))
                .ok_or_else(|| {
                    limit("PBF diagnostics", self.write_options.limits.max_total_bytes)
                })?;
            check(
                cost,
                self.write_options.limits.max_total_bytes,
                "PBF diagnostics",
            )?;
            diag(
                &mut report,
                "pbf",
                "write.container_property",
                Impact::Loss,
                None,
                None,
                format!("Container property {:?} has no PBF field", p.name),
            );
        }
        Ok(Written {
            output: OutputBundle::single(o.primary_name.clone(), bytes),
            report,
        })
    }
}
struct Budget {
    used: usize,
    max: usize,
}
impl Budget {
    fn new(n: usize, l: &ResourceLimits) -> Result<Self> {
        let mut b = Self {
            used: 0,
            max: l.max_total_bytes,
        };
        b.add(n)?;
        Ok(b)
    }
    fn add(&mut self, n: usize) -> Result<()> {
        self.used = self
            .used
            .checked_add(n)
            .ok_or_else(|| limit("PBF aggregate bytes", self.max))?;
        check(self.used, self.max, "PBF aggregate bytes")
    }
    fn items<T>(&mut self, n: usize) -> Result<()> {
        self.add(
            n.checked_mul(std::mem::size_of::<T>())
                .and_then(|v| v.checked_mul(2))
                .ok_or_else(|| limit("PBF aggregate bytes", self.max))?,
        )
    }
}
#[derive(Clone, Copy)]
struct Header {
    version: Version,
    height: u8,
    count: usize,
    wildcard: u16,
    buckets: usize,
    cp: usize,
    off: usize,
    size: usize,
    rle: bool,
}
fn get16(b: &[u8], at: usize) -> Result<u16> {
    let a = b
        .get(at..at + 2)
        .ok_or_else(|| invalid("truncated PBF u16"))?;
    Ok(u16::from_le_bytes([a[0], a[1]]))
}
fn number(b: &[u8], at: usize, n: usize) -> Result<u32> {
    match n {
        2 => Ok(u32::from(get16(b, at)?)),
        4 => {
            let a = b
                .get(at..at + 4)
                .ok_or_else(|| invalid("truncated PBF u32"))?;
            Ok(u32::from_le_bytes([a[0], a[1], a[2], a[3]]))
        }
        _ => Err(invalid("invalid PBF field width")),
    }
}
fn header(b: &[u8]) -> Result<Header> {
    let a = b.get(..6).ok_or_else(|| invalid("truncated PBF header"))?;
    let mut h = Header {
        version: Version::One,
        height: a[1],
        count: get16(a, 2)? as usize,
        wildcard: get16(a, 4)?,
        buckets: 0,
        cp: 2,
        off: 2,
        size: 6,
        rle: false,
    };
    if h.count == 0 {
        return Err(invalid("PBF has no mappings"));
    }
    if char::from_u32(u32::from(h.wildcard)).is_none() {
        return Err(invalid("PBF wildcard is not Unicode"));
    }
    match a[0] {
        1 => {}
        2 | 3 => {
            let a = b
                .get(..8)
                .ok_or_else(|| invalid("truncated PBF v2 header"))?;
            h.version = Version::Two;
            h.buckets = a[6] as usize;
            h.cp = a[7] as usize;
            h.off = 4;
            h.size = 8;
            if h.buckets == 0 || !matches!(h.cp, 2 | 4) {
                return Err(invalid("invalid PBF table geometry"));
            }
            if a[0] == 3 {
                let a = b
                    .get(..10)
                    .ok_or_else(|| invalid("truncated PBF v3 header"))?;
                h.version = Version::Three;
                h.size = a[8] as usize;
                if h.size < 10 || h.size > b.len() || a[9] & !3 != 0 {
                    return Err(invalid("invalid PBF v3 header/features"));
                }
                h.off = if a[9] & 1 != 0 { 2 } else { 4 };
                h.rle = a[9] & 2 != 0;
            }
        }
        _ => return Err(invalid("unsupported PBF version")),
    };
    Ok(h)
}
/// Structural hint only: PBF has no unique magic. A malformed explicit PBF read
/// returns a detailed error; this detector does not allocate or parse bitmaps.
pub fn detect(b: &[u8]) -> Confidence {
    if let Ok(h) = header(b) {
        let base = h.size + h.buckets * 4 + h.count * (h.cp + h.off);
        if base + 5 <= b.len() {
            return Confidence::Possible;
        }
    }
    Confidence::NoMatch
}
/// Decode bounded raw or RLE4 glyphs, retaining the native bitmap rectangle.
pub fn read(
    bytes: &[u8],
    options: &PbfReadOptions,
    limits: &ResourceLimits,
) -> Result<(Font, Report)> {
    check(bytes.len(), limits.max_input_bytes, "PBF input bytes")?;
    parse(bytes, options, limits, bytes.len(), 0)
}
fn parse(
    bytes: &[u8],
    options: &PbfReadOptions,
    l: &ResourceLimits,
    retained: usize,
    asset_len: usize,
) -> Result<(Font, Report)> {
    let h = header(bytes)?;
    check(h.count, l.max_glyphs, "PBF mappings")?;
    let entry = h.cp + h.off;
    let start = h.size + h.buckets * 4;
    let data = start + h.count * entry;
    check(data, l.max_table_bytes, "PBF mapping table")?;
    if data > bytes.len() {
        return Err(invalid("truncated PBF mapping table"));
    }
    let mut budget = read_base_budget(retained, h.count, asset_len, l)?;
    let mut mappings = Vec::with_capacity(h.count);
    let mut visited = vec![false; h.count];
    let mut report = Report::default();
    let mut unsorted = false;
    let ranges = if h.buckets == 0 { 1 } else { h.buckets };
    for bucket in 0..ranges {
        let (offset, count) = if h.buckets == 0 {
            (0, h.count)
        } else {
            let p = h.size + bucket * 4;
            if bytes[p] as usize != bucket {
                return Err(invalid("PBF hash bucket ID disagrees with its index"));
            }
            (get16(bytes, p + 2)? as usize, bytes[p + 1] as usize)
        };
        if offset % entry != 0 || offset / entry + count > h.count {
            return Err(invalid("PBF bucket range outside mapping table"));
        }
        let mut previous = None;
        for (i, occupied) in visited
            .iter_mut()
            .enumerate()
            .skip(offset / entry)
            .take(count)
        {
            if *occupied {
                return Err(invalid("overlapping PBF hash buckets"));
            }
            *occupied = true;
            let p = start + i * entry;
            let cp = number(bytes, p, h.cp)?;
            if char::from_u32(cp).is_none() {
                return Err(invalid("PBF codepoint is not a Unicode scalar"));
            }
            if h.buckets != 0 && cp as usize % h.buckets != bucket {
                return Err(invalid("PBF codepoint in wrong hash bucket"));
            }
            if previous.is_some_and(|v| v >= cp) {
                unsorted = true;
            }
            previous = Some(cp);
            let offset = number(bytes, p + h.cp, h.off)? as usize;
            let offset = if h.version == Version::One {
                offset * 4
            } else {
                offset
            };
            mappings.push((cp, offset));
        }
    }
    if visited.iter().any(|v| !*v) {
        return Err(invalid("PBF buckets do not cover every mapping"));
    }
    mappings.sort_unstable();
    if mappings.windows(2).any(|w| w[0].0 == w[1].0) {
        return Err(invalid("duplicate PBF codepoint"));
    }
    mappings.sort_unstable_by_key(|&(cp, off)| (off, cp));
    if unsorted {
        diag(
            &mut report,
            "pbf",
            "read.unsorted_table",
            Impact::Repair,
            None,
            None,
            "Mapping table is not sorted for Pebble binary search",
        );
    }
    if mappings[0].1 == 0 {
        diag(
            &mut report,
            "pbf",
            "read.zero_offset",
            Impact::None,
            None,
            None,
            "Offset zero is a glyph in this file but means missing to the Pebble runtime",
        );
    }
    let mut font = Font::default();
    font.source_format = Some("pbf".into());
    font.charset = FontCharset {
        name: Some("unicode".into()),
        ..Default::default()
    };
    if h.size > 10 {
        budget.add(h.size - 10 + 128)?;
        font.headers.push(Property::new(
            "pbf.header_extra",
            PropertyValue::Bytes(bytes[10..h.size].to_vec()),
        ));
        diag(
            &mut report,
            "pbf",
            "read.extended_header",
            Impact::None,
            None,
            None,
            "Extended PBF header is preserved; the official Pebble runtime requires ten bytes",
        );
    }
    let mut i = 0;
    let mut previous_end = 0;
    let mut descent = 0;
    while i < mappings.len() {
        let offset = mappings[i].1;
        if offset < previous_end {
            return Err(invalid("overlapping PBF glyph records"));
        }
        let mut end = i + 1;
        while end < mappings.len() && mappings[end].1 == offset {
            end += 1;
        }
        let p = data
            .checked_add(offset)
            .ok_or_else(|| invalid("PBF glyph offset overflow"))?;
        let n = if h.version == Version::One { 8 } else { 5 };
        let record = bytes
            .get(
                p..p.checked_add(n)
                    .ok_or_else(|| invalid("PBF glyph offset overflow"))?,
            )
            .ok_or_else(|| invalid("PBF glyph header outside input"))?;
        let w = u32::from(record[0]);
        let units = usize::from(record[1]);
        let payload = if h.rle {
            units.div_ceil(2)
        } else {
            (w as usize * units).div_ceil(8)
        };
        let raw = bytes
            .get(
                p + n
                    ..(p + n)
                        .checked_add(payload)
                        .ok_or_else(|| invalid("PBF bitmap end overflow"))?,
            )
            .ok_or_else(|| invalid("truncated PBF bitmap"))?;
        let height = if h.rle {
            let pixels = (0..units)
                .map(|j| usize::from((raw[j / 2] >> (4 * (j % 2))) & 7) + 1)
                .sum::<usize>();
            if w == 0 {
                if pixels != 0 {
                    return Err(invalid("nonempty PBF RLE with zero width"));
                }
                0
            } else {
                if pixels % w as usize != 0 {
                    diag(
                        &mut report,
                        "pbf",
                        "read.rle_tail",
                        Impact::Loss,
                        Some(font.len()),
                        Some(mappings[i].0),
                        format!(
                            "Discarded {} RLE pixels after the final complete row, matching Pebble runtime rendering",
                            pixels % w as usize
                        ),
                    );
                }
                pixels / w as usize
            }
        } else {
            units
        };
        if height > 255 {
            return Err(invalid("decoded PBF height exceeds u8"));
        }
        check(
            w as usize * height,
            l.max_pixels_per_glyph,
            "PBF glyph pixels",
        )?;
        let stride = (w as usize).div_ceil(8);
        let bitmap_bytes = stride * height;
        budget.add(bitmap_bytes * if options.trim { 3 } else { 2 })?;
        budget.items::<Codepoint>(end - i)?;
        let mut raster = vec![0; bitmap_bytes];
        if h.rle {
            let mut pixel = 0;
            for j in 0..units {
                let unit = (raw[j / 2] >> (4 * (j % 2))) & 15;
                for _ in 0..usize::from((unit & 7) + 1) {
                    if unit & 8 != 0 && pixel < w as usize * height {
                        raster[pixel / w as usize * stride + (pixel % w as usize) / 8] |=
                            128 >> ((pixel % w as usize) % 8);
                    }
                    pixel += 1;
                }
            }
        } else if w != 0 {
            for y in 0..height {
                for x in 0..w as usize {
                    let bit = y * w as usize + x;
                    if raw[bit / 8] & (1 << (bit % 8)) != 0 {
                        raster[y * stride + x / 8] |= 128 >> (x % 8);
                    }
                }
            }
        }
        let mut bitmap: Bitmap = Bitmap::from_raw(w, height as u32, &raster, bitmap_limits(l))?;
        let mut bounds = Bounds::new(
            w,
            height as u32,
            i64::from(record[2] as i8),
            i64::from(h.height) - i64::from(record[3] as i8) - height as i64,
        );
        if options.trim {
            let ink = bitmap.bbox();
            if ink.width != w
                || ink.height != height as u32
                || (ink.width == 0 && bounds != Bounds::default())
            {
                diag(
                    &mut report,
                    "pbf",
                    "read.trim",
                    Impact::Loss,
                    Some(font.len()),
                    Some(mappings[i].0),
                    "Blank bitmap margins were trimmed",
                );
                bitmap = bitmap.crop_bounds(ink)?;
                bounds = if ink.width == 0 {
                    Bounds::default()
                } else {
                    Bounds::new(ink.width, ink.height, bounds.x + ink.x, bounds.y + ink.y)
                };
            }
        }
        if bitmap.has_ink() {
            descent = descent.max(-bounds.y);
        }
        let mut glyph = Glyph::new(bitmap);
        glyph.metrics.bounds = bounds;
        glyph.metrics.advance = Some(Vector::new(i64::from(record[n - 1] as i8), 0));
        glyph.codepoints = mappings[i..end]
            .iter()
            .map(|v| Codepoint::Unicode(v.0))
            .collect();
        if h.version == Version::One && record[4..7] != [0, 0, 0] {
            budget.add(160)?;
            glyph.properties.push(Property::new(
                "pbf.v1_padding",
                PropertyValue::Bytes(record[4..7].to_vec()),
            ));
        }
        font.add_glyph(glyph)?;
        previous_end = offset + n + payload;
        i = end;
    }
    font.metrics = FontMetrics {
        pixel_size: Some(u32::from(h.height)),
        point_size: Some(Rational::integer(i64::from(h.height))),
        resolution: Some(Vector::new(72, 72)),
        ascent: Some(i64::from(h.height)),
        descent: Some(descent.max(0)),
        default_character: Some(Codepoint::Unicode(u32::from(h.wildcard))),
        default_glyph: font.glyph_id(Codepoint::Unicode(u32::from(h.wildcard))),
        ..Default::default()
    };
    font.update_bounds()?;
    if font.metrics.default_glyph.is_none() {
        diag(
            &mut report,
            "pbf",
            "read.missing_wildcard",
            Impact::None,
            None,
            Some(u32::from(h.wildcard)),
            "PBF wildcard has no matching glyph",
        );
    }
    Ok((font, report))
}

fn invalid(s: impl Into<String>) -> crate::Error {
    crate::Error::InvalidInput(s.into())
}
fn limit(resource: &'static str, limit: usize) -> crate::Error {
    crate::Error::ResourceLimit { resource, limit }
}
fn check(n: usize, max: usize, resource: &'static str) -> Result<()> {
    if n > max {
        Err(limit(resource, max))
    } else {
        Ok(())
    }
}
fn bitmap_limits(l: &ResourceLimits) -> crate::bitmap::BitmapLimits {
    crate::bitmap::BitmapLimits {
        max_pixels: l.max_pixels_per_glyph,
        max_bytes: l.max_total_bytes,
        max_dimension: 255,
    }
}
fn diag(
    r: &mut Report,
    format: &str,
    code: &str,
    impact: Impact,
    glyph_index: Option<usize>,
    codepoint: Option<u32>,
    message: impl Into<String>,
) {
    r.push(crate::report::ReportEntry {
        level: crate::report::Level::Warning,
        impact,
        code: format!("{format}.{code}"),
        location: crate::report::Location {
            font_index: Some(0),
            glyph_index,
            codepoint,
            asset: None,
        },
        message: message.into(),
    });
}

// Shared allocation-free accounting keeps successful writer output readable
// under the same ResourceLimits, including alias-heavy tables.
fn read_base_budget(
    retained: usize,
    count: usize,
    asset_len: usize,
    l: &ResourceLimits,
) -> Result<Budget> {
    let mut budget = Budget::new(retained, l)?;
    budget.add(
        (count * 2 + 4)
            .checked_mul(asset_len)
            .ok_or_else(|| limit("PBF diagnostic asset bytes", l.max_total_bytes))?,
    )?;
    budget.items::<(u32, usize)>(count)?;
    budget.add(count)?;
    budget.items::<Glyph>(count)?;
    budget.add(count * 1024 + 2048)?;
    Ok(budget)
}
