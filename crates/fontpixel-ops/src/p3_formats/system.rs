//! Native Macintosh resources and operating-system font options.
use super::{feature, target};
use crate::options::{check_choice, check_range};
use fontpixel::{FontRegistry, ResourceLimits, Result};

const READ_OPTIONS_FBIT_READ_KIND: [&str; 2] = ["fbit", "hfnt"];
const READ_OPTIONS_WSFONT_KNOWN_BIT_ORDER: [&str; 2] = ["left", "right"];
const READ_OPTIONS_WSFONT_KNOWN_BYTE_ORDER: [&str; 2] = ["left", "right"];
const READ_OPTIONS_BEOS_READ_COVERAGE: [&str; 2] = ["native8", "full-nibble"];
/// Read options: native Macintosh resources and operating-system font options.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct ReadOptions {
    /// Include the later GEOS DEL slot when interpreting a native strike.
    #[cfg_attr(feature = "clap", arg(long))]
    pub geos_read_include_del: bool,
    /// Bare Japanese Macintosh resource type (default fbit).
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_FBIT_READ_KIND))]
    pub fbit_read_kind: Option<String>,
    /// Scoped bitmap data fork for an fbit resource.
    #[cfg_attr(feature = "clap", arg(long))]
    pub fbit_read_data_fork: Option<String>,
    /// Scoped AppleDouble resource companion for an fbit data-fork input.
    #[cfg_attr(feature = "clap", arg(long, conflicts_with = "fbit_read_data_fork"))]
    pub fbit_apple_double_resource: Option<String>,
    /// Explicit cell width for bare HFNT input.
    #[cfg_attr(feature = "clap", arg(long, value_parser = clap::value_parser!(u16).range(1..)))]
    pub fbit_read_hfnt_width: Option<u16>,
    /// Charset label for Lisa legacy positions.
    #[cfg_attr(feature = "clap", arg(long))]
    pub lisa_read_charset: Option<String>,
    /// Accept shortened SymbOS character ranges.
    #[cfg_attr(feature = "clap", arg(long))]
    pub symbos_read_allow_short_range: bool,
    /// Meaning of WSF driver-specific bit order zero.
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_WSFONT_KNOWN_BIT_ORDER))]
    pub wsfont_known_bit_order: Option<String>,
    /// Meaning of WSF driver-specific byte order zero.
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_WSFONT_KNOWN_BYTE_ORDER))]
    pub wsfont_known_byte_order: Option<String>,
    /// BeOS coverage interpretation (default native8).
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_BEOS_READ_COVERAGE))]
    pub beos_read_coverage: Option<String>,
    /// Permit explicit rounding of fractional BeOS advances on input.
    #[cfg_attr(feature = "clap", arg(long))]
    pub beos_read_round_fractional_metrics: bool,
}
impl ReadOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.fbit_read_kind.as_deref() {
            check_choice("fbit_read_kind", value, &READ_OPTIONS_FBIT_READ_KIND)?;
        }
        if let Some(value) = self.fbit_read_hfnt_width {
            check_range(
                "fbit_read_hfnt_width",
                i128::from(value),
                1,
                i128::from(u16::MAX),
            )?;
        }
        if let Some(value) = self.wsfont_known_bit_order.as_deref() {
            check_choice(
                "wsfont_known_bit_order",
                value,
                &READ_OPTIONS_WSFONT_KNOWN_BIT_ORDER,
            )?;
        }
        if let Some(value) = self.wsfont_known_byte_order.as_deref() {
            check_choice(
                "wsfont_known_byte_order",
                value,
                &READ_OPTIONS_WSFONT_KNOWN_BYTE_ORDER,
            )?;
        }
        if let Some(value) = self.beos_read_coverage.as_deref() {
            check_choice(
                "beos_read_coverage",
                value,
                &READ_OPTIONS_BEOS_READ_COVERAGE,
            )?;
        }
        Ok(())
    }
}
impl ReadOptions {
    #[cfg(feature = "wsfont")]
    fn wsfont_orders(&self) -> fontpixel::formats::wsfont::WsfontReadOptions {
        fontpixel::formats::wsfont::WsfontReadOptions {
            known_bit_order: wsfont_order(self.wsfont_known_bit_order.as_deref().unwrap_or("left")),
            known_byte_order: wsfont_order(
                self.wsfont_known_byte_order.as_deref().unwrap_or("left"),
            ),
        }
    }
    pub(crate) fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        feature(self.geos_read_include_del, cfg!(feature = "geos"), "geos")?;
        #[cfg(feature = "geos")]
        registry.set_geos_read_options(fontpixel::formats::geos::GeosReadOptions {
            include_del: self.geos_read_include_del,
        });
        feature(
            self.fbit_read_kind.is_some()
                || self.fbit_read_data_fork.is_some()
                || self.fbit_apple_double_resource.is_some()
                || self.fbit_read_hfnt_width.is_some(),
            cfg!(feature = "fbit"),
            "fbit",
        )?;
        feature(
            self.lisa_read_charset.is_some(),
            cfg!(feature = "lisa"),
            "lisa",
        )?;
        feature(
            self.symbos_read_allow_short_range,
            cfg!(feature = "symbos"),
            "symbos",
        )?;
        feature(
            self.wsfont_known_bit_order.is_some() || self.wsfont_known_byte_order.is_some(),
            cfg!(feature = "wsfont"),
            "wsfont",
        )?;
        feature(
            self.beos_read_coverage.is_some() || self.beos_read_round_fractional_metrics,
            cfg!(feature = "beos"),
            "beos",
        )?;
        #[cfg(feature = "fbit")]
        registry.set_fbit_read_options(fontpixel::formats::fbit::FbitReadOptions {
            kind: fbit_kind(self.fbit_read_kind.as_deref()),
            data_fork: self
                .fbit_read_data_fork
                .as_deref()
                .map(fontpixel::ResourcePath::new)
                .transpose()?,
            hfnt_width: self.fbit_read_hfnt_width,
            apple_double_resource: self
                .fbit_apple_double_resource
                .as_deref()
                .map(fontpixel::ResourcePath::new)
                .transpose()?,
        });
        #[cfg(feature = "lisa")]
        registry.set_lisa_read_options(fontpixel::formats::lisa::LisaReadOptions {
            charset: self.lisa_read_charset.clone(),
        });
        #[cfg(feature = "symbos")]
        registry.set_symbos_read_options(fontpixel::formats::symbos::SymbosReadOptions {
            allow_short_range: self.symbos_read_allow_short_range,
        });
        #[cfg(feature = "wsfont")]
        registry.set_wsfont_read_options(self.wsfont_orders());
        #[cfg(feature = "beos")]
        registry.set_beos_read_options(fontpixel::formats::beos::BeosReadOptions {
            coverage: beos_coverage(self.beos_read_coverage.as_deref().unwrap_or("native8")),
            round_fractional_metrics: self.beos_read_round_fractional_metrics,
        });
        let _ = registry;
        Ok(())
    }
}

