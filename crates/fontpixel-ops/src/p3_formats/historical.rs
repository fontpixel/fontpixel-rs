//! Native historical printer-font variants and associated metric files.
use super::{feature, target};
use crate::options::{check_choice, check_range};
use fontpixel::{FontRegistry, ResourceLimits, Result};

const READ_OPTIONS_DAISY_READ_VERSION: [&str; 3] = ["2", "3", "magnified"];
const READ_OPTIONS_TASPRINT_READ_VERSION: [&str; 4] = ["48k", "plus3", "cpc", "pcw"];
const READ_OPTIONS_PXL_READ_VERSION: [&str; 2] = ["1001", "1002"];
const READ_OPTIONS_SIGNUM_READ_VERSION: [&str; 4] = ["e24", "p9", "p24", "l30"];
/// Read options: native historical printer-font variants and associated metric files.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct ReadOptions {
    /// Require a Daisy-Dot input generation.
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_DAISY_READ_VERSION))]
    pub daisy_read_version: Option<String>,
    /// PolyPrint baseline measured from the cell top.
    #[cfg_attr(feature = "clap", arg(long, allow_hyphen_values = true))]
    pub polyprint_baseline: Option<i64>,
    /// Print Shop baseline measured from the cell top.
    #[cfg_attr(feature = "clap", arg(long, allow_hyphen_values = true))]
    pub printshop_baseline: Option<i64>,
    /// Require a TasPrint input platform.
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_TASPRINT_READ_VERSION))]
    pub tasprint_read_version: Option<String>,
    /// Explicit PCW input width.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u8).range(1..=16)))]
    pub tasprint_read_width: Option<u8>,
    /// Require PXL version 1001 or 1002.
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_PXL_READ_VERSION))]
    pub pxl_read_version: Option<String>,
    /// Scoped TFM sidecar path for PXL input.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pxl_read_tfm: Option<String>,
    /// Require a Signum editor or printer input layout.
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_SIGNUM_READ_VERSION))]
    pub signum_read_version: Option<String>,
    /// Explicit Signum baseline.
    #[cfg_attr(feature = "clap", arg(long, allow_hyphen_values = true))]
    pub signum_baseline: Option<i64>,
    /// Explicit Signum line height.
    #[cfg_attr(feature = "clap", arg(long, allow_hyphen_values = true))]
    pub signum_line_height: Option<i64>,
}
impl ReadOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.daisy_read_version.as_deref() {
            check_choice(
                "daisy_read_version",
                value,
                &READ_OPTIONS_DAISY_READ_VERSION,
            )?;
        }
        if let Some(value) = self.tasprint_read_version.as_deref() {
            check_choice(
                "tasprint_read_version",
                value,
                &READ_OPTIONS_TASPRINT_READ_VERSION,
            )?;
        }
        if let Some(value) = self.tasprint_read_width {
            check_range("tasprint_read_width", i128::from(value), 1, 16)?;
        }
        if let Some(value) = self.pxl_read_version.as_deref() {
            check_choice("pxl_read_version", value, &READ_OPTIONS_PXL_READ_VERSION)?;
        }
        if let Some(value) = self.signum_read_version.as_deref() {
            check_choice(
                "signum_read_version",
                value,
                &READ_OPTIONS_SIGNUM_READ_VERSION,
            )?;
        }
        Ok(())
    }
}
impl ReadOptions {
    pub(crate) fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        feature(
            self.daisy_read_version.is_some(),
            cfg!(feature = "daisy"),
            "daisy",
        )?;
        feature(
            self.polyprint_baseline.is_some(),
            cfg!(feature = "polyprint"),
            "polyprint",
        )?;
        feature(
            self.printshop_baseline.is_some(),
            cfg!(feature = "printshop"),
            "printshop",
        )?;
        feature(
            self.tasprint_read_version.is_some() || self.tasprint_read_width.is_some(),
            cfg!(feature = "tasprint"),
            "tasprint",
        )?;
        feature(
            self.pxl_read_version.is_some() || self.pxl_read_tfm.is_some(),
            cfg!(feature = "pxl"),
            "pxl",
        )?;
        feature(
            self.signum_read_version.is_some()
                || self.signum_baseline.is_some()
                || self.signum_line_height.is_some(),
            cfg!(feature = "signum"),
            "signum",
        )?;
        #[cfg(feature = "daisy")]
        registry.set_daisy_read_options(fontpixel::formats::daisy::DaisyReadOptions {
            version: self.daisy_read_version.as_deref().map(daisy_version),
        });
        #[cfg(feature = "polyprint")]
        registry.set_polyprint_read_options(fontpixel::formats::polyprint::PolyprintReadOptions {
            baseline: self.polyprint_baseline,
        });
        #[cfg(feature = "printshop")]
        registry.set_printshop_read_options(fontpixel::formats::printshop::PrintshopReadOptions {
            baseline: self.printshop_baseline,
        });
        #[cfg(feature = "tasprint")]
        registry.set_tasprint_read_options(fontpixel::formats::tasprint::TasprintReadOptions {
            version: self.tasprint_read_version.as_deref().map(tasprint_version),
            width: self.tasprint_read_width,
        });
        #[cfg(feature = "pxl")]
        registry.set_pxl_read_options(fontpixel::formats::pxl::PxlReadOptions {
            version: self.pxl_read_version.as_deref().map(pxl_version),
            tfm: self
                .pxl_read_tfm
                .as_deref()
                .map(fontpixel::ResourcePath::new)
                .transpose()?,
        });
        #[cfg(feature = "signum")]
        registry.set_signum_read_options(fontpixel::formats::signum::SignumReadOptions {
            version: self.signum_read_version.as_deref().map(signum_version),
            baseline: self.signum_baseline,
            line_height: self.signum_line_height,
        });
        let _ = registry;
        Ok(())
    }
}

