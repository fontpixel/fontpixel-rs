//! Native console fonts, executable loaders and glyph directories.
use super::{feature, target};
use crate::options::check_choice;
use fontpixel::{FontRegistry, ResourceLimits, Result};
const READ_OPTIONS_COMFONT_READ_VARIANT: [&str; 7] = [
    "fontedit",
    "frapt",
    "frapt-tsr",
    "letafont",
    "mania",
    "psfcom",
    "udg",
];
const READ_OPTIONS_CONSOLEET_READ_CODES: [&str; 2] = ["legacy", "unicode"];
const READ_OPTIONS_ZAPFONT_READ_VARIANT: [&str; 2] = ["old", "redraw"];
/// Read options: native console fonts, executable loaders and glyph directories.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct ReadOptions {
    /// Baseline below cell top for BBC, COM, Consoleet, GRASP, HRCG, Wyse, XBIN and Zap.
    #[cfg_attr(feature = "clap", arg(long, allow_hyphen_values = true))]
    pub native_read_baseline: Option<i64>,
    /// Charset label for native console positions; numeric codes stay unchanged.
    #[cfg_attr(feature = "clap", arg(long))]
    pub native_read_charset: Option<String>,
    /// Require one executable font loader variant.
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_COMFONT_READ_VARIANT))]
    pub comfont_read_variant: Option<String>,
    /// Meaning of Consoleet numeric filenames (default legacy).
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_CONSOLEET_READ_CODES))]
    pub consoleet_read_codes: Option<String>,
    /// Require one Zap font layout.
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_ZAPFONT_READ_VARIANT))]
    pub zapfont_read_variant: Option<String>,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(ReadOptions);
impl ReadOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.comfont_read_variant.as_deref() {
            check_choice(
                "comfont_read_variant",
                value,
                &READ_OPTIONS_COMFONT_READ_VARIANT,
            )?;
        }
        if let Some(value) = self.consoleet_read_codes.as_deref() {
            check_choice(
                "consoleet_read_codes",
                value,
                &READ_OPTIONS_CONSOLEET_READ_CODES,
            )?;
        }
        if let Some(value) = self.zapfont_read_variant.as_deref() {
            check_choice(
                "zapfont_read_variant",
                value,
                &READ_OPTIONS_ZAPFONT_READ_VARIANT,
            )?;
        }
        Ok(())
    }
}
impl ReadOptions {
    pub(crate) fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        feature(
            self.native_read_baseline.is_some() || self.native_read_charset.is_some(),
            cfg!(any(
                feature = "bbc",
                feature = "wyse",
                feature = "grasp",
                feature = "hrcg",
                feature = "xbin",
                feature = "comfont",
                feature = "consoleet",
                feature = "zapfont"
            )),
            "native console formats",
        )?;
        feature(
            self.comfont_read_variant.is_some(),
            cfg!(feature = "comfont"),
            "comfont",
        )?;
        feature(
            self.consoleet_read_codes.is_some(),
            cfg!(feature = "consoleet"),
            "consoleet",
        )?;
        feature(
            self.zapfont_read_variant.is_some(),
            cfg!(feature = "zapfont"),
            "zapfont",
        )?;
        #[cfg(feature = "bbc")]
        registry.set_bbc_read_options(fontpixel::formats::bbc::BbcReadOptions {
            baseline: self.native_read_baseline,
            charset: self.native_read_charset.clone(),
        });
        #[cfg(feature = "wyse")]
        registry.set_wyse_read_options(fontpixel::formats::wyse::WyseReadOptions {
            baseline: self.native_read_baseline,
            charset: self.native_read_charset.clone(),
        });
        #[cfg(feature = "grasp")]
        registry.set_grasp_read_options(fontpixel::formats::grasp::GraspReadOptions {
            baseline: self.native_read_baseline,
            charset: self.native_read_charset.clone(),
        });
        #[cfg(feature = "hrcg")]
        registry.set_hrcg_read_options(fontpixel::formats::hrcg::HrcgReadOptions {
            baseline: self.native_read_baseline,
            charset: self.native_read_charset.clone(),
        });
        #[cfg(feature = "xbin")]
        registry.set_xbin_read_options(fontpixel::formats::xbin::XbinReadOptions {
            baseline: self.native_read_baseline,
            charset: self.native_read_charset.clone(),
        });
        #[cfg(feature = "comfont")]
        registry.set_comfont_read_options(fontpixel::formats::comfont::ComfontReadOptions {
            baseline: self.native_read_baseline,
            charset: self.native_read_charset.clone(),
            variant: self.comfont_read_variant.as_deref().map(comfont_variant),
        });
        #[cfg(feature = "consoleet")]
        registry.set_consoleet_read_options(fontpixel::formats::consoleet::ConsoleetReadOptions {
            baseline: self.native_read_baseline,
            charset: self.native_read_charset.clone(),
            codepoint_mode: consoleet_codes(self.consoleet_read_codes.as_deref()),
        });
        #[cfg(feature = "zapfont")]
        registry.set_zapfont_read_options(fontpixel::formats::zapfont::ZapfontReadOptions {
            baseline: self.native_read_baseline,
            charset: self.native_read_charset.clone(),
            variant: self.zapfont_read_variant.as_deref().map(zap_variant),
        });
        let _ = registry;
        Ok(())
    }
}
const WRITE_OPTIONS_COMFONT_VARIANT: [&str; 7] = [
    "fontedit",
    "frapt",
    "frapt-tsr",
    "letafont",
    "mania",
    "psfcom",
    "udg",
];
const WRITE_OPTIONS_COMFONT_CPM_TARGET: [&str; 3] = ["pcw", "cpc", "spectrum"];
const WRITE_OPTIONS_CONSOLEET_CODES: [&str; 2] = ["legacy", "unicode"];
const WRITE_OPTIONS_XBIN_COUNT: [&str; 3] = ["auto", "256", "512"];
const WRITE_OPTIONS_XBIN_SCREEN: [&str; 2] = ["preserve", "font-only"];
const WRITE_OPTIONS_ZAPFONT_VARIANT: [&str; 2] = ["old", "redraw"];
/// Write options: native console fonts, executable loaders and glyph directories.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct WriteOptions {
    /// Executable font loader variant (default frapt).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_COMFONT_VARIANT))]
    pub comfont_variant: Option<String>,
    /// CP/M target for PSFCOM and UDG native loaders (default pcw).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_COMFONT_CPM_TARGET))]
    pub comfont_cpm_target: Option<String>,
    /// Meaning of Consoleet output filenames (default legacy).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_CONSOLEET_CODES))]
    pub consoleet_codes: Option<String>,
    /// XBIN glyph-slot count (default auto).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_XBIN_COUNT))]
    pub xbin_count: Option<String>,
    /// Retain validated XBIN screen metadata or write font alone.
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_XBIN_SCREEN))]
    pub xbin_screen: Option<String>,
    /// Zap output layout (default redraw).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_ZAPFONT_VARIANT))]
    pub zapfont_variant: Option<String>,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(WriteOptions);
