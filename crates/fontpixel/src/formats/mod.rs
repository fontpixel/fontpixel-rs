//! Capability registration and model-independent dispatch infrastructure.
//!
//! The generic registry starts with planned entries. `crate::registry()` installs
//! actual FontSet codecs under their Cargo features.
pub use crate::resources::TransportLimits;
use crate::resources::{Input, ReadContext, ResourceLimits, ResourcePath, Written};
use crate::{Error, Operation, Report, Result};

#[cfg(feature = "amiga")]
pub mod amiga;
#[cfg(feature = "bdf")]
pub mod bdf;
#[cfg(feature = "bmfont")]
pub mod bmfont;
#[cfg(feature = "c")]
pub mod c;
pub mod catalogue;
#[cfg(any(
    feature = "hex",
    feature = "psf",
    feature = "draw",
    feature = "raw",
    feature = "cpi"
))]
mod console;
#[cfg(feature = "cpi")]
pub mod cpi;
#[cfg(feature = "draw")]
pub mod draw;
#[cfg(any(feature = "bmfont", feature = "sfd", feature = "kbits"))]
#[path = "bmfont/fidelity.rs"]
pub(crate) mod editor_fidelity;
#[cfg(any(
    feature = "otb",
    feature = "bdat",
    feature = "color",
    feature = "vector-raster"
))]
pub mod embedded;
#[cfg(feature = "fnt")]
pub mod fnt;
#[cfg(feature = "fon")]
pub mod fon;
#[cfg(feature = "fontx2")]
pub mod fontx2;
#[cfg(feature = "gem")]
pub mod gem;
#[cfg(feature = "gf")]
pub mod gf;
#[cfg(feature = "hanfont")]
pub mod hanfont;
#[cfg(feature = "hbf")]
pub mod hbf;
#[cfg(feature = "hex")]
pub mod hex;
#[cfg(feature = "hzk")]
pub mod hzk;
#[cfg(feature = "iigs")]
pub mod iigs;
#[cfg(feature = "image")]
pub mod image;
#[cfg(feature = "kbits")]
pub mod kbits;
#[cfg(feature = "kbitx")]
pub mod kbitx;
#[cfg(any(
    feature = "amiga",
    feature = "gem",
    feature = "snf",
    feature = "vfont",
    feature = "fzx",
    feature = "figlet",
    feature = "palm",
    feature = "gdr",
    feature = "dosstart",
    feature = "edwin",
    feature = "mkwinfont",
    feature = "psf2txt",
    feature = "pcr",
    feature = "plus3dos",
    feature = "drhalo",
    feature = "writeon",
    feature = "daisy",
    feature = "polyprint",
    feature = "printshop",
    feature = "tasprint",
    feature = "pxl",
    feature = "signum",
    feature = "rockbox",
    feature = "u8m",
    feature = "cybiko",
    feature = "playdate",
    feature = "hmzk",
    feature = "sbf",
    feature = "beos",
    feature = "geos",
    feature = "gpi",
    feature = "oberon",
    feature = "pcgeos",
    feature = "symbos",
    feature = "vtfont",
    feature = "wsfont",
    feature = "bbc",
    feature = "comfont",
    feature = "consoleet",
    feature = "grasp",
    feature = "hrcg",
    feature = "wyse",
    feature = "xbin",
    feature = "zapfont",
    feature = "catalyst",
    feature = "chiwriter",
    feature = "dashen",
    feature = "hppcl",
    feature = "prebuilt",
    feature = "riscos",
    feature = "xerox",
    feature = "pilfont",
    feature = "sfont",
    feature = "vlw",
    feature = "image",
    feature = "ssfn",
    feature = "pfr"
))]
#[path = "gem/common.rs"]
pub(crate) mod legacy_common;
#[cfg(feature = "nfnt")]
pub mod nfnt;
#[cfg(feature = "palm")]
pub mod palm;
#[cfg(feature = "pbf")]
pub mod pbf;
#[cfg(feature = "pcf")]
pub mod pcf;
#[cfg(feature = "pf2")]
pub mod pf2;
#[cfg(feature = "pk")]
pub mod pk;
#[cfg(feature = "plan9")]
pub mod plan9;
#[cfg(feature = "psf")]
pub mod psf;
#[cfg(feature = "raw")]
pub mod raw;
mod registry_codecs;
#[cfg(feature = "sfd")]
pub mod sfd;
#[cfg(feature = "snf")]
pub mod snf;
#[cfg(feature = "svg")]
pub mod svg;
#[cfg(any(feature = "pk", feature = "gf", feature = "plan9"))]
#[path = "pk/common.rs"]
pub(crate) mod tex_common;
mod transport;
#[cfg(feature = "vector-raster")]
pub mod vector_raster;
#[cfg(any(feature = "pf2", feature = "fontx2", feature = "hzk"))]
#[path = "pf2/common.rs"]
pub(crate) mod w42_common;
#[cfg(any(feature = "bdf", feature = "pcf"))]
pub(crate) mod x11;
#[cfg(feature = "yaff")]
pub mod yaff;

#[cfg(feature = "figlet")]
pub mod figlet;
#[cfg(feature = "fzx")]
pub mod fzx;
#[cfg(feature = "vfont")]
pub mod vfont;

#[cfg(feature = "drcs")]
pub mod drcs;
#[cfg(feature = "gdr")]
pub mod gdr;

