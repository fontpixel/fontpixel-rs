//! Explicit KBITX/PBF input choices and PBF/HBF output geometry.
use crate::options::RasterOptions;
use crate::options::{check_choice, check_range};
use fontpixel::{Error, FontRegistry, Result};

/// Read options: explicit KBITX/PBF input choices and PBF/HBF output geometry.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct ReadOptions {
    /// Explicitly reduce KBITX grayscale during import; default retains exact samples.
    #[cfg_attr(feature = "clap", arg(long))]
    pub kbitx_read_threshold: Option<u8>,
    /// Crop PBF blank margins during import, with a loss report.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pbf_trim: bool,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(ReadOptions);
impl ReadOptions {
    /// Check the value constraints the command-line parser enforces; this
    /// group has none beyond its field types.
    pub fn validate(&self) -> fontpixel::Result<()> {
        Ok(())
    }
}
impl ReadOptions {
    pub(crate) fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        #[cfg(feature = "kbitx")]
        registry.set_kbitx_read_options(fontpixel::formats::kbitx::KbitxReadOptions {
            gray_threshold: self.kbitx_read_threshold,
        });
        #[cfg(not(feature = "kbitx"))]
        if self.kbitx_read_threshold.is_some() {
            return Err(Error::FeatureDisabled {
                format: "kbitx".into(),
            });
        }
        #[cfg(feature = "pbf")]
        registry.set_pbf_read_options(fontpixel::formats::pbf::PbfReadOptions {
            trim: self.pbf_trim,
        });
        #[cfg(not(feature = "pbf"))]
        if self.pbf_trim {
            return Err(Error::FeatureDisabled {
                format: "pbf".into(),
            });
        }
        let _ = registry;
        Ok(())
    }
}
const WRITE_OPTIONS_PBF_VERSION: [&str; 3] = ["1", "2", "3"];
const WRITE_OPTIONS_PBF_COMPRESSION: [&str; 2] = ["raw", "rle4"];
const WRITE_OPTIONS_PBF_OFFSET_WIDTH: [&str; 3] = ["auto", "16", "32"];
const WRITE_OPTIONS_PBF_CODEPOINT_BYTES: [&str; 2] = ["2", "4"];
/// Write options: explicit KBITX/PBF input choices and PBF/HBF output geometry.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct WriteOptions {
    /// Opt into the fontpixel.com BDF dialect, with this fallback family slug.
    #[cfg_attr(feature = "clap", arg(long))]
    pub bdf_catalogue_family: Option<String>,
    /// Pebble output wire version (default 3).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_PBF_VERSION))]
    pub pbf_version: Option<String>,
    /// Pebble bitmap encoding; RLE4 requires v3.
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_PBF_COMPRESSION))]
    pub pbf_compression: Option<String>,
    /// Pebble glyph offset width (default auto).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_PBF_OFFSET_WIDTH))]
    pub pbf_offset_width: Option<String>,
    /// Pebble hash bucket count, 1..255 (default 255).
    #[cfg_attr(feature = "clap", arg(long, value_parser = clap::value_parser!(u8).range(1..)))]
    pub pbf_hash_buckets: Option<u8>,
    /// Pebble Unicode field width; default chooses 2 or 4 bytes.
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_PBF_CODEPOINT_BYTES))]
    pub pbf_codepoint_bytes: Option<String>,
    /// Pebble line-top baseline override in pixels.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pbf_max_height: Option<u8>,
    /// Pebble BMP fallback codepoint, e.g. U+003F.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pbf_wildcard: Option<String>,
    /// Relative HBF bitmap asset name, resolved beside the output header.
    #[cfg_attr(feature = "clap", arg(long))]
    pub hbf_bitmap_name: Option<String>,
    /// Split HBF bitmaps after this many glyphs per asset.
    #[cfg_attr(feature = "clap", arg(long))]
    pub hbf_glyphs_per_file: Option<usize>,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(WriteOptions);
