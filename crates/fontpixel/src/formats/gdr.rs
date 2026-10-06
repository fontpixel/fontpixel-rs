//! Native Psion/EPOC direct-store GDR bitmap fonts.
use super::{Confidence, FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
use crate::model::FontSet;
use crate::resources::{ReadContext, Written};
use crate::{Report, ResourceLimits, Result};
mod reader;
mod wire;
mod writer;
pub use reader::read;
pub use writer::write;
/// Supported native strike descriptor versions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum GdrVersion {
    /// Nokia 7650 shared-metric layout.
    V39,
    /// Symbian 7.0 per-character inline metrics.
    V40,
    /// EPOC/Symbian layout used by the published fnttran compiler.
    #[default]
    V41,
    /// Symbian layout with capital/max ascent and standard/max descent plus line gap.
    V42,
}
/// Input character interpretation.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct GdrReadOptions {
    /// Interpret native 16-bit character codes as Unicode, default true.
    pub unicode: bool,
}
impl Default for GdrReadOptions {
    fn default() -> Self {
        Self { unicode: true }
    }
}
/// Bounded native GDR output choices.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields, default))]
pub struct GdrWriteOptions {
    /// Native strike descriptor layout.
    pub version: GdrVersion,
    /// Collection UID; absent preserves `gdr.collection_uid`, or zero.
    pub collection_uid: Option<u32>,
    /// Use Unicode mappings, or explicitly interpret legacy code numbers.
    pub unicode: bool,
    /// Explicit threshold for grayscale input.
    pub gray_threshold: Option<u8>,
    /// Shared allocation/geometry limits.
    pub limits: ResourceLimits,
    /// Maximum encoded file length.
    pub max_output_bytes: usize,
}
impl Default for GdrWriteOptions {
    fn default() -> Self {
        Self {
            version: GdrVersion::V41,
            collection_uid: None,
            unicode: true,
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
pub struct GdrCodec {
    /// Character interpretation.
    pub read_options: GdrReadOptions,
    /// Output choices.
    pub write_options: GdrWriteOptions,
}
impl FormatDetector for GdrCodec {
    fn detect(&self, c: &mut ReadContext<'_>) -> Result<Confidence> {
        Ok(if c.bytes().starts_with(&wire::MAGIC) {
            Confidence::Certain
        } else {
            Confidence::NoMatch
        })
    }
}
impl FormatReader<FontSet> for GdrCodec {
    fn read(&self, c: &mut ReadContext<'_>, _: &ReadOptions) -> Result<(FontSet, Report)> {
        read(c, &self.read_options)
    }
}
impl FormatWriter<FontSet> for GdrCodec {
    fn write(&self, s: &FontSet, w: &WriteOptions) -> Result<Written> {
        write(s, w, &self.write_options)
    }
}
impl GdrCodec {
    /// [`write`](FormatWriter::write) with the model reserved by the caller.
    pub(crate) fn write_reserved(&self, s: &FontSet, w: &WriteOptions) -> Result<Written> {
        writer::write_charged(s, w, &self.write_options, super::ModelCharge::Reserved)
    }
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;
    use crate::formats::serde_test::{defaults, rejects, round_trip};

    #[test]
    fn options_serde_defaults_and_rejections() {
        assert_eq!(round_trip(&GdrVersion::V39), "v39");
        assert_eq!(round_trip(&GdrVersion::V40), "v40");
        assert_eq!(round_trip(&GdrVersion::V41), "v41");
        assert_eq!(round_trip(&GdrVersion::V42), "v42");
        rejects::<GdrVersion>(r#""not_a_variant""#);
        defaults::<GdrReadOptions>();
        defaults::<GdrWriteOptions>();
        defaults::<GdrCodec>();
    }
}