const WRITE_OPTIONS_GEOS_CONTAINER: [&str; 2] = ["bare", "cvt"];
const WRITE_OPTIONS_GEOS_VARIANT: [&str; 5] = ["standard", "extended", "utf8", "mega", "mega-utf8"];
const WRITE_OPTIONS_FBIT_KIND: [&str; 2] = ["fbit", "hfnt"];
const WRITE_OPTIONS_FBIT_CONTAINER: [&str; 8] = [
    "bare",
    "resource-fork",
    "dfont",
    "suitcase",
    "macbinary",
    "apple-single",
    "apple-double",
    "binhex",
];
const WRITE_OPTIONS_FBIT_DEFINITION: [&str; 3] = ["inline", "data-fork5", "data-fork7"];
const WRITE_OPTIONS_LISA_COMPRESSION: [&str; 3] = ["preserve", "compressed", "uncompressed"];
const WRITE_OPTIONS_SYMBOS_VERSION: [&str; 3] = ["small", "medium", "big"];
const WRITE_OPTIONS_SYMBOS_LAYOUT: [&str; 2] = ["screen", "wysiwyg"];
const WRITE_OPTIONS_WSFONT_BIT_ORDER: [&str; 3] = ["known", "left", "right"];
const WRITE_OPTIONS_WSFONT_BYTE_ORDER: [&str; 3] = ["known", "left", "right"];
const WRITE_OPTIONS_WSFONT_DEPTH: [&str; 2] = ["mono", "alpha"];
const WRITE_OPTIONS_BEOS_COVERAGE: [&str; 2] = ["native8", "full-nibble"];
/// Write options: native Macintosh resources and operating-system font options.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct WriteOptions {
    /// GEOS output resource envelope (default preserves source, otherwise cvt).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_GEOS_CONTAINER))]
    pub geos_container: Option<String>,
    /// GEOS native strike and mapping variant.
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_GEOS_VARIANT))]
    pub geos_variant: Option<String>,
    /// Explicitly include or omit the GEOS DEL slot.
    #[cfg_attr(feature = "clap", arg(long, action=clap::ArgAction::Set))]
    pub geos_include_del: Option<bool>,
    /// GEOS family identifier.
    #[cfg_attr(feature = "clap", arg(long))]
    pub geos_font_id: Option<u16>,
    /// GEOS family name.
    #[cfg_attr(feature = "clap", arg(long))]
    pub geos_family: Option<String>,
    /// Rebuild GEOS records instead of preserving unchanged native resources.
    #[cfg_attr(feature = "clap", arg(long))]
    pub geos_rebuild: bool,
    /// Explicitly use legacy codes as Unicode in GEOS extended output.
    #[cfg_attr(feature = "clap", arg(long))]
    pub geos_legacy_as_unicode: bool,
    /// VFNT single-column width (double-width glyphs use two halves).
    #[cfg_attr(feature = "clap", arg(long, requires="vtfont_cell_height", value_parser=clap::value_parser!(u8).range(1..)))]
    pub vtfont_cell_width: Option<u8>,
    /// VFNT cell height.
    #[cfg_attr(feature = "clap", arg(long, requires="vtfont_cell_width", value_parser=clap::value_parser!(u8).range(1..)))]
    pub vtfont_cell_height: Option<u8>,
    /// Rebuild VFNT native glyph pools and maps even for unchanged input.
    #[cfg_attr(feature = "clap", arg(long))]
    pub vtfont_rebuild: bool,
    /// Explicitly project legacy numeric labels to VFNT Unicode mappings.
    #[cfg_attr(feature = "clap", arg(long))]
    pub vtfont_legacy_as_unicode: bool,
    /// Japanese Macintosh resource kind (default fbit).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_FBIT_KIND))]
    pub fbit_kind: Option<String>,
    /// Japanese Macintosh resource envelope (default bare).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_FBIT_CONTAINER))]
    pub fbit_container: Option<String>,
    /// fbit bitmap placement (default inline).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_FBIT_DEFINITION))]
    pub fbit_definition: Option<String>,
    /// Relative companion name for a split bare fbit output.
    #[cfg_attr(feature = "clap", arg(long))]
    pub fbit_data_fork: Option<String>,
    /// Lisa resource compression (default preserve).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_LISA_COMPRESSION))]
    pub lisa_compression: Option<String>,
    /// SymbOS native layout.
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_SYMBOS_VERSION))]
    pub symbos_version: Option<String>,
    /// Initial native SymbOS character code.
    #[cfg_attr(feature = "clap", arg(long))]
    pub symbos_first_code: Option<u8>,
    /// SymbOS extended writing-system identifier.
    #[cfg_attr(feature = "clap", arg(long, conflicts_with="symbos_layout", value_parser=clap::value_parser!(u8).range(2..=31)))]
    pub symbos_writing_system: Option<u8>,
    /// SymbOS alternative extended layout header.
    #[cfg_attr(feature = "clap", arg(long, conflicts_with="symbos_writing_system", value_parser = WRITE_OPTIONS_SYMBOS_LAYOUT))]
    pub symbos_layout: Option<String>,
    /// SymbOS output cell height.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u8).range(1..=48)))]
    pub symbos_height: Option<u8>,
    /// Project Oberon family byte.
    #[cfg_attr(feature = "clap", arg(long))]
    pub oberon_family: Option<u8>,
    /// Project Oberon variant byte.
    #[cfg_attr(feature = "clap", arg(long))]
    pub oberon_variant: Option<u8>,
    /// WSF native encoding identifier.
    #[cfg_attr(feature = "clap", arg(long))]
    pub wsfont_encoding: Option<u32>,
    /// WSF wire bit order.
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_WSFONT_BIT_ORDER))]
    pub wsfont_bit_order: Option<String>,
    /// WSF wire byte order.
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_WSFONT_BYTE_ORDER))]
    pub wsfont_byte_order: Option<String>,
    /// WSF coverage storage.
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_WSFONT_DEPTH))]
    pub wsfont_depth: Option<String>,
    /// WSF bytes per raster row.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u32).range(1..)))]
    pub wsfont_stride: Option<u32>,
    /// BeOS output coverage palette.
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_BEOS_COVERAGE))]
    pub beos_coverage: Option<String>,
    /// Explicitly quantize BeOS gray samples to the selected palette.
    #[cfg_attr(feature = "clap", arg(long))]
    pub beos_quantize_gray: bool,
    /// Interpret BeOS source legacy labels as Unicode numbers.
    #[cfg_attr(feature = "clap", arg(long))]
    pub beos_legacy_as_unicode: bool,
    /// Permit retained fractional BeOS advances on output.
    #[cfg_attr(feature = "clap", arg(long))]
    pub beos_round_fractional_metrics: bool,
}
impl WriteOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.geos_container.as_deref() {
            check_choice("geos_container", value, &WRITE_OPTIONS_GEOS_CONTAINER)?;
        }
        if let Some(value) = self.geos_variant.as_deref() {
            check_choice("geos_variant", value, &WRITE_OPTIONS_GEOS_VARIANT)?;
        }
        if let Some(value) = self.vtfont_cell_width {
            check_range(
                "vtfont_cell_width",
                i128::from(value),
                1,
                i128::from(u8::MAX),
            )?;
        }
        if let Some(value) = self.vtfont_cell_height {
            check_range(
                "vtfont_cell_height",
                i128::from(value),
                1,
                i128::from(u8::MAX),
            )?;
        }
        if let Some(value) = self.fbit_kind.as_deref() {
            check_choice("fbit_kind", value, &WRITE_OPTIONS_FBIT_KIND)?;
        }
        if let Some(value) = self.fbit_container.as_deref() {
            check_choice("fbit_container", value, &WRITE_OPTIONS_FBIT_CONTAINER)?;
        }
        if let Some(value) = self.fbit_definition.as_deref() {
            check_choice("fbit_definition", value, &WRITE_OPTIONS_FBIT_DEFINITION)?;
        }
        if let Some(value) = self.lisa_compression.as_deref() {
            check_choice("lisa_compression", value, &WRITE_OPTIONS_LISA_COMPRESSION)?;
        }
        if let Some(value) = self.symbos_version.as_deref() {
            check_choice("symbos_version", value, &WRITE_OPTIONS_SYMBOS_VERSION)?;
        }
        if let Some(value) = self.symbos_writing_system {
            check_range("symbos_writing_system", i128::from(value), 2, 31)?;
        }
        if let Some(value) = self.symbos_layout.as_deref() {
            check_choice("symbos_layout", value, &WRITE_OPTIONS_SYMBOS_LAYOUT)?;
        }
        if let Some(value) = self.symbos_height {
            check_range("symbos_height", i128::from(value), 1, 48)?;
        }
        if let Some(value) = self.wsfont_bit_order.as_deref() {
            check_choice("wsfont_bit_order", value, &WRITE_OPTIONS_WSFONT_BIT_ORDER)?;
        }
        if let Some(value) = self.wsfont_byte_order.as_deref() {
            check_choice("wsfont_byte_order", value, &WRITE_OPTIONS_WSFONT_BYTE_ORDER)?;
        }
        if let Some(value) = self.wsfont_depth.as_deref() {
            check_choice("wsfont_depth", value, &WRITE_OPTIONS_WSFONT_DEPTH)?;
        }
        if let Some(value) = self.wsfont_stride {
            check_range("wsfont_stride", i128::from(value), 1, i128::from(u32::MAX))?;
        }
        if let Some(value) = self.beos_coverage.as_deref() {
            check_choice("beos_coverage", value, &WRITE_OPTIONS_BEOS_COVERAGE)?;
        }
        Ok(())
    }
}
impl WriteOptions {
    pub(crate) fn apply(
        &self,
        id: &str,
        limits: &ResourceLimits,
        threshold: Option<u8>,
        unicode_as_legacy: bool,
        input: &ReadOptions,
        registry: &mut FontRegistry,
    ) -> Result<()> {
        target(
            self.geos_container.is_some()
                || self.geos_variant.is_some()
                || self.geos_include_del.is_some()
                || self.geos_font_id.is_some()
                || self.geos_family.is_some()
                || self.geos_rebuild
                || self.geos_legacy_as_unicode,
            id,
            "geos",
        )?;
        #[cfg(feature = "geos")]
        if id == "geos" {
            use fontpixel::formats::geos::{GeosContainer, GeosVariant, GeosWriteOptions};
            registry.set_geos_write_options(GeosWriteOptions {
                container: self.geos_container.as_deref().map(|v| {
                    if v == "bare" {
                        GeosContainer::Bare
                    } else {
                        GeosContainer::Cvt
                    }
                }),
                variant: self.geos_variant.as_deref().map(|v| match v {
                    "extended" => GeosVariant::Extended,
                    "utf8" => GeosVariant::Utf8,
                    "mega" => GeosVariant::Mega,
                    "mega-utf8" => GeosVariant::MegaUtf8,
                    _ => GeosVariant::Standard,
                }),
                include_del: self.geos_include_del,
                font_id: self.geos_font_id,
                family: self.geos_family.clone(),
                preserve_native: !self.geos_rebuild,
                unicode_as_legacy,
                legacy_as_unicode: self.geos_legacy_as_unicode,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        target(
            self.vtfont_cell_width.is_some()
                || self.vtfont_cell_height.is_some()
                || self.vtfont_rebuild
                || self.vtfont_legacy_as_unicode,
            id,
            "vtfont",
        )?;
        #[cfg(feature = "vtfont")]
        if id == "vtfont" {
            registry.set_vtfont_write_options(fontpixel::formats::vtfont::VtfontWriteOptions {
                cell_width: self.vtfont_cell_width,
                cell_height: self.vtfont_cell_height,
                preserve_native: !self.vtfont_rebuild,
                legacy_as_unicode: self.vtfont_legacy_as_unicode,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        target(
            self.fbit_kind.is_some()
                || self.fbit_container.is_some()
                || self.fbit_definition.is_some()
                || self.fbit_data_fork.is_some(),
            id,
            "fbit",
        )?;
        target(self.lisa_compression.is_some(), id, "lisa")?;
        target(
            self.symbos_version.is_some()
                || self.symbos_first_code.is_some()
                || self.symbos_writing_system.is_some()
                || self.symbos_layout.is_some()
                || self.symbos_height.is_some(),
            id,
            "symbos",
        )?;
        target(
            self.oberon_family.is_some() || self.oberon_variant.is_some(),
            id,
            "oberon",
        )?;
        target(
            self.wsfont_encoding.is_some()
                || self.wsfont_bit_order.is_some()
                || self.wsfont_byte_order.is_some()
                || self.wsfont_depth.is_some()
                || self.wsfont_stride.is_some(),
            id,
            "wsfont",
        )?;
        target(
            self.beos_coverage.is_some()
                || self.beos_quantize_gray
                || self.beos_legacy_as_unicode
                || self.beos_round_fractional_metrics,
            id,
            "beos",
        )?;
        #[cfg(feature = "fbit")]
        if id == "fbit" {
            use fontpixel::formats::{
                fbit::{Definition, FbitWriteOptions},
                nfnt::Container,
            };
            registry.set_fbit_write_options(FbitWriteOptions {
                kind: fbit_kind(self.fbit_kind.as_deref()),
                container: match self.fbit_container.as_deref() {
                    Some("resource-fork") => Container::ResourceFork,
                    Some("dfont") => Container::Dfont,
                    Some("suitcase") => Container::Suitcase,
                    Some("macbinary") => Container::MacBinary,
                    Some("apple-single") => Container::AppleSingle,
                    Some("apple-double") => Container::AppleDouble,
                    Some("binhex") => Container::BinHex,
                    _ => Container::Bare,
                },
                definition: match self.fbit_definition.as_deref() {
                    Some("data-fork5") => Definition::DataFork5,
                    Some("data-fork7") => Definition::DataFork7,
                    _ => Definition::Inline,
                },
                data_fork: self
                    .fbit_data_fork
                    .as_deref()
                    .map(fontpixel::ResourcePath::new)
                    .transpose()?,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "lisa")]
        if id == "lisa" {
            use fontpixel::formats::lisa::{Compression, LisaWriteOptions};
            registry.set_lisa_write_options(LisaWriteOptions {
                compression: match self.lisa_compression.as_deref() {
                    Some("compressed") => Compression::Compressed,
                    Some("uncompressed") => Compression::Uncompressed,
                    _ => Compression::Preserve,
                },
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "symbos")]
        if id == "symbos" {
            use fontpixel::formats::symbos::{
                SymbosExtendedInfo, SymbosVersion, SymbosWriteOptions,
            };
            registry.set_symbos_write_options(SymbosWriteOptions {
                version: self.symbos_version.as_deref().map(|v| match v {
                    "small" => SymbosVersion::Small,
                    "big" => SymbosVersion::Big,
                    _ => SymbosVersion::Medium,
                }),
                first_code: self.symbos_first_code,
                extended: self
                    .symbos_writing_system
                    .map(SymbosExtendedInfo::WritingSystem)
                    .or_else(|| {
                        self.symbos_layout
                            .as_deref()
                            .map(|v| SymbosExtendedInfo::Layout {
                                wysiwyg: v == "wysiwyg",
                            })
                    }),
                height: self.symbos_height,
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "oberon")]
        if id == "oberon" {
            registry.set_oberon_write_options(fontpixel::formats::oberon::OberonWriteOptions {
                family: self.oberon_family,
                variant: self.oberon_variant,
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "wsfont")]
        if id == "wsfont" {
            use fontpixel::formats::wsfont::{WsfontDepth, WsfontWriteOptions};
            registry.set_wsfont_write_options(WsfontWriteOptions {
                encoding: self.wsfont_encoding,
                bit_order: self.wsfont_bit_order.as_deref().map(wsfont_order),
                byte_order: self.wsfont_byte_order.as_deref().map(wsfont_order),
                depth: self.wsfont_depth.as_deref().map(|v| {
                    if v == "alpha" {
                        WsfontDepth::Alpha
                    } else {
                        WsfontDepth::Mono
                    }
                }),
                stride: self.wsfont_stride,
                known_orders: input.wsfont_orders(),
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "beos")]
        if id == "beos" {
            registry.set_beos_write_options(fontpixel::formats::beos::BeosWriteOptions {
                coverage: self.beos_coverage.as_deref().map(beos_coverage),
                quantize_gray: self.beos_quantize_gray,
                legacy_as_unicode: self.beos_legacy_as_unicode,
                round_fractional_metrics: self.beos_round_fractional_metrics,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        let _ = (limits, threshold, unicode_as_legacy, input, registry);
        Ok(())
    }
}
#[cfg(feature = "fbit")]
fn fbit_kind(value: Option<&str>) -> fontpixel::formats::fbit::ResourceKind {
    use fontpixel::formats::fbit::ResourceKind;
    if value == Some("hfnt") {
        ResourceKind::Hfnt
    } else {
        ResourceKind::Fbit
    }
}
#[cfg(feature = "wsfont")]
fn wsfont_order(value: &str) -> fontpixel::formats::wsfont::WsfontOrder {
    use fontpixel::formats::wsfont::WsfontOrder;
    match value {
        "known" => WsfontOrder::Known,
        "right" => WsfontOrder::RightToLeft,
        _ => WsfontOrder::LeftToRight,
    }
}
#[cfg(feature = "beos")]
fn beos_coverage(value: &str) -> fontpixel::formats::beos::BeosCoverage {
    use fontpixel::formats::beos::BeosCoverage;
    if value == "full-nibble" {
        BeosCoverage::FullNibble
    } else {
        BeosCoverage::Native8
    }
}
