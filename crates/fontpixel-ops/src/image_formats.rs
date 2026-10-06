//! Explicit atlas and native image-directory options.
use fontpixel::{Error, FontRegistry, Result};

/// Read options: explicit atlas and native image-directory options.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct ReadOptions {
    /// Relative JSON/TOML atlas mapping, resolved beside the primary input.
    #[cfg_attr(feature = "clap", arg(long))]
    pub image_mapping: Option<String>,
    /// Literal prefix on numeric glyph filenames in an input directory.
    #[cfg_attr(feature = "clap", arg(long))]
    pub image_read_prefix: Option<String>,
    /// Input directory filename radix (default 16).
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u32).range(2..=36)))]
    pub image_read_radix: Option<u32>,
    /// Input filename code domain (default legacy).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["legacy", "unicode"]))]
    pub image_read_domain: Option<String>,
    /// Input image background (default black).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["black", "white", "alpha"]))]
    pub image_read_background: Option<String>,
    /// Input glyph baseline measured down from each image's top.
    #[cfg_attr(feature = "clap", arg(long, allow_hyphen_values = true))]
    pub image_read_baseline: Option<i64>,
}
impl ReadOptions {
    pub(crate) fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        #[cfg(feature = "image")]
        registry.set_image_read_options(fontpixel::formats::image::ImageReadOptions {
            mapping: None,
            mapping_path: self
                .image_mapping
                .as_deref()
                .map(fontpixel::ResourcePath::new)
                .transpose()?,
            imageset: set_options(
                self.image_read_prefix.as_deref(),
                self.image_read_radix,
                self.image_read_domain.as_deref(),
                self.image_read_background.as_deref(),
                self.image_read_baseline,
            ),
        });
        #[cfg(not(feature = "image"))]
        if self.image_mapping.is_some()
            || self.image_read_prefix.is_some()
            || self.image_read_radix.is_some()
            || self.image_read_domain.is_some()
            || self.image_read_background.is_some()
            || self.image_read_baseline.is_some()
        {
            return Err(Error::FeatureDisabled {
                format: "image".into(),
            });
        }
        let _ = registry;
        Ok(())
    }
}

/// Write options: explicit atlas and native image-directory options.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct WriteOptions {
    /// Output image organization (default atlas).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["atlas", "imageset"]))]
    pub image_layout: Option<String>,
    /// Explicit image container (directory default png; atlas uses extension).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["png", "bmp", "gif", "tga", "pbm", "pgm"]))]
    pub image_encoding: Option<String>,
    /// Atlas columns (default 16).
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u32).range(1..)))]
    pub image_columns: Option<u32>,
    /// Atlas mapping format (default json).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["json", "toml"]))]
    pub image_mapping_format: Option<String>,
    /// Literal prefix on numeric output glyph filenames.
    #[cfg_attr(feature = "clap", arg(long))]
    pub image_prefix: Option<String>,
    /// Output directory filename radix (default 16).
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u32).range(2..=36)))]
    pub image_radix: Option<u32>,
    /// Output filename code domain (default legacy).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["legacy", "unicode"]))]
    pub image_domain: Option<String>,
    /// Output image background (default black).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["black", "white", "alpha"]))]
    pub image_background: Option<String>,
    /// Output glyph baseline measured down from the image top.
    #[cfg_attr(feature = "clap", arg(long, allow_hyphen_values = true))]
    pub image_baseline: Option<i64>,
}
impl WriteOptions {
    pub(crate) fn apply(
        &self,
        id: &str,
        raster: &crate::options::RasterOptions,
        threshold: Option<u8>,
        registry: &mut FontRegistry,
    ) -> Result<()> {
        let set_explicit = self.image_prefix.is_some()
            || self.image_radix.is_some()
            || self.image_domain.is_some()
            || self.image_background.is_some()
            || self.image_baseline.is_some();
        let atlas_explicit = self.image_columns.is_some() || self.image_mapping_format.is_some();
        let explicit = set_explicit
            || atlas_explicit
            || self.image_layout.is_some()
            || self.image_encoding.is_some();
        if id != "image" && explicit {
            return Err(Error::InvalidInput(
                "image output options require --to image".into(),
            ));
        }
        let imageset = self.image_layout.as_deref() == Some("imageset");
        if set_explicit && !imageset {
            return Err(Error::InvalidInput(
                "image directory options require --image-layout imageset".into(),
            ));
        }
        if imageset && atlas_explicit {
            return Err(Error::InvalidInput(
                "atlas columns and mapping options require --image-layout atlas".into(),
            ));
        }
        #[cfg(feature = "image")]
        if id == "image" {
            use fontpixel::formats::image::{
                Encoding, ImageLayout, ImageWriteOptions, MappingFormat,
            };
            registry.set_image_write_options(ImageWriteOptions {
                layout: if imageset {
                    ImageLayout::Imageset
                } else {
                    ImageLayout::Atlas
                },
                imageset: set_options(
                    self.image_prefix.as_deref(),
                    self.image_radix,
                    self.image_domain.as_deref(),
                    self.image_background.as_deref(),
                    self.image_baseline,
                ),
                encoding: self.image_encoding.as_deref().map(|v| match v {
                    "bmp" => Encoding::Bmp,
                    "gif" => Encoding::Gif,
                    "tga" => Encoding::Tga,
                    "pbm" => Encoding::Pbm,
                    "pgm" => Encoding::Pgm,
                    _ => Encoding::Png,
                }),
                columns: self.image_columns.unwrap_or(16),
                mapping_format: if self.image_mapping_format.as_deref() == Some("toml") {
                    MappingFormat::Toml
                } else {
                    MappingFormat::Json
                },
                gray_threshold: threshold,
                limits: raster.input_limits(),
            });
        }
        #[cfg(not(feature = "image"))]
        if id == "image" || explicit {
            return Err(Error::FeatureDisabled {
                format: "image".into(),
            });
        }
        let _ = (raster, threshold, registry);
        Ok(())
    }
}

#[cfg(feature = "image")]
fn set_options(
    prefix: Option<&str>,
    radix: Option<u32>,
    domain: Option<&str>,
    background: Option<&str>,
    baseline: Option<i64>,
) -> fontpixel::formats::image::ImageSetOptions {
    use fontpixel::formats::image::{ImageSetBackground, ImageSetDomain, ImageSetOptions};
    ImageSetOptions {
        prefix: prefix.unwrap_or("").into(),
        radix: radix.unwrap_or(16),
        baseline,
        domain: if domain == Some("unicode") {
            ImageSetDomain::Unicode
        } else {
            ImageSetDomain::Legacy
        },
        background: match background {
            Some("white") => ImageSetBackground::White,
            Some("alpha") => ImageSetBackground::Alpha,
            _ => ImageSetBackground::Black,
        },
    }
}
