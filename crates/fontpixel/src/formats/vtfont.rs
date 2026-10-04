//! FreeBSD VFNT0002 fonts with native half-glyph and bold fallback semantics.
use super::legacy_common::*;
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::bitmap::{Bitmap, Bounds, Mono};
use crate::model::{Codepoint, Font, FontSet, Glyph, Property, PropertyValue, Spacing, Vector};
use crate::resources::{OutputBundle, ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
use std::collections::{BTreeMap, BTreeSet};
mod reader;
mod writer;
/// Native input policy.
#[derive(Debug, Clone, Default)]
pub struct VtfontReadOptions {}
/// Native bounded output choices.
#[derive(Debug, Clone)]
pub struct VtfontWriteOptions {
    /// Width of one half-cell; absent preserves source metadata or derives it.
    pub cell_width: Option<u8>,
    /// Height of one cell; absent preserves source metadata or derives it.
    pub cell_height: Option<u8>,
    /// Reuse native pool/maps only if a complete fresh model comparison proves unchanged.
    pub preserve_native: bool,
    /// Explicit permission to interpret legacy numbers as Unicode scalars.
    pub legacy_as_unicode: bool,
    /// Explicit threshold for grayscale input.
    pub gray_threshold: Option<u8>,
    /// Shared resource limits.
    pub limits: ResourceLimits,
    /// Output cap, also bounded by max_input_bytes.
    pub max_output_bytes: usize,
}
impl Default for VtfontWriteOptions {
    fn default() -> Self {
        Self {
            cell_width: None,
            cell_height: None,
            preserve_native: true,
            legacy_as_unicode: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec.
#[derive(Debug, Clone, Default)]
pub struct VtfontCodec {
    /// Native input policy.
    pub read_options: VtfontReadOptions,
    /// Native output policy.
    pub write_options: VtfontWriteOptions,
}
impl FormatDetector for VtfontCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(if c.bytes().starts_with(b"VFNT0002") {
            Confidence::Certain
        } else {
            Confidence::NoMatch
        })
    }
}
impl FormatReader<FontSet> for VtfontCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for VtfontCodec {
    fn write(&self, s: &FontSet, w: &WriteOptions) -> Result<Written> {
        write(s, w, &self.write_options)
    }
}
/// Decode effective mappings, including independent bold fallback for each half.
pub fn read(c: &mut ReadContext<'_>, _: &VtfontReadOptions) -> Result<(FontSet, Report)> {
    let mut b = Budget::new(c.retained_input_bytes(), c.limits())?;
    reader::parse(c.bytes(), c.limits(), &mut b)
}
/// Encode native rasters and four sorted mapping tables, auditing a same-limit reread.
pub fn write(s: &FontSet, w: &WriteOptions, o: &VtfontWriteOptions) -> Result<Written> {
    let mut r = Report::default();
    let result = writer::encode(s, w, o, &mut r);
    finish(result, r, "vtfont", w.strict)
}