#[cfg(feature = "pf2")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_pf2(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "pf2") {
            e.detector = Some(Box::new(pf2::Pf2Codec::default()));
            e.reader = Some(Box::new(pf2::Pf2Codec::default()));
            e.writer = Some(transport::writer(pf2::Pf2Codec::default()));
        }
    }
    /// Set explicit PF2 read options.
    pub fn set_pf2_read_options(&mut self, options: pf2::Pf2ReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "pf2" && e.reader.is_some())
        {
            e.reader = Some(Box::new(pf2::Pf2Codec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Set explicit PF2 write options.
    pub fn set_pf2_write_options(&mut self, options: pf2::Pf2WriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "pf2" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(pf2::Pf2Codec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "fontx2")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_fontx2(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "fontx2") {
            e.detector = Some(Box::new(fontx2::Fontx2Codec::default()));
            e.reader = Some(Box::new(fontx2::Fontx2Codec::default()));
            e.writer = Some(transport::writer(fontx2::Fontx2Codec::default()));
        }
    }
    /// Set explicit FONTX2 read options.
    pub fn set_fontx2_read_options(&mut self, options: fontx2::Fontx2ReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "fontx2" && e.reader.is_some())
        {
            e.reader = Some(Box::new(fontx2::Fontx2Codec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Set explicit FONTX2 write options.
    pub fn set_fontx2_write_options(&mut self, options: fontx2::Fontx2WriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "fontx2" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(fontx2::Fontx2Codec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "hzk")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_hzk(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "hzk") {
            // Headerless HZK requires explicit geometry or a preset.
            e.reader = Some(Box::new(hzk::HzkCodec::default()));
            e.writer = Some(transport::writer(hzk::HzkCodec::default()));
        }
    }
    /// Set explicit HZK read options.
    pub fn set_hzk_read_options(&mut self, options: hzk::HzkReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "hzk" && e.reader.is_some())
        {
            e.reader = Some(Box::new(hzk::HzkCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Set explicit HZK write options.
    pub fn set_hzk_write_options(&mut self, options: hzk::HzkWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "hzk" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(hzk::HzkCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "draw")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_draw(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "draw") {
            e.detector = Some(Box::new(draw::DrawCodec::default()));
            e.reader = Some(Box::new(draw::DrawCodec::default()));
            e.writer = Some(transport::writer(draw::DrawCodec::default()));
        }
    }

    /// Set explicit DRAW write options.
    pub fn set_draw_write_options(&mut self, options: draw::DrawWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "draw" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(draw::DrawCodec {
                write_options: options,
            }));
        }
    }
}

#[cfg(feature = "raw")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_raw(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "raw") {
            // Headerless raw data requires an explicit layout.
            e.reader = Some(Box::new(raw::RawCodec::default()));
            e.writer = Some(transport::writer(raw::RawCodec::default()));
        }
    }
    /// Set explicit RAW read options.
    pub fn set_raw_read_options(&mut self, options: raw::RawReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "raw" && e.reader.is_some())
        {
            e.reader = Some(Box::new(raw::RawCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Set explicit RAW write options.
    pub fn set_raw_write_options(&mut self, options: raw::RawWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "raw" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(raw::RawCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "cpi")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_cpi(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "cpi") {
            e.detector = Some(Box::new(cpi::CpiCodec::default()));
            e.reader = Some(Box::new(cpi::CpiCodec::default()));
            e.writer = Some(transport::writer(cpi::CpiCodec::default()));
        }
    }
    /// Set explicit CPI read options.
    pub fn set_cpi_read_options(&mut self, options: cpi::CpiReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "cpi" && e.reader.is_some())
        {
            e.reader = Some(Box::new(cpi::CpiCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Set explicit CPI write options.
    pub fn set_cpi_write_options(&mut self, options: cpi::CpiWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "cpi" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(cpi::CpiCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "nfnt")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_nfnt(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "nfnt") {
            e.detector = Some(Box::new(nfnt::NfntCodec::default()));
            e.reader = Some(Box::new(nfnt::NfntCodec::default()));
            e.writer = Some(transport::writer(nfnt::NfntCodec::default()));
        }
    }
    /// Set explicit NFNT read options.
    pub fn set_nfnt_read_options(&mut self, options: nfnt::NfntReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "nfnt" && e.reader.is_some())
        {
            e.reader = Some(Box::new(nfnt::NfntCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Set explicit NFNT write options.
    pub fn set_nfnt_write_options(&mut self, options: nfnt::NfntWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "nfnt" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(nfnt::NfntCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "bmfont")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_bmfont(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "bmfont") {
            e.detector = Some(Box::new(bmfont::BmfontCodec::default()));
            e.reader = Some(Box::new(bmfont::BmfontCodec::default()));
            e.writer = Some(transport::writer(bmfont::BmfontCodec::default()));
        }
    }
    /// Set explicit BMFONT read options.
    pub fn set_bmfont_read_options(&mut self, options: bmfont::BmfontReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "bmfont" && e.reader.is_some())
        {
            e.reader = Some(Box::new(bmfont::BmfontCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Set explicit BMFONT write options.
    pub fn set_bmfont_write_options(&mut self, options: bmfont::BmfontWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "bmfont" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(bmfont::BmfontCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "sfd")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_sfd(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "sfd") {
            e.detector = Some(Box::new(sfd::SfdCodec::default()));
            e.reader = Some(Box::new(sfd::SfdCodec::default()));
            e.writer = Some(transport::writer(sfd::SfdCodec::default()));
        }
    }

    /// Set explicit SFD write options.
    pub fn set_sfd_write_options(&mut self, options: sfd::SfdWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "sfd" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(sfd::SfdCodec {
                write_options: options,
            }));
        }
    }
}

#[cfg(feature = "kbits")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_kbits(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "kbits") {
            e.detector = Some(Box::new(kbits::KbitsCodec::default()));
            e.reader = Some(Box::new(kbits::KbitsCodec::default()));
            e.writer = Some(transport::writer(kbits::KbitsCodec::default()));
        }
    }

    /// Set explicit KBITS write options.
    pub fn set_kbits_write_options(&mut self, options: kbits::KbitsWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "kbits" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(kbits::KbitsCodec {
                write_options: options,
            }));
        }
    }
}

#[cfg(feature = "pk")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_pk(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "pk") {
            e.detector = Some(Box::new(pk::PkCodec::default()));
            e.reader = Some(Box::new(pk::PkCodec::default()));
            e.writer = Some(transport::writer(pk::PkCodec::default()));
        }
    }

    /// Set explicit PK write options.
    pub fn set_pk_write_options(&mut self, options: pk::PkWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "pk" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(pk::PkCodec {
                write_options: options,
            }));
        }
    }
}

#[cfg(feature = "gf")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_gf(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "gf") {
            e.detector = Some(Box::new(gf::GfCodec::default()));
            e.reader = Some(Box::new(gf::GfCodec::default()));
            e.writer = Some(transport::writer(gf::GfCodec::default()));
        }
    }

    /// Set explicit GF write options.
    pub fn set_gf_write_options(&mut self, options: gf::GfWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "gf" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(gf::GfCodec {
                write_options: options,
            }));
        }
    }
}

