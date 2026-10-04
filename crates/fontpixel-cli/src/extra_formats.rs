//! Explicit KBITX/PBF input choices and PBF/HBF output geometry.
use super::RasterArguments;
use clap::Args;
use fontpixel::{Error, FontRegistry, Result};

#[derive(Args, Default)]
pub(crate) struct ReadArguments {
    /// Explicitly reduce KBITX grayscale during import; default retains exact samples.
    #[arg(long)]
    kbitx_read_threshold: Option<u8>,
    /// Crop PBF blank margins during import, with a loss report.
    #[arg(long)]
    pbf_trim: bool,
}
impl ReadArguments {
    pub fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
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
#[derive(Args, Default)]
pub(crate) struct WriteArguments {
    /// Pebble output wire version (default 3).
    #[arg(long, value_parser = ["1", "2", "3"])]
    pbf_version: Option<String>,
    /// Pebble bitmap encoding; RLE4 requires v3.
    #[arg(long, value_parser = ["raw", "rle4"])]
    pbf_compression: Option<String>,
    /// Pebble glyph offset width (default auto).
    #[arg(long, value_parser = ["auto", "16", "32"])]
    pbf_offset_width: Option<String>,
    /// Pebble hash bucket count, 1..255 (default 255).
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..))]
    pbf_hash_buckets: Option<u8>,
    /// Pebble Unicode field width; default chooses 2 or 4 bytes.
    #[arg(long, value_parser = ["2", "4"])]
    pbf_codepoint_bytes: Option<String>,
    /// Pebble line-top baseline override in pixels.
    #[arg(long)]
    pbf_max_height: Option<u8>,
    /// Pebble BMP fallback codepoint, e.g. U+003F.
    #[arg(long)]
    pbf_wildcard: Option<String>,
    /// Relative HBF bitmap asset name, resolved beside the output header.
    #[arg(long)]
    hbf_bitmap_name: Option<String>,
    /// Split HBF bitmaps after this many glyphs per asset.
    #[arg(long)]
    hbf_glyphs_per_file: Option<usize>,
}
impl WriteArguments {
    pub fn apply(
        &self,
        id: &str,
        raster: &RasterArguments,
        threshold: Option<u8>,
        registry: &mut FontRegistry,
    ) -> Result<()> {
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
                    u16::try_from(super::parse_code(s)?)
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