const WRITE_OPTIONS_DAISY_VERSION: [&str; 3] = ["2", "3", "magnified"];
const WRITE_OPTIONS_TASPRINT_VERSION: [&str; 4] = ["48k", "plus3", "cpc", "pcw"];
const WRITE_OPTIONS_PXL_VERSION: [&str; 2] = ["1001", "1002"];
const WRITE_OPTIONS_SIGNUM_VERSION: [&str; 4] = ["e24", "p9", "p24", "l30"];
/// Write options: native historical printer-font variants and associated metric files.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct WriteOptions {
    /// Daisy-Dot output generation (default 3).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_DAISY_VERSION))]
    pub daisy_version: Option<String>,
    /// Actual Daisy-Dot height marker plus one.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u8).range(1..)))]
    pub daisy_height: Option<u8>,
    /// Daisy-Dot underline row, which defines its baseline.
    #[cfg_attr(feature = "clap", arg(long))]
    pub daisy_underline: Option<u8>,
    /// Explicitly allow graphics bytes forbidden by the Daisy-Dot manual.
    #[cfg_attr(feature = "clap", arg(long))]
    pub daisy_allow_control_bytes: bool,
    /// TasPrint output platform (default 48k).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_TASPRINT_VERSION))]
    pub tasprint_version: Option<String>,
    /// TasPrint PCW output width.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u8).range(1..=16)))]
    pub tasprint_width: Option<u8>,
    /// PCW character count: the documented 127 or observed 128.
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u16).range(127..=128)))]
    pub tasprint_pcw_count: Option<u16>,
    /// Native PXL output generation (default 1001).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_PXL_VERSION))]
    pub pxl_version: Option<String>,
    /// Optional relative name of the generated PXL metric sidecar.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pxl_tfm: Option<String>,
    /// PXL magnification value.
    #[cfg_attr(feature = "clap", arg(long))]
    pub pxl_magnification: Option<u32>,
    /// Signum output layout (default e24).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_SIGNUM_VERSION))]
    pub signum_version: Option<String>,
}
impl WriteOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.daisy_version.as_deref() {
            check_choice("daisy_version", value, &WRITE_OPTIONS_DAISY_VERSION)?;
        }
        if let Some(value) = self.daisy_height {
            check_range("daisy_height", i128::from(value), 1, i128::from(u8::MAX))?;
        }
        if let Some(value) = self.tasprint_version.as_deref() {
            check_choice("tasprint_version", value, &WRITE_OPTIONS_TASPRINT_VERSION)?;
        }
        if let Some(value) = self.tasprint_width {
            check_range("tasprint_width", i128::from(value), 1, 16)?;
        }
        if let Some(value) = self.tasprint_pcw_count {
            check_range("tasprint_pcw_count", i128::from(value), 127, 128)?;
        }
        if let Some(value) = self.pxl_version.as_deref() {
            check_choice("pxl_version", value, &WRITE_OPTIONS_PXL_VERSION)?;
        }
        if let Some(value) = self.signum_version.as_deref() {
            check_choice("signum_version", value, &WRITE_OPTIONS_SIGNUM_VERSION)?;
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
        registry: &mut FontRegistry,
    ) -> Result<()> {
        target(
            self.daisy_version.is_some()
                || self.daisy_height.is_some()
                || self.daisy_underline.is_some()
                || self.daisy_allow_control_bytes,
            id,
            "daisy",
        )?;
        target(
            self.tasprint_version.is_some()
                || self.tasprint_width.is_some()
                || self.tasprint_pcw_count.is_some(),
            id,
            "tasprint",
        )?;
        target(
            self.pxl_version.is_some()
                || self.pxl_tfm.is_some()
                || self.pxl_magnification.is_some(),
            id,
            "pxl",
        )?;
        target(self.signum_version.is_some(), id, "signum")?;
        #[cfg(feature = "daisy")]
        if id == "daisy" {
            registry.set_daisy_write_options(fontpixel::formats::daisy::DaisyWriteOptions {
                version: self
                    .daisy_version
                    .as_deref()
                    .map(daisy_version)
                    .unwrap_or_default(),
                height: self.daisy_height,
                underline: self.daisy_underline,
                allow_control_bytes: self.daisy_allow_control_bytes,
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "polyprint")]
        if id == "polyprint" {
            registry.set_polyprint_write_options(
                fontpixel::formats::polyprint::PolyprintWriteOptions {
                    unicode_as_legacy,
                    gray_threshold: threshold,
                    limits: limits.clone(),
                    ..Default::default()
                },
            );
        }
        #[cfg(feature = "printshop")]
        if id == "printshop" {
            registry.set_printshop_write_options(
                fontpixel::formats::printshop::PrintshopWriteOptions {
                    unicode_as_legacy,
                    gray_threshold: threshold,
                    limits: limits.clone(),
                    ..Default::default()
                },
            );
        }
        #[cfg(feature = "tasprint")]
        if id == "tasprint" {
            registry.set_tasprint_write_options(
                fontpixel::formats::tasprint::TasprintWriteOptions {
                    version: self
                        .tasprint_version
                        .as_deref()
                        .map(tasprint_version)
                        .unwrap_or_default(),
                    width: self.tasprint_width,
                    pcw_count: self.tasprint_pcw_count.unwrap_or(128),
                    unicode_as_legacy,
                    gray_threshold: threshold,
                    limits: limits.clone(),
                    ..Default::default()
                },
            );
        }
        #[cfg(feature = "pxl")]
        if id == "pxl" {
            registry.set_pxl_write_options(fontpixel::formats::pxl::PxlWriteOptions {
                version: self
                    .pxl_version
                    .as_deref()
                    .map(pxl_version)
                    .unwrap_or_default(),
                tfm: self
                    .pxl_tfm
                    .as_deref()
                    .map(fontpixel::ResourcePath::new)
                    .transpose()?,
                magnification: self.pxl_magnification,
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "signum")]
        if id == "signum" {
            registry.set_signum_write_options(fontpixel::formats::signum::SignumWriteOptions {
                version: self
                    .signum_version
                    .as_deref()
                    .map(signum_version)
                    .unwrap_or_default(),
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        let _ = (limits, threshold, unicode_as_legacy, registry);
        Ok(())
    }
}

#[cfg(feature = "daisy")]
fn daisy_version(value: &str) -> fontpixel::formats::daisy::DaisyVersion {
    use fontpixel::formats::daisy::DaisyVersion;
    match value {
        "2" => DaisyVersion::V2,
        "magnified" => DaisyVersion::Magnified,
        _ => DaisyVersion::V3,
    }
}
#[cfg(feature = "tasprint")]
fn tasprint_version(value: &str) -> fontpixel::formats::tasprint::TasprintVersion {
    use fontpixel::formats::tasprint::TasprintVersion;
    match value {
        "plus3" => TasprintVersion::Plus3,
        "cpc" => TasprintVersion::Cpc,
        "pcw" => TasprintVersion::Pcw,
        _ => TasprintVersion::Spectrum48k,
    }
}
#[cfg(feature = "pxl")]
fn pxl_version(value: &str) -> fontpixel::formats::pxl::PxlVersion {
    use fontpixel::formats::pxl::PxlVersion;
    if value == "1002" {
        PxlVersion::V1002
    } else {
        PxlVersion::V1001
    }
}
#[cfg(feature = "signum")]
fn signum_version(value: &str) -> fontpixel::formats::signum::SignumVersion {
    use fontpixel::formats::signum::SignumVersion;
    match value {
        "p9" => SignumVersion::P9,
        "p24" => SignumVersion::P24,
        "l30" => SignumVersion::L30,
        _ => SignumVersion::E24,
    }
}