#[cfg(feature = "plan9")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_plan9(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "plan9") {
            e.detector = Some(Box::new(plan9::Plan9Codec::default()));
            e.reader = Some(Box::new(plan9::Plan9Codec::default()));
            e.writer = Some(transport::writer(plan9::Plan9Codec::default()));
        }
    }

    /// Set explicit PLAN9 write options.
    pub fn set_plan9_write_options(&mut self, options: plan9::Plan9WriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "plan9" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(plan9::Plan9Codec {
                write_options: options,
            }));
        }
    }
}

#[cfg(feature = "amiga")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_amiga(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "amiga") {
            e.detector = Some(Box::new(amiga::AmigaCodec::default()));
            e.reader = Some(Box::new(amiga::AmigaCodec::default()));
            e.writer = Some(transport::writer(amiga::AmigaCodec::default()));
        }
    }
    /// Set explicit AMIGA read options.
    pub fn set_amiga_read_options(&mut self, options: amiga::AmigaReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "amiga" && e.reader.is_some())
        {
            e.reader = Some(Box::new(amiga::AmigaCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Set explicit AMIGA write options.
    pub fn set_amiga_write_options(&mut self, options: amiga::AmigaWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "amiga" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(amiga::AmigaCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "gem")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_gem(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "gem") {
            e.detector = Some(Box::new(gem::GemCodec::default()));
            e.reader = Some(Box::new(gem::GemCodec::default()));
            e.writer = Some(transport::writer(gem::GemCodec::default()));
        }
    }
    /// Set explicit GEM read options.
    pub fn set_gem_read_options(&mut self, options: gem::GemReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "gem" && e.reader.is_some())
        {
            e.detector = Some(Box::new(gem::GemCodec {
                read_options: options.clone(),
                ..Default::default()
            }));
            e.reader = Some(Box::new(gem::GemCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Set explicit GEM write options.
    pub fn set_gem_write_options(&mut self, options: gem::GemWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "gem" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(gem::GemCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "snf")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_snf(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "snf") {
            e.detector = Some(Box::new(snf::SnfCodec::default()));
            e.reader = Some(Box::new(snf::SnfCodec::default()));
            e.writer = Some(transport::writer(snf::SnfCodec::default()));
        }
    }
    /// Set explicit SNF read options.
    pub fn set_snf_read_options(&mut self, options: snf::SnfReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "snf" && e.reader.is_some())
        {
            e.detector = Some(Box::new(snf::SnfCodec {
                read_options: options.clone(),
                ..Default::default()
            }));
            e.reader = Some(Box::new(snf::SnfCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Set explicit SNF write options.
    pub fn set_snf_write_options(&mut self, options: snf::SnfWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "snf" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(snf::SnfCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

/// Static format metadata, including compiled feature state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FormatInfo {
    /// Stable CLI/API identifier.
    pub id: &'static str,
    /// Human-readable name.
    pub name: &'static str,
    /// Possible extensions without dots; hints only, including compound suffixes.
    pub extensions: &'static [&'static str],
    /// Whether this build enables the format's Cargo feature.
    pub feature_enabled: bool,
}

/// Available hooks, separate from catalogue membership and feature selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FormatStatus {
    /// Catalogue information.
    pub format: FormatInfo,
    /// An actual content detector is registered.
    pub detect: bool,
    /// An actual reader is registered.
    pub read: bool,
    /// An actual writer is registered.
    pub write: bool,
}

/// Increasing strength of structural evidence, with filename hints below content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    /// No content evidence.
    NoMatch,
    /// Extension-only hint, returned by the registry (not a detector).
    Extension,
    /// Plausible content; may be ambiguous.
    Possible,
    /// Strong magic and/or validated structure.
    Certain,
}

/// Successful detection, explicitly preserving the strength of its evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detected {
    /// Selected format.
    pub format: FormatInfo,
    /// How the selection was made.
    pub confidence: Confidence,
}

/// Common options; each implemented format will add strongly typed options.
#[derive(Debug, Clone, Default)]
pub struct ReadOptions {
    /// Explicit ID bypassing auto-detection.
    pub format: Option<String>,
    /// Input resource limits.
    pub limits: ResourceLimits,
}

/// Common write options. Format-specific options belong to later format modules.
#[derive(Debug, Clone)]
pub struct WriteOptions {
    /// Refuse to return assets if the combined read/write report has loss.
    pub strict: bool,
    /// Primary relative output name; codecs may add sibling/nested sidecars.
    pub primary_name: ResourcePath,
    /// Additional native-write and gzip/envelope budgets. `None` preserves the
    /// typed writer's limits. Present limits intersect those limits, so a common
    /// setting cannot silently enlarge a codec budget. Built-in native wrappers
    /// validate the model and restrict their codec before writing; compressed
    /// output also enforces the applicable envelope and replay budgets.
    /// Plain output additionally checks the sum of all returned asset lengths
    /// against `max_output_bytes`, after error/strict diagnostics. Codecs without
    /// a native output cap may encode under their resource budget before this
    /// returned-output check. A typed per-asset cap is not an aggregate cap.
    pub transport_limits: Option<TransportLimits>,
}

impl WriteOptions {
    /// Create non-strict options with a validated primary asset name.
    pub fn new(primary_name: ResourcePath) -> Self {
        Self {
            strict: false,
            primary_name,
            transport_limits: None,
        }
    }
}

/// A decoded model and all read diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded<M> {
    /// Decoded model (`FontSet` for the built-in font registry).
    pub fonts: M,
    /// Reader diagnostics.
    pub report: Report,
    /// The actual format used to decode the input.
    pub format: &'static str,
}

/// Format content-recognition hook. No filesystem writes are allowed.
pub trait FormatDetector: Send + Sync {
    /// Inspect bounded input; do not treat its filename as content evidence.
    fn detect(&self, input: &mut ReadContext<'_>) -> Result<Confidence>;
}

/// Format reader over a shared model, including caller-supplied sidecars.
pub trait FormatReader<M>: Send + Sync {
    /// Parse one complete input; diagnostics must describe repairs and loss.
    fn read(&self, input: &mut ReadContext<'_>, options: &ReadOptions) -> Result<(M, Report)>;
}

/// Side-effect-free writer returning all assets and loss diagnostics.
pub trait FormatWriter<M>: Send + Sync {
    /// Encode the model entirely in memory. Never persist output here: strict
    /// rejection happens in the registry after the full report is available.
    fn write(&self, fonts: &M, options: &WriteOptions) -> Result<Written>;

    /// Budgets for registry-applied envelopes. Built-in registrations forward
    /// their actual typed writer limits. Custom writers may override this hook.
    fn transport_limits(&self) -> TransportLimits {
        TransportLimits::default()
    }

    /// Validate the same native output profile with the memory left after
    /// retaining the gzip result and diagnostics. Called only for gzip output,
    /// after its uncompressed source buffer has been released. The default
    /// leaves native validation to custom writers; envelope budgets still apply.
    fn validate_transport(
        &self,
        _fonts: &M,
        _options: &WriteOptions,
        _limits: &TransportLimits,
    ) -> Result<()> {
        Ok(())
    }
}

/// One catalogue entry with independently installable capabilities.
pub struct Registration<M> {
    /// Static metadata.
    pub format: FormatInfo,
    /// Optional content detector.
    pub detector: Option<Box<dyn FormatDetector>>,
    /// Optional model reader.
    pub reader: Option<Box<dyn FormatReader<M>>>,
    /// Optional model writer.
    pub writer: Option<Box<dyn FormatWriter<M>>>,
}

impl<M> Registration<M> {
    /// An honest unimplemented entry with no capability hooks.
    pub fn planned(format: FormatInfo) -> Self {
        Self {
            format,
            detector: None,
            reader: None,
            writer: None,
        }
    }

    /// Runtime capabilities of this entry.
    pub fn status(&self) -> FormatStatus {
        FormatStatus {
            format: self.format,
            detect: self.detector.is_some(),
            read: self.reader.is_some(),
            write: self.writer.is_some(),
        }
    }
}

/// Extensible model-independent format registry. [`crate::FontRegistry`]
/// specializes it with the real FontSet without duplicating dispatch contracts.
pub struct Registry<M> {
    entries: Vec<Registration<M>>,
}

impl<M> Default for Registry<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M> Registry<M> {
    /// Generic planned catalogue. Use `crate::registry()` for built-in FontSet codecs.
    pub fn new() -> Self {
        Self {
            entries: catalogue::CATALOGUE
                .iter()
                .copied()
                .map(Registration::planned)
                .collect(),
        }
    }

    /// Iterate all formats in catalogue/registration order, including unavailable ones.
    pub fn formats(&self) -> impl ExactSizeIterator<Item = FormatStatus> + '_ {
        self.entries.iter().map(Registration::status)
    }

    /// Look up metadata and actual capabilities by exact stable ID.
    pub fn status(&self, id: &str) -> Result<FormatStatus> {
        Ok(self.entry(id)?.status())
    }

    fn entry(&self, id: &str) -> Result<&Registration<M>> {
        self.entries
            .iter()
            .find(|entry| entry.format.id == id)
            .ok_or_else(|| Error::UnknownFormat(id.into()))
    }

    /// Install a new format or replace an empty planned entry. Duplicate live
    /// registrations and hooks for a disabled feature are rejected.
    pub fn register(&mut self, registration: Registration<M>) -> Result<()> {
        let id = registration.format.id;
        if id.is_empty()
            || !id
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        {
            return Err(Error::InvalidInput(
                "format IDs must use lowercase ASCII letters, digits, and hyphens".into(),
            ));
        }
        if !registration.format.feature_enabled {
            return Err(Error::FeatureDisabled { format: id.into() });
        }
        if let Some(existing) = self.entries.iter_mut().find(|entry| entry.format.id == id) {
            if existing.detector.is_some() || existing.reader.is_some() || existing.writer.is_some()
            {
                return Err(Error::InvalidInput(format!(
                    "format '{id}' is already registered"
                )));
            }
            if existing.format != registration.format {
                return Err(Error::InvalidInput(format!(
                    "registration changes catalogue metadata for '{id}'"
                )));
            }
            *existing = registration;
        } else {
            self.entries.push(registration);
        }
        Ok(())
    }

    /// Longest-suffix filename hints. Collisions remain visible, never pick the
    /// first matching `.fnt`, `.font`, `.ttf`, etc. from catalogue order.
    pub fn extension_candidates(&self, name: &str) -> Vec<FormatInfo> {
        let name = name.to_ascii_lowercase();
        let mut longest = 0;
        let mut candidates = Vec::new();
        for entry in &self.entries {
            let length = entry
                .format
                .extensions
                .iter()
                .filter(|ext| name.ends_with(&format!(".{ext}")))
                .map(|ext| ext.len())
                .max()
                .unwrap_or(0);
            if length > longest {
                longest = length;
                candidates.clear();
            }
            if length > 0 && length == longest {
                candidates.push(entry.format);
            }
        }
        candidates
    }

    /// Infer a unique filename hint, preserving ambiguity as an error.
    pub fn format_from_name(&self, name: &str) -> Result<FormatInfo> {
        match self.extension_candidates(name).as_slice() {
            [] => Err(Error::UnrecognizedFormat),
            [format] => Ok(*format),
            many => Err(Error::AmbiguousFormat {
                candidates: many.iter().map(|format| format.id.into()).collect(),
            }),
        }
    }

    /// Run content detectors first, falling back to filename hints only when
    /// no detector matches. Catalogue hints do not imply a reader exists.
    pub fn detect(&self, input: &Input<'_>, limits: &ResourceLimits) -> Result<Detected> {
        self.detect_context(&mut ReadContext::new(input, limits)?)
    }

    fn detect_context(&self, context: &mut ReadContext<'_>) -> Result<Detected> {
        if context.is_directory() {
            return Err(Error::InvalidInput(
                "directory font input requires an explicit format".into(),
            ));
        }
        let mut best = Confidence::NoMatch;
        let mut candidates = Vec::new();
        for entry in &self.entries {
            if let Some(detector) = &entry.detector {
                let confidence = detector.detect(context)?;
                if confidence == Confidence::Extension {
                    return Err(Error::InvalidInput(format!(
                        "detector '{}' returned filename-only evidence",
                        entry.format.id
                    )));
                }
                if confidence > best {
                    best = confidence;
                    candidates.clear();
                }
                if confidence != Confidence::NoMatch && confidence == best {
                    candidates.push(entry.format);
                }
            }
        }
        if candidates.is_empty() {
            return context
                .name()
                .ok_or(Error::UnrecognizedFormat)
                .and_then(|name| self.format_from_name(name))
                .map(|format| Detected {
                    format,
                    confidence: Confidence::Extension,
                });
        }
        match candidates.as_slice() {
            [format] => Ok(Detected {
                format: *format,
                confidence: best,
            }),
            many => Err(Error::AmbiguousFormat {
                candidates: many.iter().map(|format| format.id.into()).collect(),
            }),
        }
    }

    /// Fail early if a writer is unavailable, before a CLI opens any output.
    pub fn ensure_writable(&self, id: &str) -> Result<()> {
        let entry = self.entry(id)?;
        if entry.writer.is_none() {
            return Err(Error::NotImplemented {
                format: id.into(),
                operation: Operation::Write,
            });
        }
        Ok(())
    }

    /// Decode through an explicit reader or structural/filename detection.
    pub fn read(&self, input: &Input<'_>, options: &ReadOptions) -> Result<Loaded<M>> {
        let mut context = ReadContext::new(input, &options.limits)?;
        let id = if let Some(id) = &options.format {
            id.as_str()
        } else {
            self.detect_context(&mut context)?.format.id
        };
        let entry = self.entry(id)?;
        let reader = entry.reader.as_ref().ok_or_else(|| Error::NotImplemented {
            format: id.into(),
            operation: Operation::Read,
        })?;
        let (fonts, report) = reader.read(&mut context, options)?;
        if report.has_errors() {
            return Err(Error::ReportedErrors(report));
        }
        Ok(Loaded {
            fonts,
            report,
            format: entry.format.id,
        })
    }

    /// Encode with no preceding read diagnostics.
    pub fn write(&self, fonts: &M, id: &str, options: &WriteOptions) -> Result<Written> {
        self.write_with_report(fonts, id, options, Report::default())
    }

    /// Combine earlier read diagnostics with write diagnostics before checking
    /// strict mode, so loss during decoding cannot be silently overlooked.
    pub fn write_with_report(
        &self,
        fonts: &M,
        id: &str,
        options: &WriteOptions,
        mut report: Report,
    ) -> Result<Written> {
        self.ensure_writable(id)?;
        if report.has_errors() {
            return Err(Error::ReportedErrors(report));
        }
        if options.strict && report.has_loss() {
            return Err(Error::StrictLoss(report));
        }
        let writer = self
            .entry(id)?
            .writer
            .as_ref()
            .ok_or_else(|| Error::NotImplemented {
                format: id.into(),
                operation: Operation::Write,
            })?;
        let written = match writer.write(fonts, options) {
            Ok(written) => written,
            Err(Error::ReportedErrors(writer_report)) => {
                report.extend(writer_report);
                return Err(Error::ReportedErrors(report));
            }
            Err(Error::StrictLoss(writer_report)) => {
                report.extend(writer_report);
                return Err(Error::StrictLoss(report));
            }
            Err(error) => return Err(error),
        };
        let writer_report_start = report.entries.len();
        report.extend(written.report);
        if report.has_errors() {
            return Err(Error::ReportedErrors(report));
        }
        if options.strict && report.has_loss() {
            return Err(Error::StrictLoss(report));
        }
        let output = if crate::resources::uses_gzip(&written.output) {
            let mut limits = writer.transport_limits();
            if let Some(common) = &options.transport_limits {
                limits = limits.intersect(common);
            }
            let retained_report =
                crate::resources::report_bytes(&report, limits.resources.max_total_bytes)?;
            let output =
                crate::resources::encode_transport(written.output, &limits, retained_report)?;
            let retained = output
                .assets()
                .iter()
                .try_fold(retained_report, |total, asset| {
                    total
                        .checked_add(asset.bytes.capacity())
                        .ok_or(Error::ResourceLimit {
                            resource: "gzip retained bytes",
                            limit: limits.resources.max_total_bytes,
                        })
                })?;
            if let Err(error) =
                writer.validate_transport(fonts, options, &limits.without_retained(retained)?)
            {
                return Err(match error {
                    Error::ReportedErrors(extra) => Error::ReportedErrors(
                        transport::merge_validation_report(report, extra, writer_report_start),
                    ),
                    Error::StrictLoss(extra) => Error::StrictLoss(
                        transport::merge_validation_report(report, extra, writer_report_start),
                    ),
                    other => other,
                });
            }
            output
        } else {
            if let Some(common) = &options.transport_limits {
                let maximum = common.max_output_bytes;
                written.output.assets().iter().try_fold(0usize, |total, asset| {
                    total
                        .checked_add(asset.bytes.len())
                        .filter(|bytes| *bytes <= maximum)
                        .ok_or(Error::ResourceLimit {
                            resource: "plain output asset bytes",
                            limit: maximum,
                        })
                })?;
            }
            written.output
        };
        Ok(Written { output, report })
    }
}

#[cfg(feature = "bdf")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_bdf(&mut self) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == "bdf") {
            entry.detector = Some(Box::new(bdf::BdfCodec::default()));
            entry.reader = Some(Box::new(bdf::BdfCodec::default()));
            entry.writer = Some(transport::writer(bdf::BdfCodec::default()));
        }
    }
}

#[cfg(feature = "pcf")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_pcf(&mut self) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == "pcf") {
            entry.detector = Some(Box::new(pcf::PcfCodec::default()));
            entry.reader = Some(Box::new(pcf::PcfCodec::default()));
            entry.writer = Some(transport::writer(pcf::PcfCodec::default()));
        }
    }
    /// Configure the built-in PCF writer without altering its detector/reader.
    pub fn set_pcf_write_options(&mut self, options: pcf::PcfWriteOptions) -> Result<()> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "pcf" && e.writer.is_some())
            .ok_or_else(|| Error::NotImplemented {
                format: "pcf".into(),
                operation: Operation::Write,
            })?;
        entry.writer = Some(transport::writer(pcf::PcfCodec {
            write_options: options,
        }));
        Ok(())
    }
}

