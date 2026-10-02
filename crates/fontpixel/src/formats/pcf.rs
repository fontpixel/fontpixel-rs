//! X11 Portable Compiled Format, with independent deterministic serialization.
//!
//! Registry calls accept bounded gzip input/output. Direct [`read`] and [`write()`]
//! operate on uncompressed PCF bytes. See `docs/formats/pcf.md` for the exact
//! SPEC-v3/bdftopcf compatibility rules and reported conversion losses.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, BitmapLimits, Bounds, Mono};
use crate::model::{
    Font, FontSet, Glyph, GlyphBitmap, Properties, Property, PropertyValue, Vector,
};
use crate::report::{Impact, Level, Location, ReportEntry};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Error, Report, ResourceLimits, Result};

mod reader;
mod writer;

/// Byte/bit significance order, independently selectable in PCF.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Order {
    /// Most significant byte/bit first (the default).
    #[default]
    Msb,
    /// Least significant byte/bit first.
    Lsb,
}
/// Alignment of each bitmap row in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GlyphPad {
    /// One byte.
    One,
    /// Two bytes.
    Two,
    /// Four bytes (the default).
    #[default]
    Four,
    /// Eight bytes.
    Eight,
}
impl GlyphPad {
    fn index(self) -> u32 {
        match self {
            Self::One => 0,
            Self::Two => 1,
            Self::Four => 2,
            Self::Eight => 3,
        }
    }
}
/// Bitmap byte-swap unit when bit and byte order differ.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScanUnit {
    /// One byte (the default).
    #[default]
    One,
    /// Two bytes.
    Two,
    /// Four bytes.
    Four,
}
impl ScanUnit {
    fn index(self) -> u32 {
        match self {
            Self::One => 0,
            Self::Two => 1,
            Self::Four => 2,
        }
    }
}
/// PCF-specific deterministic output choices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcfWriteOptions {
    /// Integer byte order inside tables.
    pub byte_order: Order,
    /// Bitmap bit order.
    pub bit_order: Order,
    /// Per-row byte alignment.
    pub glyph_pad: GlyphPad,
    /// Bitmap byte-swap unit.
    pub scan_unit: ScanUnit,
    /// Explicit Gray8 threshold; None refuses grayscale input.
    pub gray_threshold: Option<u8>,
    /// Maximum serialized bytes, enforced before output allocations.
    pub max_output_bytes: usize,
    /// Budgets for glyphs, intermediate tables, pixels and model data.
    pub limits: ResourceLimits,
}
impl Default for PcfWriteOptions {
    fn default() -> Self {
        Self {
            byte_order: Order::Msb,
            bit_order: Order::Msb,
            glyph_pad: GlyphPad::Four,
            scan_unit: ScanUnit::One,
            gray_threshold: None,
            max_output_bytes: 256 * 1024 * 1024,
            limits: ResourceLimits::default(),
        }
    }
}
/// Actual PCF registry hooks, with strongly typed writer configuration.
#[derive(Debug, Clone, Default)]
pub struct PcfCodec {
    /// Options for registry writes.
    pub write_options: PcfWriteOptions,
}
impl FormatDetector for PcfCodec {
    fn detect(&self, input: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(detect(input.bytes()))
    }
}
impl FormatReader<FontSet> for PcfCodec {
    fn read(
        &self,
        input: &mut ReadContext<'_>,
        _options: &ReadOptions,
    ) -> Result<(FontSet, Report)> {
        let (font, mut report) = read(input.bytes(), input.limits())?;
        for entry in &mut report.entries {
            entry.location.asset = input.name().map(str::to_owned);
        }
        let mut set = FontSet::from(font);
        set.source_format = Some("pcf".into());
        Ok((set, report))
    }
}
impl FormatWriter<FontSet> for PcfCodec {
    fn write(&self, fonts: &FontSet, options: &WriteOptions) -> Result<Written> {
        let (bytes, mut report) = write(fonts.single()?, &self.write_options)?;
        for p in fonts.properties.iter() {
            loss(
                &mut report,
                "container_property",
                None,
                format!("PCF cannot store container property {:?}", p.name),
            );
        }
        Ok(Written {
            output: OutputBundle::single(options.primary_name.clone(), bytes),
            report,
        })
    }
}
/// Recognize the four-byte PCF magic (gzip is handled by the registry transport).
pub fn detect(bytes: &[u8]) -> Confidence {
    if bytes.starts_with(MAGIC) {
        Confidence::Certain
    } else {
        Confidence::NoMatch
    }
}
/// Decode one bounded uncompressed PCF strike into the real model.
pub fn read(bytes: &[u8], limits: &ResourceLimits) -> Result<(Font, Report)> {
    reader::read(bytes, limits)
}
/// Encode a strike without executing external tools; direct calls return losses.
/// Use registry `write_with_report` for strict rejection of combined read/write loss.
pub fn write(font: &Font, options: &PcfWriteOptions) -> Result<(Vec<u8>, Report)> {
    writer::write(font, options)
}

