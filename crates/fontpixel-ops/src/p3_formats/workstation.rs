//! Native workstation/printer font variants and associated interpretation.
use super::{feature, target};
use fontpixel::{Error, FontRegistry, ResourceLimits, Result};
/// Read options: native workstation/printer font variants and associated interpretation.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct ReadOptions {
    /// Require a prebuilt bitmap numeric byte order.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["big", "little"]))]
    pub prebuilt_read_byte_order: Option<String>,
    /// Prebuilt input bitmap pixel order (default msb; independent of numeric byte order).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["msb", "lsb"]))]
    pub prebuilt_read_bitmap_bit_order: Option<String>,
    /// Catalyst native directory origin (default resource-body).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["resource-body", "glyph-data"]))]
    pub catalyst_offset_base: Option<String>,
    /// Require ChiWriter version 3 or 4.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["3", "4"]))]
    pub chiwriter_read_version: Option<String>,
    /// Explicit compatible bitmap directory origin.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["contiguous", "absolute", "relative"]))]
    pub chiwriter_read_layout: Option<String>,
    /// ChiWriter input raster bytes per row.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u8).range(1..)))]
    pub chiwriter_read_stride: Option<u8>,
    /// Require Dashen screen/printer generation.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["3", "4"]))]
    pub dashen_read_version: Option<String>,
    /// Explicitly accept the historical monobit PCL descriptor-size discrepancy.
    #[cfg_attr(feature = "clap", arg(long))]
    pub hppcl_allow_legacy_descriptor_size: bool,
}
impl ReadOptions {
    pub(crate) fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        feature(
            self.prebuilt_read_byte_order.is_some()
                || self.prebuilt_read_bitmap_bit_order.is_some(),
            cfg!(feature = "prebuilt"),
            "prebuilt",
        )?;
        #[cfg(feature = "prebuilt")]
        registry.set_prebuilt_read_options(fontpixel::formats::prebuilt::PrebuiltReadOptions {
            byte_order: self.prebuilt_read_byte_order.as_deref().map(prebuilt_order),
            bitmap_bit_order: prebuilt_bits(self.prebuilt_read_bitmap_bit_order.as_deref()),
        });
        feature(
            self.catalyst_offset_base.is_some(),
            cfg!(feature = "catalyst"),
            "catalyst",
        )?;
        feature(
            self.chiwriter_read_version.is_some()
                || self.chiwriter_read_layout.is_some()
                || self.chiwriter_read_stride.is_some(),
            cfg!(feature = "chiwriter"),
            "chiwriter",
        )?;
        feature(
            self.dashen_read_version.is_some(),
            cfg!(feature = "dashen"),
            "dashen",
        )?;
        feature(
            self.hppcl_allow_legacy_descriptor_size,
            cfg!(feature = "hppcl"),
            "hppcl",
        )?;
        #[cfg(feature = "catalyst")]
        registry.set_catalyst_read_options(fontpixel::formats::catalyst::CatalystReadOptions {
            offset_base: catalyst_base(self.catalyst_offset_base.as_deref()),
        });
        #[cfg(feature = "chiwriter")]
        registry.set_chiwriter_read_options(fontpixel::formats::chiwriter::ChiwriterReadOptions {
            version: self
                .chiwriter_read_version
                .as_deref()
                .map(chiwriter_version),
            layout: chiwriter_layout(self.chiwriter_read_layout.as_deref()),
            stride: self.chiwriter_read_stride,
        });
        #[cfg(feature = "hppcl")]
        registry.set_hppcl_read_options(fontpixel::formats::hppcl::HppclReadOptions {
            allow_legacy_descriptor_size: self.hppcl_allow_legacy_descriptor_size,
        });
        #[cfg(feature = "dashen")]
        registry.set_dashen_read_options(fontpixel::formats::dashen::DashenReadOptions {
            version: self.dashen_read_version.as_deref().map(dashen_version),
            pcl: fontpixel::formats::hppcl::HppclReadOptions {
                allow_legacy_descriptor_size: self.hppcl_allow_legacy_descriptor_size,
            },
        });
        let _ = registry;
        Ok(())
    }
}
/// Write options: native workstation/printer font variants and associated interpretation.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct WriteOptions {
    /// Prebuilt numeric byte order (default big).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["big", "little"]))]
    pub prebuilt_byte_order: Option<String>,
    /// Prebuilt output bitmap pixel order (default msb; lsb for explicit compatibility).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["msb", "lsb"]))]
    pub prebuilt_bitmap_bit_order: Option<String>,
    /// Prebuilt native coverage precision; otherwise inferred per strike.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["mono", "gray2", "gray4", "gray8"]))]
    pub prebuilt_depth: Option<String>,
    /// Explicitly round prebuilt gray samples to the selected palette.
    #[cfg_attr(feature = "clap", arg(long))]
    pub prebuilt_quantize: bool,
    /// Prebuilt character-set name.
    #[cfg_attr(feature = "clap", arg(long))]
    pub prebuilt_charset_name: Option<String>,
    /// Prebuilt font identifier.
    #[cfg_attr(feature = "clap", arg(long))]
    pub prebuilt_identifier: Option<u32>,
    /// Catalyst output directory origin.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["resource-body", "glyph-data"]))]
    pub catalyst_write_offset_base: Option<String>,
    /// Catalyst common raster height.
    #[cfg_attr(feature = "clap", arg(long))]
    pub catalyst_height: Option<u8>,
    /// Catalyst inter-line leading.
    #[cfg_attr(feature = "clap", arg(long))]
    pub catalyst_leading: Option<u8>,
    /// Catalyst common trailing bearing.
    #[cfg_attr(feature = "clap", arg(long))]
    pub catalyst_sidebearing: Option<u8>,
    /// ChiWriter output generation (default 4).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["3", "4"]))]
    pub chiwriter_version: Option<String>,
    /// ChiWriter compatible output directory.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["contiguous", "absolute", "relative"]))]
    pub chiwriter_layout: Option<String>,
    /// ChiWriter output raster bytes per row.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u8).range(1..)))]
    pub chiwriter_stride: Option<u8>,
    /// ChiWriter output cell width.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u8).range(1..)))]
    pub chiwriter_width: Option<u8>,
    /// ChiWriter output cell height.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u8).range(1..)))]
    pub chiwriter_height: Option<u8>,
    /// ChiWriter cell-top to baseline distance.
    #[cfg_attr(feature = "clap", arg(long))]
    pub chiwriter_baseline: Option<u8>,
    /// ChiWriter v3 baseline-to-next-cell-top distance.
    #[cfg_attr(feature = "clap", arg(long))]
    pub chiwriter_line_gap: Option<u8>,
    /// ChiWriter native filename (Latin-1, at most thirteen bytes).
    #[cfg_attr(feature = "clap", arg(long))]
    pub chiwriter_filename: Option<String>,
    /// Dashen output generation (default 3).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["3", "4"]))]
    pub dashen_version: Option<String>,
    /// Dashen native screen top offset.
    #[cfg_attr(feature = "clap", arg(long))]
    pub dashen_top: Option<u8>,
    /// Emit an explicit Dashen codepoint remapping table.
    #[cfg_attr(feature = "clap", arg(long))]
    pub dashen_codepoint_map: bool,
    /// PCL physical raster orientation.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["portrait", "landscape", "reverse-portrait", "reverse-landscape"]))]
    pub hppcl_orientation: Option<String>,
    /// PCL character encoding (default raw).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["raw", "rle"]))]
    pub hppcl_compression: Option<String>,
    /// Maximum PCL character-command bytes, including descriptor (17..32767).
    #[cfg_attr(feature = "clap", arg(long))]
    pub hppcl_continuation_bytes: Option<usize>,
    /// PCL format 20 horizontal resolution.
    #[cfg_attr(feature = "clap", arg(long, requires="hppcl_y_resolution", value_parser=clap::value_parser!(u16).range(1..)))]
    pub hppcl_x_resolution: Option<u16>,
    /// PCL format 20 vertical resolution.
    #[cfg_attr(feature = "clap", arg(long, requires="hppcl_x_resolution", value_parser=clap::value_parser!(u16).range(1..)))]
    pub hppcl_y_resolution: Option<u16>,
    /// PCL native symbol-set identifier.
    #[cfg_attr(feature = "clap", arg(long))]
    pub hppcl_symbol_set: Option<u16>,
}
impl WriteOptions {
    pub(crate) fn apply(
        &self,
        id: &str,
        limits: &ResourceLimits,
        threshold: Option<u8>,
        unicode_as_legacy: bool,
        registry: &mut FontRegistry,
    ) -> Result<()> {
        target(
            self.prebuilt_byte_order.is_some()
                || self.prebuilt_bitmap_bit_order.is_some()
                || self.prebuilt_depth.is_some()
                || self.prebuilt_quantize
                || self.prebuilt_charset_name.is_some()
                || self.prebuilt_identifier.is_some(),
            id,
            "prebuilt",
        )?;
        #[cfg(feature = "prebuilt")]
        if id == "prebuilt" {
            use fontpixel::formats::prebuilt::{PrebuiltDepth, PrebuiltWriteOptions};
            registry.set_prebuilt_write_options(PrebuiltWriteOptions {
                byte_order: prebuilt_order(self.prebuilt_byte_order.as_deref().unwrap_or("big")),
                bitmap_bit_order: prebuilt_bits(self.prebuilt_bitmap_bit_order.as_deref()),
                depth: self.prebuilt_depth.as_deref().map(|v| match v {
                    "gray2" => PrebuiltDepth::Gray2,
                    "gray4" => PrebuiltDepth::Gray4,
                    "gray8" => PrebuiltDepth::Gray8,
                    _ => PrebuiltDepth::Mono,
                }),
                quantize: self.prebuilt_quantize,
                charset_name: self.prebuilt_charset_name.clone(),
                identifier: self.prebuilt_identifier,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        target(
            self.catalyst_write_offset_base.is_some()
                || self.catalyst_height.is_some()
                || self.catalyst_leading.is_some()
                || self.catalyst_sidebearing.is_some(),
            id,
            "catalyst",
        )?;
        target(
            self.chiwriter_version.is_some()
                || self.chiwriter_layout.is_some()
                || self.chiwriter_stride.is_some()
                || self.chiwriter_width.is_some()
                || self.chiwriter_height.is_some()
                || self.chiwriter_baseline.is_some()
                || self.chiwriter_line_gap.is_some()
                || self.chiwriter_filename.is_some(),
            id,
            "chiwriter",
        )?;
        target(
            self.dashen_version.is_some() || self.dashen_top.is_some() || self.dashen_codepoint_map,
            id,
            "dashen",
        )?;
        if (self.hppcl_orientation.is_some()
            || self.hppcl_compression.is_some()
            || self.hppcl_continuation_bytes.is_some()
            || self.hppcl_x_resolution.is_some()
            || self.hppcl_y_resolution.is_some()
            || self.hppcl_symbol_set.is_some())
            && !["hppcl", "dashen"].contains(&id)
        {
            return Err(Error::InvalidInput(
                "PCL options require hppcl or a Dashen printer strike".into(),
            ));
        }
        #[cfg(feature = "catalyst")]
        if id == "catalyst" {
            registry.set_catalyst_write_options(
                fontpixel::formats::catalyst::CatalystWriteOptions {
                    offset_base: catalyst_base(self.catalyst_write_offset_base.as_deref()),
                    height: self.catalyst_height,
                    leading: self.catalyst_leading,
                    sidebearing: self.catalyst_sidebearing,
                    unicode_as_legacy,
                    gray_threshold: threshold,
                    limits: limits.clone(),
                    ..Default::default()
                },
            );
        }
        #[cfg(feature = "chiwriter")]
        if id == "chiwriter" {
            registry.set_chiwriter_write_options(
                fontpixel::formats::chiwriter::ChiwriterWriteOptions {
                    version: chiwriter_version(self.chiwriter_version.as_deref().unwrap_or("4")),
                    layout: chiwriter_layout(self.chiwriter_layout.as_deref()),
                    stride: self.chiwriter_stride,
                    width: self.chiwriter_width,
                    height: self.chiwriter_height,
                    baseline: self.chiwriter_baseline,
                    line_gap: self.chiwriter_line_gap,
                    filename: self.chiwriter_filename.clone(),
                    unicode_as_legacy,
                    gray_threshold: threshold,
                    limits: limits.clone(),
                    ..Default::default()
                },
            );
        }
        #[cfg(feature = "hppcl")]
        if id == "hppcl" {
            registry.set_hppcl_write_options(self.pcl(limits, threshold, unicode_as_legacy));
        }
        #[cfg(feature = "dashen")]
        if id == "dashen" {
            registry.set_dashen_write_options(fontpixel::formats::dashen::DashenWriteOptions {
                version: dashen_version(self.dashen_version.as_deref().unwrap_or("3")),
                top: self.dashen_top,
                codepoint_map: self.dashen_codepoint_map,
                pcl: self.pcl(limits, threshold, unicode_as_legacy),
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        let _ = (limits, threshold, unicode_as_legacy, registry);
        Ok(())
    }
    #[cfg(feature = "hppcl")]
    fn pcl(
        &self,
        limits: &ResourceLimits,
        threshold: Option<u8>,
        unicode_as_legacy: bool,
    ) -> fontpixel::formats::hppcl::HppclWriteOptions {
        use fontpixel::formats::hppcl::{HppclCompression, HppclOrientation, HppclWriteOptions};
        HppclWriteOptions {
            orientation: match self.hppcl_orientation.as_deref() {
                Some("landscape") => HppclOrientation::Landscape,
                Some("reverse-portrait") => HppclOrientation::ReversePortrait,
                Some("reverse-landscape") => HppclOrientation::ReverseLandscape,
                _ => HppclOrientation::Portrait,
            },
            compression: if self.hppcl_compression.as_deref() == Some("rle") {
                HppclCompression::Rle
            } else {
                HppclCompression::Raw
            },
            continuation_bytes: self.hppcl_continuation_bytes.unwrap_or(32767),
            resolution: self.hppcl_x_resolution.zip(self.hppcl_y_resolution),
            symbol_set: self.hppcl_symbol_set,
            unicode_as_legacy,
            gray_threshold: threshold,
            limits: limits.clone(),
            ..Default::default()
        }
    }
}
#[cfg(feature = "catalyst")]
fn catalyst_base(v: Option<&str>) -> fontpixel::formats::catalyst::CatalystOffsetBase {
    use fontpixel::formats::catalyst::CatalystOffsetBase;
    if v == Some("glyph-data") {
        CatalystOffsetBase::GlyphData
    } else {
        CatalystOffsetBase::ResourceBody
    }
}
#[cfg(feature = "chiwriter")]
fn chiwriter_version(v: &str) -> fontpixel::formats::chiwriter::ChiwriterVersion {
    use fontpixel::formats::chiwriter::ChiwriterVersion;
    if v == "3" {
        ChiwriterVersion::V3
    } else {
        ChiwriterVersion::V4
    }
}
#[cfg(feature = "chiwriter")]
fn chiwriter_layout(v: Option<&str>) -> fontpixel::formats::chiwriter::ChiwriterLayout {
    use fontpixel::formats::chiwriter::ChiwriterLayout;
    match v {
        Some("absolute") => ChiwriterLayout::AbsoluteOffsets,
        Some("relative") => ChiwriterLayout::RelativeOffsets,
        _ => ChiwriterLayout::Contiguous,
    }
}
#[cfg(feature = "dashen")]
fn dashen_version(v: &str) -> fontpixel::formats::dashen::DashenVersion {
    use fontpixel::formats::dashen::DashenVersion;
    if v == "4" {
        DashenVersion::V4
    } else {
        DashenVersion::V3
    }
}
#[cfg(feature = "prebuilt")]
fn prebuilt_order(v: &str) -> fontpixel::formats::prebuilt::PrebuiltByteOrder {
    use fontpixel::formats::prebuilt::PrebuiltByteOrder;
    if v == "little" {
        PrebuiltByteOrder::Little
    } else {
        PrebuiltByteOrder::Big
    }
}

#[cfg(feature = "prebuilt")]
fn prebuilt_bits(v: Option<&str>) -> fontpixel::formats::prebuilt::PrebuiltBitOrder {
    use fontpixel::formats::prebuilt::PrebuiltBitOrder;
    if v == Some("lsb") {
        PrebuiltBitOrder::LsbFirst
    } else {
        PrebuiltBitOrder::MsbFirst
    }
}