#[cfg(feature = "yaff")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_yaff(&mut self) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == "yaff") {
            entry.detector = Some(Box::new(yaff::YaffCodec::default()));
            entry.reader = Some(Box::new(yaff::YaffCodec::default()));
            entry.writer = Some(transport::writer(yaff::YaffCodec::default()));
        }
    }
    /// Configure bounded YAFF writes independently of other codecs.
    pub fn set_yaff_write_options(&mut self, options: yaff::YaffWriteOptions) -> Result<()> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "yaff" && e.writer.is_some())
            .ok_or_else(|| Error::NotImplemented {
                format: "yaff".into(),
                operation: Operation::Write,
            })?;
        entry.writer = Some(transport::writer(yaff::YaffCodec {
            write_options: options,
        }));
        Ok(())
    }
}

#[cfg(any(feature = "otb", feature = "bdat", feature = "color"))]
impl Registry<crate::FontSet> {
    pub(crate) fn install_embedded(
        &mut self,
        id: &'static str,
        kind: crate::sfnt::bitmap::Kind,
        write: bool,
    ) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == id) {
            entry.detector = Some(Box::new(embedded::EmbeddedCodec::new(kind)));
            entry.reader = Some(Box::new(embedded::EmbeddedCodec::new(kind)));
            if write {
                entry.writer = Some(transport::writer(embedded::EmbeddedCodec::new(kind)));
            }
        }
    }
    /// Configure installed OTB/bdat writers without changing readers or color output.
    pub fn set_embedded_write_options(&mut self, options: embedded::EmbeddedWriteOptions) {
        for (id, kind) in [
            ("otb", crate::sfnt::bitmap::Kind::OpenType),
            ("bdat", crate::sfnt::bitmap::Kind::Apple),
        ] {
            if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == id && e.writer.is_some()) {
                let mut codec = embedded::EmbeddedCodec::new(kind);
                codec.write_options = options.clone();
                entry.writer = Some(transport::writer(codec));
            }
        }
    }
}