const MAGIC: &[u8] = b"\x01fcp";
const PROPERTIES: u32 = 1;
const ACCELERATORS: u32 = 2;
const METRICS: u32 = 4;
const BITMAPS: u32 = 8;
const INK_METRICS: u32 = 16;
const ENCODINGS: u32 = 32;
const SWIDTHS: u32 = 64;
const GLYPH_NAMES: u32 = 128;
const BDF_ACCELERATORS: u32 = 256;
type Metric = [i32; 6];
fn invalid(message: impl Into<String>) -> Error {
    Error::InvalidInput(message.into())
}
fn limit(resource: &'static str, maximum: usize) -> Error {
    Error::ResourceLimit {
        resource,
        limit: maximum,
    }
}
fn bound(size: usize, maximum: usize, resource: &'static str) -> Result<()> {
    if size > maximum {
        Err(limit(resource, maximum))
    } else {
        Ok(())
    }
}
fn loss(report: &mut Report, code: &str, glyph: Option<usize>, message: String) {
    report.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: format!("pcf.write.{code}"),
        location: Location {
            font_index: Some(0),
            glyph_index: glyph,
            ..Location::default()
        },
        message,
    });
}
fn notice(report: &mut Report, code: &str, message: String) {
    report.push(ReportEntry {
        level: Level::Warning,
        impact: Impact::None,
        code: format!("pcf.read.{code}"),
        location: Location {
            font_index: Some(0),
            ..Location::default()
        },
        message,
    });
}
fn integer(v: &PropertyValue) -> Option<i64> {
    match v {
        PropertyValue::Integer(n) => Some(*n),
        PropertyValue::Unsigned(n) => (*n).try_into().ok(),
        _ => None,
    }
}
fn prop_int(p: &Properties, name: &str) -> Option<i64> {
    p.get_case_insensitive(name).and_then(integer)
}
fn list(values: impl IntoIterator<Item = i64>) -> PropertyValue {
    PropertyValue::List(values.into_iter().map(PropertyValue::Integer).collect())
}
fn row_bytes(width: usize, pad: usize) -> usize {
    width.div_ceil(pad * 8) * pad
}
fn round4(n: usize) -> usize {
    n.div_ceil(4) * 4
}
fn bitmap_limits(limits: &ResourceLimits) -> BitmapLimits {
    BitmapLimits {
        max_dimension: 65535,
        max_pixels: limits.max_pixels_per_glyph,
        max_bytes: limits.max_table_bytes.min(limits.max_total_bytes),
    }
}
fn swap_units(bytes: &mut [u8], unit: usize) {
    for chunk in bytes.chunks_mut(unit) {
        if chunk.len() == unit {
            chunk.reverse();
        } else {
            let mut padded = [0u8; 4];
            padded[..chunk.len()].copy_from_slice(chunk);
            padded[..unit].reverse();
            chunk.copy_from_slice(&padded[..chunk.len()]);
        }
    }
}
fn initial_bounds() -> (Metric, Metric) {
    (
        [32767, 32767, 32767, 32767, 32767, 65535],
        [-32768, -32768, -32768, -32768, -32768, 0],
    )
}
fn minmax(min: &mut Metric, max: &mut Metric, m: &Metric) {
    if m[..5].iter().any(|&v| v != 0) {
        for i in 0..5 {
            min[i] = min[i].min(m[i]);
            max[i] = max[i].max(m[i]);
        }
    }
    min[5] &= m[5];
    max[5] |= m[5];
}
fn swidth(pointsize: f64, xres: i64, width: i32, font_width: u32) -> i64 {
    let x = if pointsize != 0.0 && xres != 0 {
        f64::from(width) * 72000.0 / (pointsize * xres as f64)
    } else {
        f64::from(width) * 1000.0 / f64::from(font_width.max(1))
    };
    (x + 0.5).floor() as i64
}

// Canonical big-endian accelerator field provenance, independent of each source
// table's byte order. Flags are bytes, followed by three i32 and i16 metrics.
fn accelerator_fields(bytes: &[u8], msb: bool) -> Vec<u8> {
    let mut result = bytes.to_vec();
    if !msb && result.len() >= 20 {
        for word in result[8..20].as_chunks_mut::<4>().0 {
            word.reverse();
        }
        for word in result[20..].as_chunks_mut::<2>().0 {
            word.reverse();
        }
    }
    result
}