impl WriteOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.pbf_version.as_deref() {
            check_choice("pbf_version", value, &WRITE_OPTIONS_PBF_VERSION)?;
        }
        if let Some(value) = self.pbf_compression.as_deref() {
            check_choice("pbf_compression", value, &WRITE_OPTIONS_PBF_COMPRESSION)?;
        }
        if let Some(value) = self.pbf_offset_width.as_deref() {
            check_choice("pbf_offset_width", value, &WRITE_OPTIONS_PBF_OFFSET_WIDTH)?;
        }
        if let Some(value) = self.pbf_hash_buckets {
            check_range(
                "pbf_hash_buckets",
                i128::from(value),
                1,
                i128::from(u8::MAX),
            )?;
        }
        if let Some(value) = self.pbf_codepoint_bytes.as_deref() {
            check_choice(
                "pbf_codepoint_bytes",
                value,
                &WRITE_OPTIONS_PBF_CODEPOINT_BYTES,
            )?;
        }
        Ok(())
    }
}
impl WriteOptions {
    pub(crate) fn apply(
        &self,
        id: &str,
        raster: &RasterOptions,
        threshold: Option<u8>,
        registry: &mut FontRegistry,
    ) -> Result<()> {
        if let Some(family) = &self.bdf_catalogue_family {
            if family.trim().is_empty() || family.contains(['\n', '\r']) {
                return Err(Error::InvalidInput(
                    "BDF catalogue family must be a nonempty single line".into(),
                ));
            }
            #[cfg(not(feature = "bdf"))]
            return Err(Error::FeatureDisabled {
                format: "bdf".into(),
            });
        }
        #[cfg(feature = "bdf")]
        if id == "bdf" && (self.bdf_catalogue_family.is_some() || threshold.is_some()) {
            registry.set_bdf_write_options(fontpixel::formats::bdf::BdfWriteOptions {
                catalogue_family: self.bdf_catalogue_family.clone(),
                gray_threshold: threshold,
                ..Default::default()
            })?;
        }
        let pbf_requested = self.pbf_version.is_some()
            || self.pbf_compression.is_some()
            || self.pbf_offset_width.is_some()
            || self.pbf_hash_buckets.is_some()
            || self.pbf_codepoint_bytes.is_some()
            || self.pbf_max_height.is_some()
            || self.pbf_wildcard.is_some();
        let hbf_requested = self.hbf_bitmap_name.is_some() || self.hbf_glyphs_per_file.is_some();
        if pbf_requested && id != "pbf" {
            return Err(Error::InvalidInput(
                "PBF output options require --to pbf".into(),
            ));
        }
        if hbf_requested && id != "hbf" {
            return Err(Error::InvalidInput(
                "HBF output options require --to hbf".into(),
            ));
        }
        #[cfg(feature = "pbf")]
        if id == "pbf" {
            use fontpixel::formats::pbf::{Compression, OffsetWidth, PbfWriteOptions, Version};
            let wildcard = self
                .pbf_wildcard
                .as_deref()
                .map(|s| {
                    u16::try_from(crate::options::parse_code(s)?)
                        .map_err(|_| Error::InvalidInput("PBF wildcard must fit u16".into()))
                })
                .transpose()?;
            registry.set_pbf_write_options(PbfWriteOptions {
                version: match self.pbf_version.as_deref() {
                    Some("1") => Version::One,
                    Some("2") => Version::Two,
                    _ => Version::Three,
                },
                compression: if self.pbf_compression.as_deref() == Some("rle4") {
                    Compression::Rle4
                } else {
                    Compression::Raw
                },
                offset_width: match self.pbf_offset_width.as_deref() {
                    Some("16") => OffsetWidth::U16,
                    Some("32") => OffsetWidth::U32,
                    _ => OffsetWidth::Auto,
                },
                hash_buckets: self.pbf_hash_buckets.unwrap_or(255),
                codepoint_bytes: self
                    .pbf_codepoint_bytes
                    .as_deref()
                    .map(|s| if s == "2" { 2 } else { 4 }),
                max_height: self.pbf_max_height,
                wildcard,
                gray_threshold: threshold,
                limits: raster.input_limits(),
                ..Default::default()
            });
        }
        #[cfg(not(feature = "pbf"))]
        if pbf_requested || id == "pbf" {
            return Err(Error::FeatureDisabled {
                format: "pbf".into(),
            });
        }
        #[cfg(feature = "hbf")]
        if id == "hbf" {
            registry.set_hbf_write_options(fontpixel::formats::hbf::HbfWriteOptions {
                bitmap_name: self
                    .hbf_bitmap_name
                    .clone()
                    .map(fontpixel::ResourcePath::new)
                    .transpose()?,
                glyphs_per_file: self.hbf_glyphs_per_file,
                gray_threshold: threshold,
                limits: raster.input_limits(),
            });
        }
        #[cfg(not(feature = "hbf"))]
        if hbf_requested || id == "hbf" {
            return Err(Error::FeatureDisabled {
                format: "hbf".into(),
            });
        }
        #[cfg(feature = "kbitx")]
        if id == "kbitx" {
            registry.set_kbitx_write_options(fontpixel::formats::kbitx::KbitxWriteOptions {
                limits: raster.input_limits(),
                ..Default::default()
            });
        }
        let _ = (raster, threshold, registry);
        Ok(())
    }
}