#[cfg(feature = "vector-raster")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_vector(&mut self, id: &'static str) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.format.id == id) {
            let codec = vector_raster::Codec::new(id);
            if id != "vector-raster" {
                entry.detector = Some(Box::new(codec.clone()));
            }
            entry.reader = Some(Box::new(codec.clone()));
            entry.writer = Some(transport::writer(codec));
        }
    }
    /// Configure all registered outline/container readers consistently.
    pub fn set_vector_read_options(&mut self, options: vector_raster::Options) {
        for id in ["ttf", "otf", "woff", "woff2", "ttc", "vector-raster"] {
            if let Some(entry) = self
                .entries
                .iter_mut()
                .find(|e| e.format.id == id && e.reader.is_some())
            {
                let mut codec = vector_raster::Codec::new(entry.format.id);
                codec.read_options = options.clone();
                entry.reader = Some(Box::new(codec));
            }
        }
    }
    /// Configure all registered outline/container writers consistently.
    pub fn set_vector_write_options(&mut self, options: vector_raster::WriteOptions) {
        for id in ["ttf", "otf", "woff", "woff2", "ttc", "vector-raster"] {
            if let Some(entry) = self
                .entries
                .iter_mut()
                .find(|e| e.format.id == id && e.writer.is_some())
            {
                let mut codec = vector_raster::Codec::new(entry.format.id);
                codec.write_options = options.clone();
                entry.writer = Some(transport::writer(codec));
            }
        }
    }
}

