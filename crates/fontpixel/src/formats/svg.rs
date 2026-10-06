//! Bounded SVG 1.1 path fonts and standalone bitmap/glyph images.
//! No XML DTD, script execution, external resources or implicit grayscale conversion.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter};
use crate::outline::{self, Shape};
use crate::report::{Impact, Level};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Bitmap, Error, FontSet, Glyph, Report, ReportEntry, ResourceLimits, Result};
mod path;
mod reader;
mod writer;
pub use reader::read;
pub use writer::{glyph_image, image, path_data, write};
const NS: &str = "http://www.w3.org/2000/svg";
const FP: &str = "https://fontpixel.org/ns/svg/1";
/// Explicit aggregate XML, geometry and raster budgets.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct Limits {
    /// Input bytes, per-record bytes, glyph count and per-glyph pixels.
    pub resources: ResourceLimits,
    /// Maximum XML nodes, including comments and text.
    pub max_nodes: u32,
    /// Maximum element nesting depth.
    pub max_depth: usize,
    /// Maximum attributes on one element.
    pub max_attributes: usize,
    /// Maximum fonts in one document.
    pub max_fonts: usize,
    /// Total parsed/rasterized path edges, including repeated alias records.
    pub max_total_edges: usize,
    /// Total raster pixels and row overhead across all records.
    pub max_total_pixels: usize,
    /// Per-glyph geometry limits.
    pub geometry: outline::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            resources: ResourceLimits::default(),
            max_nodes: 2_000_000,
            max_depth: 64,
            max_attributes: 128,
            max_fonts: 1024,
            max_total_edges: 32 * 1024 * 1024,
            max_total_pixels: 256 * 1024 * 1024,
            geometry: outline::Limits::default(),
        }
    }
}
/// SVG-font raster projection options.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct ReadOptions {
    /// Explicit pixel size, 1..=4096; otherwise native extension or exhaustive grid.
    pub ppem: Option<u16>,
    /// Select one font by document order; None reads all.
    pub font_index: Option<usize>,
    /// Bounded input and output resources.
    pub limits: Limits,
}
/// Native SVG-font serialization options.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct WriteOptions {
    /// Positive design units per source pixel.
    pub units_per_pixel: u16,
    /// Merged squares, separate squares or true circular dots.
    pub shape: Shape,
    /// Diameter/side relative to a pixel, (0, 1]; merged squares require 1.
    pub dot_size: f64,
    /// Explicit conversion for grayscale glyphs; None rejects Gray8.
    pub gray_threshold: Option<u8>,
    /// Encoded UTF-8 output byte budget.
    pub max_output_bytes: usize,
    /// Input model and geometry budgets.
    pub limits: Limits,
}
impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            units_per_pixel: 100,
            shape: Shape::Square,
            dot_size: 1.,
            gray_threshold: None,
            max_output_bytes: 256 * 1024 * 1024,
            limits: Limits::default(),
        }
    }
}
/// Standalone image options. Paths use y-down coordinates and no font resources.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct ImageOptions {
    /// Palette, integer scale and image/output limits.
    pub image: crate::render::ImageOptions,
    /// Pixel outline shape.
    pub shape: Shape,
    /// Diameter/side relative to one pixel, (0, 1].
    pub dot_size: f64,
    /// Geometry budgets.
    pub limits: outline::Limits,
}
impl Default for ImageOptions {
    fn default() -> Self {
        Self {
            image: Default::default(),
            shape: Shape::Square,
            dot_size: 1.,
            limits: Default::default(),
        }
    }
}
/// Registered SVG-font codec (standalone images are deliberately output-only).
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct SvgCodec {
    /// Font reader options.
    pub read_options: ReadOptions,
    /// Font writer options.
    pub write_options: WriteOptions,
}
impl FormatDetector for SvgCodec {
    fn detect(&self, input: &mut ReadContext<'_>) -> Result<Confidence> {
        let mut limits = self.read_options.limits.clone();
        limits.resources.max_input_bytes = limits
            .resources
            .max_input_bytes
            .min(input.limits().max_input_bytes);
        limits.resources.max_table_bytes = limits
            .resources
            .max_table_bytes
            .min(input.limits().max_table_bytes);
        limits.resources.max_total_bytes = limits
            .resources
            .max_total_bytes
            .min(input.limits().max_total_bytes);
        Ok(reader::detect(input.bytes(), &limits))
    }
}
impl FormatReader<FontSet> for SvgCodec {
    fn read(
        &self,
        input: &mut ReadContext<'_>,
        _: &super::ReadOptions,
    ) -> Result<(FontSet, Report)> {
        let mut o = self.read_options.clone();
        let l = input.limits();
        o.limits.resources.max_input_bytes = o
            .limits
            .resources
            .max_input_bytes
            .min(l.max_input_bytes)
            .min(l.max_total_bytes);
        o.limits.resources.max_table_bytes =
            o.limits.resources.max_table_bytes.min(l.max_table_bytes);
        o.limits.resources.max_total_bytes =
            o.limits.resources.max_total_bytes.min(l.max_total_bytes);
        o.limits.resources.max_glyphs = o.limits.resources.max_glyphs.min(l.max_glyphs);
        o.limits.resources.max_pixels_per_glyph = o
            .limits
            .resources
            .max_pixels_per_glyph
            .min(l.max_pixels_per_glyph);
        o.limits.max_total_pixels = o.limits.max_total_pixels.min(l.max_total_bytes);
        read(input.bytes(), &o)
    }
}
impl FormatWriter<FontSet> for SvgCodec {
    fn write(&self, fonts: &FontSet, options: &super::WriteOptions) -> Result<Written> {
        let (bytes, report) = write(fonts, &self.write_options)?;
        Ok(Written {
            output: OutputBundle::single(options.primary_name.clone(), bytes),
            report,
        })
    }
}
fn invalid(s: impl std::fmt::Display) -> Error {
    Error::InvalidInput(format!("SVG: {s}"))
}
fn bound(n: usize, limit: usize, resource: &'static str) -> Result<()> {
    if n > limit {
        Err(Error::ResourceLimit { resource, limit })
    } else {
        Ok(())
    }
}
fn charge(n: &mut usize, amount: usize, limit: usize, label: &'static str) -> Result<()> {
    *n = n
        .checked_add(amount)
        .ok_or_else(|| invalid("budget overflow"))?;
    bound(*n, limit, label)
}
fn loss(
    r: &mut Report,
    code: &str,
    message: impl Into<String>,
    font: Option<usize>,
    glyph: Option<usize>,
) {
    let mut entry = ReportEntry {
        level: Level::Warning,
        impact: Impact::Loss,
        code: format!("svg.{code}"),
        location: Default::default(),
        message: message.into(),
    };
    entry.location.font_index = font;
    entry.location.glyph_index = glyph;
    r.push(entry);
}
fn valid_char(c: char) -> bool {
    matches!(c as u32, 9|10|13|0x20..=0xd7ff|0xe000..=0xfffd|0x10000..=0x10ffff)
}
fn validate_shape(shape: Shape, size: f64) -> Result<()> {
    if !size.is_finite() || size <= 0. || size > 1. || shape == Shape::Square && size != 1. {
        return Err(invalid(
            "dot size must be in (0,1]; merged squares require 1",
        ));
    }
    Ok(())
}
fn mono(g: &Glyph, threshold: Option<u8>) -> Result<Bitmap> {
    match &g.bitmap {
        crate::model::GlyphBitmap::Mono(b) => Ok(b.clone()),
        crate::model::GlyphBitmap::Gray8(b) => Ok(
            b.to_mono(threshold.ok_or_else(|| invalid("grayscale requires explicit threshold"))?)?
        ),
    }
}
