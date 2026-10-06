//! Palm OS bitmap resources, record databases and resource databases.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::model::FontSet;
use crate::resources::{ReadContext, Written};
use crate::{Report, ResourceLimits, Result};

mod gray;
mod reader;
mod writer;
pub use reader::read;
pub use writer::write;

/// Palm font wrapper.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PalmContainer {
    /// One bare NFNT or multi-density nfnt resource.
    Bare,
    /// Font/Font record database.
    Pdb,
    /// Resource database, conventionally a PRC file.
    #[default]
    Prc,
}
/// Bitmap resource version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PalmVersion {
    /// Classic 72-dpi NFNT.
    #[default]
    #[cfg_attr(feature = "serde", serde(rename = "1"))]
    V1,
    /// Extended nfnt with density records.
    #[cfg_attr(feature = "serde", serde(rename = "2"))]
    V2,
    /// Little-endian ARM native extended resource.
    Afnx,
    /// FontSubst afnx variant with fontType 0x0200.
    #[cfg_attr(feature = "serde", serde(rename = "fontsubst"))]
    FontSubst,
    /// GrayFont version 1 in a PRC resource database.
    Gray1,
    /// GrayFont version 2 in a PRC resource database.
    Gray2,
}
/// Interpretation of fractional 108-dpi coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PalmRounding {
    /// Require every scaled coordinate to be integral.
    #[default]
    Exact,
    /// Explicitly truncate scaled coordinates toward zero.
    Floor,
}
/// Explicit Palm input choices.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PalmReadOptions {
    /// Override automatic wrapper recognition.
    pub container: Option<PalmContainer>,
    /// Fractional density coordinates are ambiguous in the published resource specification.
    pub rounding: PalmRounding,
}
/// Bounded native Palm output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PalmWriteOptions {
    /// Output wrapper.
    pub container: PalmContainer,
    /// Bitmap layout.
    pub version: PalmVersion,
    /// Density per model strike; empty uses `palm.density`, or 72.
    pub densities: Vec<u16>,
    /// Explicit permission to use Unicode numbers as Palm legacy character codes.
    pub unicode_as_legacy: bool,
    /// Explicit grayscale threshold; absent rejects grayscale.
    pub gray_threshold: Option<u8>,
    /// Allocation and geometry limits, also applied to output decoding.
    pub limits: ResourceLimits,
    /// Maximum output bytes, also bounded by input limits.
    pub max_output_bytes: usize,
}
impl Default for PalmWriteOptions {
    fn default() -> Self {
        Self {
            container: PalmContainer::Prc,
            version: PalmVersion::V1,
            densities: Vec::new(),
            unicode_as_legacy: false,
            gray_threshold: None,
            limits: ResourceLimits::default(),
            max_output_bytes: 64 * 1024 * 1024,
        }
    }
}
/// Registry codec with independent read/write choices.
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct PalmCodec {
    /// Reader options.
    pub read_options: PalmReadOptions,
    /// Writer options.
    pub write_options: PalmWriteOptions,
}
impl FormatDetector for PalmCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(reader::detect(c.bytes()))
    }
}
impl FormatReader<FontSet> for PalmCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for PalmCodec {
    fn write(&self, s: &FontSet, w: &WriteOptions) -> Result<Written> {
        write(s, w, &self.write_options)
    }
}