#[cfg(feature = "svg")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_svg(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "svg") {
            e.detector = Some(Box::new(svg::SvgCodec::default()));
            e.reader = Some(Box::new(svg::SvgCodec::default()));
            e.writer = Some(transport::writer(svg::SvgCodec::default()));
        }
    }
    /// Configure the SVG font reader/detector independently of its writer.
    pub fn set_svg_read_options(&mut self, options: svg::ReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "svg" && e.reader.is_some())
        {
            e.detector = Some(Box::new(svg::SvgCodec {
                read_options: options.clone(),
                ..Default::default()
            }));
            e.reader = Some(Box::new(svg::SvgCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Configure deterministic SVG font output independently of its reader.
    pub fn set_svg_write_options(&mut self, options: svg::WriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "svg" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(svg::SvgCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "hex")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_hex(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "hex") {
            e.detector = Some(Box::new(hex::HexCodec::default()));
            e.reader = Some(Box::new(hex::HexCodec::default()));
            e.writer = Some(transport::writer(hex::HexCodec::default()));
        }
    }
    /// Set explicit HEX input geometry without changing output choices.
    pub fn set_hex_read_options(&mut self, options: hex::HexReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "hex" && e.reader.is_some())
        {
            e.reader = Some(Box::new(hex::HexCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Configure bounded deterministic HEX output.
    pub fn set_hex_write_options(&mut self, options: hex::HexWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "hex" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(hex::HexCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}
#[cfg(feature = "psf")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_psf(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "psf") {
            e.detector = Some(Box::new(psf::PsfCodec::default()));
            e.reader = Some(Box::new(psf::PsfCodec::default()));
            e.writer = Some(transport::writer(psf::PsfCodec::default()));
        }
    }
    /// Configure bounded PSF1/PSF2 output and Unicode-table policy.
    pub fn set_psf_write_options(&mut self, options: psf::PsfWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "psf" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(psf::PsfCodec {
                write_options: options,
            }));
        }
    }
}

#[cfg(feature = "image")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_image(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "image") {
            e.detector = Some(Box::new(image::ImageCodec::default()));
            e.reader = Some(Box::new(image::ImageCodec::default()));
            e.writer = Some(transport::writer(image::ImageCodec::default()));
        }
    }
    /// Select an explicit image mapping or caller-scoped mapping asset.
    pub fn set_image_read_options(&mut self, options: image::ImageReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "image" && e.reader.is_some())
        {
            e.reader = Some(Box::new(image::ImageCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Configure atlas image container, packing and mapping serialization.
    pub fn set_image_write_options(&mut self, options: image::ImageWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "image" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(image::ImageCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "fnt")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_fnt(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "fnt") {
            e.detector = Some(Box::new(fnt::FntCodec::default()));
            e.reader = Some(Box::new(fnt::FntCodec::default()));
            e.writer = Some(transport::writer(fnt::FntCodec::default()));
        }
    }
    /// Set legacy charset interpretation on FNT and enabled FON readers.
    pub fn set_fnt_read_options(&mut self, options: fnt::FntReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "fnt" && e.reader.is_some())
        {
            e.reader = Some(Box::new(fnt::FntCodec {
                read_options: options.clone(),
                ..Default::default()
            }));
        }
        #[cfg(feature = "fon")]
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "fon" && e.reader.is_some())
        {
            e.reader = Some(Box::new(fon::FonCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Set version, encoding and limits on FNT and enabled FON writers.
    pub fn set_fnt_write_options(&mut self, options: fnt::FntWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "fnt" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(fnt::FntCodec {
                write_options: options.clone(),
                ..Default::default()
            }));
        }
        #[cfg(feature = "fon")]
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "fon" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(fon::FonCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}
#[cfg(feature = "fon")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_fon(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "fon") {
            e.detector = Some(Box::new(fon::FonCodec::default()));
            e.reader = Some(Box::new(fon::FonCodec::default()));
            e.writer = Some(transport::writer(fon::FonCodec::default()));
        }
    }
}

#[cfg(feature = "c")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_c(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "c") {
            e.detector = Some(Box::new(c::CCodec::default()));
            e.reader = Some(Box::new(c::CCodec::default()));
            e.writer = Some(transport::writer(c::CCodec::default()));
        }
    }
    /// Configure literal C input dialect and explicit geometry.
    pub fn set_c_read_options(&mut self, options: c::CReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "c" && e.reader.is_some())
        {
            e.reader = Some(Box::new(c::CCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Configure portable generic or Adafruit GFX output.
    pub fn set_c_write_options(&mut self, options: c::CWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "c" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(c::CCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "kbitx")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_kbitx(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "kbitx") {
            e.detector = Some(Box::new(kbitx::KbitxCodec::default()));
            e.reader = Some(Box::new(kbitx::KbitxCodec::default()));
            e.writer = Some(transport::writer(kbitx::KbitxCodec::default()));
        }
    }
    /// Configure Kbitx input interpretation.
    pub fn set_kbitx_read_options(&mut self, options: kbitx::KbitxReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "kbitx" && e.reader.is_some())
        {
            e.reader = Some(Box::new(kbitx::KbitxCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Configure Kbitx output and resource limits.
    pub fn set_kbitx_write_options(&mut self, options: kbitx::KbitxWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "kbitx" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(kbitx::KbitxCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "pbf")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_pbf(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "pbf") {
            e.detector = Some(Box::new(pbf::PbfCodec::default()));
            e.reader = Some(Box::new(pbf::PbfCodec::default()));
            e.writer = Some(transport::writer(pbf::PbfCodec::default()));
        }
    }
    /// Configure Pbf input interpretation.
    pub fn set_pbf_read_options(&mut self, options: pbf::PbfReadOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "pbf" && e.reader.is_some())
        {
            e.reader = Some(Box::new(pbf::PbfCodec {
                read_options: options,
                ..Default::default()
            }));
        }
    }
    /// Configure Pbf output and resource limits.
    pub fn set_pbf_write_options(&mut self, options: pbf::PbfWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "pbf" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(pbf::PbfCodec {
                write_options: options,
                ..Default::default()
            }));
        }
    }
}

#[cfg(feature = "hbf")]
impl Registry<crate::FontSet> {
    pub(crate) fn install_hbf(&mut self) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.format.id == "hbf") {
            e.detector = Some(Box::new(hbf::HbfCodec::default()));
            e.reader = Some(Box::new(hbf::HbfCodec::default()));
            e.writer = Some(transport::writer(hbf::HbfCodec::default()));
        }
    }
    /// Configure Hbf output and resource limits.
    pub fn set_hbf_write_options(&mut self, options: hbf::HbfWriteOptions) {
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.format.id == "hbf" && e.writer.is_some())
        {
            e.writer = Some(transport::writer(hbf::HbfCodec {
                write_options: options,
            }));
        }
    }
}

