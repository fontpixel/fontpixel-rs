//! GPI, RISC OS, PFR and marker-image choices.
use super::{feature, target};
use fontpixel::{FontRegistry, ResourceLimits, Result};

/// Read options: gPI, RISC OS, PFR and marker-image choices.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct ReadOptions {
    /// Require the DAVIC specification or FreeType PFR wire dialect.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["davic", "freetype"]))]
    pub pfr_read_dialect: Option<String>,
    /// Require a Xerox native font family.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["alto", "bitblt", "prepress"]))]
    pub xerox_read_variant: Option<String>,
    /// Explicit baseline for Alto files with a zero native header baseline.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u8).range(0..=127)))]
    pub xerox_read_alto_baseline: Option<u8>,
    /// Require the SBCS or DBCS PC/GEOS layout.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["sbcs", "dbcs"]))]
    pub pcgeos_read_variant: Option<String>,
    /// Explicitly round fractional PC/GEOS input metrics.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pcgeos_read_round_fractional_metrics: bool,
    /// Retain the additional GPI unmapped null descriptor (default true).
    #[cfg_attr(feature = "clap", arg(long, action=clap::ArgAction::Set))]
    pub gpi_include_unmapped_null: Option<bool>,
    /// Require a RISC OS bitmap generation.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["xy", "4", "5", "6", "7", "8"]))]
    pub riscos_read_version: Option<String>,
    /// Relative RISC OS IntMetrics input companion.
    #[cfg_attr(feature = "clap", arg(long))]
    pub riscos_read_metrics: Option<String>,
    /// Explicit marker-image dialect; otherwise inspect native controls.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["sfont", "rfont"]))]
    pub sfont_read_flavor: Option<String>,
    /// Ordered SFont labels, comma-separated U+0041 or legacy:0x41.
    #[cfg_attr(feature = "clap", arg(long, value_delimiter = ','))]
    pub sfont_read_codepoints: Vec<String>,
    /// SFont baseline down from glyph top.
    #[cfg_attr(feature = "clap", arg(long, allow_hyphen_values = true))]
    pub sfont_read_baseline: Option<i64>,
}
impl ReadOptions {
    pub(crate) fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        feature(
            self.pfr_read_dialect.is_some(),
            cfg!(feature = "pfr"),
            "pfr",
        )?;
        #[cfg(feature = "pfr")]
        registry.set_pfr_read_options(fontpixel::formats::pfr::PfrReadOptions {
            dialect: self.pfr_read_dialect.as_deref().map(pfr_dialect),
        });
        feature(
            self.xerox_read_variant.is_some() || self.xerox_read_alto_baseline.is_some(),
            cfg!(feature = "xerox"),
            "xerox",
        )?;
        #[cfg(feature = "xerox")]
        registry.set_xerox_read_options(fontpixel::formats::xerox::XeroxReadOptions {
            variant: self.xerox_read_variant.as_deref().map(xerox_variant),
            alto_baseline: self.xerox_read_alto_baseline,
        });
        feature(
            self.pcgeos_read_variant.is_some() || self.pcgeos_read_round_fractional_metrics,
            cfg!(feature = "pcgeos"),
            "pcgeos",
        )?;
        #[cfg(feature = "pcgeos")]
        registry.set_pcgeos_read_options(fontpixel::formats::pcgeos::PcgeosReadOptions {
            variant: self.pcgeos_read_variant.as_deref().map(pcgeos_variant),
            round_fractional_metrics: self.pcgeos_read_round_fractional_metrics,
        });
        feature(
            self.gpi_include_unmapped_null.is_some(),
            cfg!(feature = "gpi"),
            "gpi",
        )?;
        feature(
            self.riscos_read_version.is_some() || self.riscos_read_metrics.is_some(),
            cfg!(feature = "riscos"),
            "riscos",
        )?;
        feature(
            self.sfont_read_flavor.is_some()
                || !self.sfont_read_codepoints.is_empty()
                || self.sfont_read_baseline.is_some(),
            cfg!(feature = "sfont"),
            "sfont",
        )?;
        #[cfg(feature = "gpi")]
        registry.set_gpi_read_options(fontpixel::formats::gpi::GpiReadOptions {
            include_unmapped_null: self.gpi_include_unmapped_null.unwrap_or(true),
        });
        #[cfg(feature = "riscos")]
        registry.set_riscos_read_options(fontpixel::formats::riscos::RiscosReadOptions {
            version: self.riscos_read_version.as_deref().map(riscos_version),
            metrics_path: self
                .riscos_read_metrics
                .as_deref()
                .map(fontpixel::ResourcePath::new)
                .transpose()?,
        });
        #[cfg(feature = "sfont")]
        registry.set_sfont_read_options(fontpixel::formats::sfont::SfontReadOptions {
            flavor: self.sfont_read_flavor.as_deref().map(sfont_flavor),
            codepoints: sfont_codes(&self.sfont_read_codepoints)?,
            baseline: self.sfont_read_baseline,
        });
        let _ = registry;
        Ok(())
    }
}