impl WriteOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.comfont_variant.as_deref() {
            check_choice("comfont_variant", value, &WRITE_OPTIONS_COMFONT_VARIANT)?;
        }
        if let Some(value) = self.comfont_cpm_target.as_deref() {
            check_choice(
                "comfont_cpm_target",
                value,
                &WRITE_OPTIONS_COMFONT_CPM_TARGET,
            )?;
        }
        if let Some(value) = self.consoleet_codes.as_deref() {
            check_choice("consoleet_codes", value, &WRITE_OPTIONS_CONSOLEET_CODES)?;
        }
        if let Some(value) = self.xbin_count.as_deref() {
            check_choice("xbin_count", value, &WRITE_OPTIONS_XBIN_COUNT)?;
        }
        if let Some(value) = self.xbin_screen.as_deref() {
            check_choice("xbin_screen", value, &WRITE_OPTIONS_XBIN_SCREEN)?;
        }
        if let Some(value) = self.zapfont_variant.as_deref() {
            check_choice("zapfont_variant", value, &WRITE_OPTIONS_ZAPFONT_VARIANT)?;
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
        cell: Option<fontpixel::bitmap::Bounds>,
        registry: &mut FontRegistry,
    ) -> Result<()> {
        target(
            self.comfont_variant.is_some() || self.comfont_cpm_target.is_some(),
            id,
            "comfont",
        )?;
        target(self.consoleet_codes.is_some(), id, "consoleet")?;
        target(
            self.xbin_count.is_some() || self.xbin_screen.is_some(),
            id,
            "xbin",
        )?;
        target(self.zapfont_variant.is_some(), id, "zapfont")?;
        #[cfg(feature = "bbc")]
        if id == "bbc" {
            registry.set_bbc_write_options(fontpixel::formats::bbc::BbcWriteOptions {
                cell,
                gray_threshold: threshold,
                unicode_as_legacy,
                limits: limits.clone(),
            });
        }
        #[cfg(feature = "wyse")]
        if id == "wyse" {
            registry.set_wyse_write_options(fontpixel::formats::wyse::WyseWriteOptions {
                cell,
                gray_threshold: threshold,
                unicode_as_legacy,
                limits: limits.clone(),
            });
        }
        #[cfg(feature = "grasp")]
        if id == "grasp" {
            registry.set_grasp_write_options(fontpixel::formats::grasp::GraspWriteOptions {
                cell,
                gray_threshold: threshold,
                unicode_as_legacy,
                limits: limits.clone(),
            });
        }
        #[cfg(feature = "hrcg")]
        if id == "hrcg" {
            registry.set_hrcg_write_options(fontpixel::formats::hrcg::HrcgWriteOptions {
                cell,
                gray_threshold: threshold,
                unicode_as_legacy,
                limits: limits.clone(),
            });
        }
        #[cfg(feature = "xbin")]
        if id == "xbin" {
            registry.set_xbin_write_options(fontpixel::formats::xbin::XbinWriteOptions {
                cell,
                gray_threshold: threshold,
                unicode_as_legacy,
                limits: limits.clone(),
                count: match self.xbin_count.as_deref() {
                    Some("256") => fontpixel::formats::xbin::CharacterCount::Chars256,
                    Some("512") => fontpixel::formats::xbin::CharacterCount::Chars512,
                    _ => fontpixel::formats::xbin::CharacterCount::Auto,
                },
                screen: if self.xbin_screen.as_deref() == Some("font-only") {
                    fontpixel::formats::xbin::ScreenMode::FontOnly
                } else {
                    fontpixel::formats::xbin::ScreenMode::Preserve
                },
            });
        }
        #[cfg(feature = "comfont")]
        if id == "comfont" {
            registry.set_comfont_write_options(fontpixel::formats::comfont::ComfontWriteOptions {
                cell,
                gray_threshold: threshold,
                unicode_as_legacy,
                limits: limits.clone(),
                variant: comfont_variant(self.comfont_variant.as_deref().unwrap_or("frapt")),
                cpm_target: match self.comfont_cpm_target.as_deref() {
                    Some("cpc") => fontpixel::formats::comfont::CpmTarget::Cpc,
                    Some("spectrum") => fontpixel::formats::comfont::CpmTarget::Spectrum,
                    _ => fontpixel::formats::comfont::CpmTarget::Pcw,
                },
            });
        }
        #[cfg(feature = "consoleet")]
        if id == "consoleet" {
            registry.set_consoleet_write_options(
                fontpixel::formats::consoleet::ConsoleetWriteOptions {
                    cell,
                    gray_threshold: threshold,
                    unicode_as_legacy,
                    limits: limits.clone(),
                    codepoint_mode: consoleet_codes(self.consoleet_codes.as_deref()),
                },
            );
        }
        #[cfg(feature = "zapfont")]
        if id == "zapfont" {
            registry.set_zapfont_write_options(fontpixel::formats::zapfont::ZapfontWriteOptions {
                cell,
                gray_threshold: threshold,
                unicode_as_legacy,
                limits: limits.clone(),
                variant: zap_variant(self.zapfont_variant.as_deref().unwrap_or("redraw")),
            });
        }
        let _ = (limits, threshold, unicode_as_legacy, cell, registry);
        Ok(())
    }
}
#[cfg(feature = "comfont")]
fn comfont_variant(v: &str) -> fontpixel::formats::comfont::ComfontVariant {
    use fontpixel::formats::comfont::ComfontVariant;
    match v {
        "fontedit" => ComfontVariant::Fontedit,
        "frapt-tsr" => ComfontVariant::FraptTsr,
        "letafont" => ComfontVariant::Letafont,
        "mania" => ComfontVariant::Mania,
        "psfcom" => ComfontVariant::Psfcom,
        "udg" => ComfontVariant::Udg,
        _ => ComfontVariant::Frapt,
    }
}
#[cfg(feature = "consoleet")]
fn consoleet_codes(v: Option<&str>) -> fontpixel::formats::consoleet::CodepointMode {
    use fontpixel::formats::consoleet::CodepointMode;
    if v == Some("unicode") {
        CodepointMode::Unicode
    } else {
        CodepointMode::Legacy
    }
}
#[cfg(feature = "zapfont")]
fn zap_variant(v: &str) -> fontpixel::formats::zapfont::ZapVariant {
    use fontpixel::formats::zapfont::ZapVariant;
    if v == "old" {
        ZapVariant::Old
    } else {
        ZapVariant::Redraw
    }
}