#[cfg(feature = "dosstart")]
pub mod dosstart;
#[cfg(feature = "drhalo")]
pub mod drhalo;
#[cfg(feature = "edwin")]
pub mod edwin;
#[cfg(feature = "mkwinfont")]
pub mod mkwinfont;
#[cfg(feature = "pcr")]
pub mod pcr;
#[cfg(feature = "plus3dos")]
pub mod plus3dos;
#[cfg(feature = "psf2txt")]
pub mod psf2txt;
#[cfg(any(
    feature = "dosstart",
    feature = "edwin",
    feature = "mkwinfont",
    feature = "psf2txt",
    feature = "pcr",
    feature = "plus3dos",
    feature = "drhalo",
    feature = "writeon"
))]
#[path = "dosstart/common.rs"]
pub(crate) mod w5a1_common;
#[cfg(feature = "writeon")]
pub mod writeon;

#[cfg(any(
    feature = "daisy",
    feature = "polyprint",
    feature = "printshop",
    feature = "tasprint",
    feature = "pxl",
    feature = "signum"
))]
#[path = "daisy/common.rs"]
pub(crate) mod historical_common;

#[cfg(any(
    feature = "rockbox",
    feature = "u8m",
    feature = "cybiko",
    feature = "playdate",
    feature = "hmzk",
    feature = "sbf"
))]
#[path = "rockbox/common.rs"]
pub(crate) mod device_common;