/// Write options: gPI, RISC OS, PFR and marker-image choices.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct WriteOptions {
    /// PFR wire dialect; otherwise preserve native source or use FreeType.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["davic", "freetype"]))]
    pub pfr_dialect: Option<String>,
    /// Scalable Screen Font native encoding (default binary).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["binary", "ascii"]))]
    pub ssfn_encoding: Option<String>,
    /// Explicitly interpret legacy numeric labels as SSFN Unicode mappings.
    #[cfg_attr(feature = "clap", arg(long))]
    pub ssfn_legacy_as_unicode: bool,
    /// Xerox native font family (default bitblt).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["alto", "bitblt", "prepress"]))]
    pub xerox_variant: Option<String>,
    /// Xerox BITBLT strike directory layout (default kerned).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["plain", "kerned", "plain-index", "kerned-index"]))]
    pub xerox_bitblt_layout: Option<String>,
    /// Explicit Xerox Alto baseline.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u8).range(0..=127)))]
    pub xerox_alto_baseline: Option<u8>,
    /// Native Xerox PrePress raster rotation metadata in arc-minutes.
    #[cfg_attr(feature = "clap", arg(long))]
    pub xerox_rotation_minutes: Option<u16>,
    /// Native PC/GEOS layout, otherwise preserve source or use SBCS.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["sbcs", "dbcs"]))]
    pub pcgeos_variant: Option<String>,
    /// PC/GEOS registered font identifier.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pcgeos_font_id: Option<u16>,
    /// PC/GEOS shared family name.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pcgeos_family: Option<String>,
    /// Rebuild PC/GEOS records instead of preserving unchanged native bytes.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pcgeos_rebuild: bool,
    /// Explicitly round fractional PC/GEOS output metrics.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pcgeos_round_fractional_metrics: bool,
    /// Explicitly use legacy code numbers as Unicode in PC/GEOS DBCS output.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pcgeos_legacy_as_unicode: bool,
    /// GPI resource packaging; otherwise preserve source choice.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["bare", "ne", "lx"]))]
    pub gpi_container: Option<String>,
    /// GPI character increment representation.
    #[cfg_attr(feature = "clap", arg(long, value_parser=["fixed", "proportional", "abc"]))]
    pub gpi_kind: Option<String>,
    /// Native OS/2 codepage identifier.
    #[cfg_attr(feature = "clap", arg(long))]
    pub gpi_codepage: Option<u16>,
    /// Native registered OS/2 font identifier.
    #[cfg_attr(feature = "clap", arg(long))]
    pub gpi_registry_id: Option<u16>,
    /// Rebuild GPI records instead of preserving unchanged native bytes.
    #[cfg_attr(feature = "clap", arg(long))]
    pub gpi_rebuild: bool,
    /// RISC OS bitmap generation (default 6).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["xy", "4", "5", "6", "7", "8"]))]
    pub riscos_version: Option<String>,
    /// RISC OS native coverage depth (default mono).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["mono", "gray4"]))]
    pub riscos_depth: Option<String>,
    /// RISC OS one-bit compression (default raw).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["raw", "packed"]))]
    pub riscos_compression: Option<String>,
    /// Relative name for an output IntMetrics companion.
    #[cfg_attr(feature = "clap", arg(long))]
    pub riscos_metrics: Option<String>,
    /// IntMetrics generation (default 2).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["0", "2"], requires="riscos_metrics"))]
    pub riscos_metrics_version: Option<String>,
    /// Explicitly round coverage to the RISC OS Gray4 palette.
    #[cfg_attr(feature = "clap", arg(long))]
    pub riscos_quantize: bool,
    /// PFR bitmap encoding (default raw).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["raw", "rle4", "rle8"]))]
    pub pfr_compression: Option<String>,
    /// Store PFR rows from top to bottom.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pfr_top_down: bool,
    /// PFR raw ink bit value (default 1).
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u8).range(0..=1)))]
    pub pfr_black_pixel: Option<u8>,
    /// Native marker-image dialect (default sfont).
    #[cfg_attr(feature = "clap", arg(long, value_parser=["sfont", "rfont"]))]
    pub sfont_flavor: Option<String>,
    /// Ordered SFont output labels, comma-separated U+0041 or legacy:0x41.
    #[cfg_attr(feature = "clap", arg(long, value_delimiter = ','))]
    pub sfont_codepoints: Vec<String>,
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
            self.ssfn_encoding.is_some() || self.ssfn_legacy_as_unicode,
            id,
            "ssfn",
        )?;
        target(
            self.xerox_variant.is_some()
                || self.xerox_bitblt_layout.is_some()
                || self.xerox_alto_baseline.is_some()
                || self.xerox_rotation_minutes.is_some(),
            id,
            "xerox",
        )?;
        #[cfg(feature = "ssfn")]
        if id == "ssfn" {
            use fontpixel::formats::ssfn::{SsfnEncoding, SsfnWriteOptions};
            registry.set_ssfn_write_options(SsfnWriteOptions {
                encoding: if self.ssfn_encoding.as_deref() == Some("ascii") {
                    SsfnEncoding::Ascii
                } else {
                    SsfnEncoding::Binary
                },
                legacy_as_unicode: self.ssfn_legacy_as_unicode,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "xerox")]
        if id == "xerox" {
            use fontpixel::formats::xerox::{XeroxBitbltLayout, XeroxWriteOptions};
            registry.set_xerox_write_options(XeroxWriteOptions {
                variant: xerox_variant(self.xerox_variant.as_deref().unwrap_or("bitblt")),
                bitblt_layout: match self.xerox_bitblt_layout.as_deref() {
                    Some("plain") => XeroxBitbltLayout::Plain,
                    Some("plain-index") => XeroxBitbltLayout::PlainIndex,
                    Some("kerned-index") => XeroxBitbltLayout::KernedIndex,
                    _ => XeroxBitbltLayout::Kerned,
                },
                alto_baseline: self.xerox_alto_baseline,
                rotation_minutes: self.xerox_rotation_minutes,
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        target(
            self.pcgeos_variant.is_some()
                || self.pcgeos_font_id.is_some()
                || self.pcgeos_family.is_some()
                || self.pcgeos_rebuild
                || self.pcgeos_round_fractional_metrics
                || self.pcgeos_legacy_as_unicode,
            id,
            "pcgeos",
        )?;
        #[cfg(feature = "pcgeos")]
        if id == "pcgeos" {
            registry.set_pcgeos_write_options(fontpixel::formats::pcgeos::PcgeosWriteOptions {
                variant: self.pcgeos_variant.as_deref().map(pcgeos_variant),
                font_id: self.pcgeos_font_id,
                family: self.pcgeos_family.clone(),
                preserve_native: !self.pcgeos_rebuild,
                round_fractional_metrics: self.pcgeos_round_fractional_metrics,
                legacy_as_unicode: self.pcgeos_legacy_as_unicode,
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        target(
            self.gpi_container.is_some()
                || self.gpi_kind.is_some()
                || self.gpi_codepage.is_some()
                || self.gpi_registry_id.is_some()
                || self.gpi_rebuild,
            id,
            "gpi",
        )?;
        target(
            self.riscos_version.is_some()
                || self.riscos_depth.is_some()
                || self.riscos_compression.is_some()
                || self.riscos_metrics.is_some()
                || self.riscos_metrics_version.is_some()
                || self.riscos_quantize,
            id,
            "riscos",
        )?;
        target(
            self.pfr_dialect.is_some()
                || self.pfr_compression.is_some()
                || self.pfr_top_down
                || self.pfr_black_pixel.is_some(),
            id,
            "pfr",
        )?;
        target(
            self.sfont_flavor.is_some() || !self.sfont_codepoints.is_empty(),
            id,
            "sfont",
        )?;
        #[cfg(feature = "gpi")]
        if id == "gpi" {
            use fontpixel::formats::gpi::{GpiContainer, GpiKind, GpiWriteOptions};
            registry.set_gpi_write_options(GpiWriteOptions {
                container: self.gpi_container.as_deref().map(|v| match v {
                    "ne" => GpiContainer::Ne,
                    "lx" => GpiContainer::Lx,
                    _ => GpiContainer::Bare,
                }),
                kind: self.gpi_kind.as_deref().map(|v| match v {
                    "fixed" => GpiKind::Fixed,
                    "proportional" => GpiKind::Proportional,
                    _ => GpiKind::Abc,
                }),
                codepage: self.gpi_codepage,
                registry_id: self.gpi_registry_id,
                preserve_native: !self.gpi_rebuild,
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "riscos")]
        if id == "riscos" {
            use fontpixel::formats::riscos::{
                RiscosCompression, RiscosDepth, RiscosMetricsVersion, RiscosWriteOptions,
            };
            registry.set_riscos_write_options(RiscosWriteOptions {
                version: riscos_version(self.riscos_version.as_deref().unwrap_or("6")),
                depth: if self.riscos_depth.as_deref() == Some("gray4") {
                    RiscosDepth::Gray4
                } else {
                    RiscosDepth::Mono
                },
                compression: if self.riscos_compression.as_deref() == Some("packed") {
                    RiscosCompression::Packed
                } else {
                    RiscosCompression::Raw
                },
                metrics_path: self
                    .riscos_metrics
                    .as_deref()
                    .map(fontpixel::ResourcePath::new)
                    .transpose()?,
                metrics_version: if self.riscos_metrics_version.as_deref() == Some("0") {
                    RiscosMetricsVersion::V0
                } else {
                    RiscosMetricsVersion::V2
                },
                quantize: self.riscos_quantize,
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "pfr")]
        if id == "pfr" {
            use fontpixel::formats::pfr::{PfrCompression, PfrWriteOptions};
            registry.set_pfr_write_options(PfrWriteOptions {
                dialect: self.pfr_dialect.as_deref().map(pfr_dialect),
                compression: match self.pfr_compression.as_deref() {
                    Some("rle4") => PfrCompression::Rle4,
                    Some("rle8") => PfrCompression::Rle8,
                    _ => PfrCompression::Raw,
                },
                top_down: self.pfr_top_down,
                black_pixel: self.pfr_black_pixel.unwrap_or(1) != 0,
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "sfont")]
        if id == "sfont" {
            registry.set_sfont_write_options(fontpixel::formats::sfont::SfontWriteOptions {
                flavor: sfont_flavor(self.sfont_flavor.as_deref().unwrap_or("sfont")),
                codepoints: sfont_codes(&self.sfont_codepoints)?,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        let _ = (limits, threshold, unicode_as_legacy, registry);
        Ok(())
    }
}
#[cfg(feature = "riscos")]
fn riscos_version(v: &str) -> fontpixel::formats::riscos::RiscosVersion {
    use fontpixel::formats::riscos::RiscosVersion;
    match v {
        "xy" => RiscosVersion::Xy,
        "4" => RiscosVersion::V4,
        "5" => RiscosVersion::V5,
        "7" => RiscosVersion::V7,
        "8" => RiscosVersion::V8,
        _ => RiscosVersion::V6,
    }
}
#[cfg(feature = "pfr")]
fn pfr_dialect(v: &str) -> fontpixel::formats::pfr::PfrDialect {
    use fontpixel::formats::pfr::PfrDialect;
    if v == "davic" {
        PfrDialect::Davic
    } else {
        PfrDialect::FreeType
    }
}
#[cfg(feature = "xerox")]
fn xerox_variant(v: &str) -> fontpixel::formats::xerox::XeroxVariant {
    use fontpixel::formats::xerox::XeroxVariant;
    match v {
        "alto" => XeroxVariant::Alto,
        "prepress" => XeroxVariant::Prepress,
        _ => XeroxVariant::Bitblt,
    }
}
#[cfg(feature = "pcgeos")]
fn pcgeos_variant(v: &str) -> fontpixel::formats::pcgeos::PcgeosVariant {
    use fontpixel::formats::pcgeos::PcgeosVariant;
    if v == "dbcs" {
        PcgeosVariant::Dbcs
    } else {
        PcgeosVariant::Sbcs
    }
}
#[cfg(feature = "sfont")]
fn sfont_flavor(v: &str) -> fontpixel::formats::sfont::SfontFlavor {
    use fontpixel::formats::sfont::SfontFlavor;
    if v == "rfont" {
        SfontFlavor::Rfont
    } else {
        SfontFlavor::Sfont
    }
}
#[cfg(feature = "sfont")]
fn sfont_codes(values: &[String]) -> Result<Option<Vec<fontpixel::model::Codepoint>>> {
    use fontpixel::{Error, model::Codepoint};
    if values.is_empty() {
        return Ok(None);
    }
    values
        .iter()
        .map(|v| {
            if let Some(n) = v.strip_prefix("legacy:") {
                return crate::options::parse_code(n).map(Codepoint::Legacy);
            }
            let n = crate::options::parse_code(v)?;
            if char::from_u32(n).is_none() {
                return Err(Error::InvalidInput(
                    "SFont label is not a Unicode scalar".into(),
                ));
            }
            Ok(Codepoint::Unicode(n))
        })
        .collect::<Result<Vec<_>>>()
        .map(Some)
}