#[cfg(feature = "polyprint")]
pub mod polyprint;

#[cfg(feature = "printshop")]
pub mod printshop;

#[cfg(feature = "hmzk")]
pub mod hmzk;

#[cfg(feature = "sbf")]
pub mod sbf;

#[cfg(feature = "cybiko")]
pub mod cybiko;

#[cfg(feature = "signum")]
pub mod signum;

#[cfg(feature = "daisy")]
pub mod daisy;

#[cfg(feature = "tasprint")]
pub mod tasprint;

#[cfg(feature = "rockbox")]
pub mod rockbox;

#[cfg(feature = "u8m")]
pub mod u8m;

#[cfg(feature = "pxl")]
pub mod pxl;

#[cfg(feature = "mgtk")]
pub mod mgtk;

#[cfg(any(
    feature = "fbit",
    feature = "lisa",
    feature = "mgtk",
    feature = "qpf",
    feature = "qpf2"
))]
#[path = "fbit/common.rs"]
pub(crate) mod w5d_common;

#[cfg(feature = "playdate")]
pub mod playdate;

#[cfg(feature = "qpf")]
pub mod qpf;

#[cfg(feature = "symbos")]
pub mod symbos;

#[cfg(feature = "bbc")]
pub mod bbc;

#[cfg(feature = "wyse")]
pub mod wyse;

#[cfg(feature = "grasp")]
pub mod grasp;

#[cfg(feature = "hrcg")]
pub mod hrcg;

#[cfg(any(
    feature = "bbc",
    feature = "comfont",
    feature = "consoleet",
    feature = "grasp",
    feature = "hrcg",
    feature = "wyse",
    feature = "xbin",
    feature = "zapfont"
))]
#[path = "bbc/common.rs"]
pub(crate) mod w5a2_common;

#[cfg(feature = "qpf2")]
pub mod qpf2;

#[cfg(feature = "xbin")]
pub mod xbin;

#[cfg(feature = "consoleet")]
pub mod consoleet;

#[cfg(feature = "oberon")]
pub mod oberon;

#[cfg(feature = "fbit")]
pub mod fbit;

#[cfg(feature = "zapfont")]
pub mod zapfont;

#[cfg(feature = "lisa")]
pub mod lisa;

#[cfg(feature = "wsfont")]
pub mod wsfont;

#[cfg(feature = "beos")]
pub mod beos;

#[cfg(feature = "comfont")]
pub mod comfont;

#[cfg(feature = "catalyst")]
pub mod catalyst;
#[cfg(any(
    feature = "catalyst",
    feature = "chiwriter",
    feature = "dashen",
    feature = "hppcl",
    feature = "prebuilt",
    feature = "riscos",
    feature = "xerox"
))]
#[path = "catalyst/common.rs"]
pub(crate) mod workstation_common;

#[cfg(feature = "chiwriter")]
pub mod chiwriter;

#[cfg(feature = "vtfont")]
pub mod vtfont;

#[cfg(feature = "hppcl")]
pub mod hppcl;

#[cfg(feature = "dashen")]
pub mod dashen;

#[cfg(any(
    feature = "pilfont",
    feature = "sfont",
    feature = "vlw",
    feature = "image"
))]
#[path = "pilfont/common.rs"]
pub(crate) mod image_bundle_common;

#[cfg(feature = "vlw")]
pub mod vlw;

#[cfg(feature = "pilfont")]
pub mod pilfont;

#[cfg(feature = "prebuilt")]
pub mod prebuilt;

#[cfg(feature = "geos")]
pub mod geos;

#[cfg(feature = "gpi")]
pub mod gpi;

#[cfg(feature = "riscos")]
pub mod riscos;
#[cfg(any(feature = "ssfn", feature = "pfr"))]
#[path = "ssfn/common.rs"]
pub(crate) mod w5e2_outline_common;

#[cfg(feature = "sfont")]
pub mod sfont;

#[cfg(feature = "pfr")]
pub mod pfr;

#[cfg(feature = "pcgeos")]
pub mod pcgeos;

#[cfg(feature = "xerox")]
pub mod xerox;

#[cfg(feature = "ssfn")]
pub mod ssfn;
