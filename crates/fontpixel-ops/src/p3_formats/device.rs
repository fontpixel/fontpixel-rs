//! Explicit device font containers, bitmap storage and related input metadata.
use super::{feature, target};
use crate::options::{check_choice, check_range};
use fontpixel::{FontRegistry, ResourceLimits, Result};

/// Read options: explicit device font containers, bitmap storage and related input metadata.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct ReadOptions {
    /// Scoped PNG mask for a PILfont descriptor (its native suffix is .pbm).
    #[cfg_attr(feature = "clap", arg(long))]
    pub pilfont_read_atlas: Option<String>,
    /// Scoped PNG atlas for a Playdate FNT descriptor.
    #[cfg_attr(feature = "clap", arg(long))]
    pub playdate_read_atlas: Option<String>,
    /// Playdate source atlas cell width.
    #[cfg_attr(feature = "clap", arg(long, requires="playdate_read_cell_height", value_parser=clap::value_parser!(u32).range(1..)))]
    pub playdate_read_cell_width: Option<u32>,
    /// Playdate source atlas cell height.
    #[cfg_attr(feature = "clap", arg(long, requires="playdate_read_cell_width", value_parser=clap::value_parser!(u32).range(1..)))]
    pub playdate_read_cell_height: Option<u32>,
    /// Charset label for MGTK legacy positions; no implicit Unicode remapping.
    #[cfg_attr(feature = "clap", arg(long))]
    pub mgtk_read_charset: Option<String>,
    /// Family name supplied externally for a QPF font.
    #[cfg_attr(feature = "clap", arg(long))]
    pub qpf_read_family: Option<String>,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(ReadOptions);
impl ReadOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.playdate_read_cell_width {
            check_range(
                "playdate_read_cell_width",
                i128::from(value),
                1,
                i128::from(u32::MAX),
            )?;
        }
        if let Some(value) = self.playdate_read_cell_height {
            check_range(
                "playdate_read_cell_height",
                i128::from(value),
                1,
                i128::from(u32::MAX),
            )?;
        }
        crate::options::check_requires(
            self.playdate_read_cell_width.is_some(),
            &[(
                self.playdate_read_cell_height.is_some(),
                "--playdate-read-cell-height <PLAYDATE_READ_CELL_HEIGHT>",
            )],
        )?;
        crate::options::check_requires(
            self.playdate_read_cell_height.is_some(),
            &[(
                self.playdate_read_cell_width.is_some(),
                "--playdate-read-cell-width <PLAYDATE_READ_CELL_WIDTH>",
            )],
        )?;
        Ok(())
    }
}
impl ReadOptions {
    pub(crate) fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        feature(
            self.pilfont_read_atlas.is_some(),
            cfg!(feature = "pilfont"),
            "pilfont",
        )?;
        #[cfg(feature = "pilfont")]
        registry.set_pilfont_read_options(fontpixel::formats::pilfont::PilfontReadOptions {
            atlas: self
                .pilfont_read_atlas
                .as_deref()
                .map(fontpixel::ResourcePath::new)
                .transpose()?,
        });
        feature(
            self.playdate_read_atlas.is_some()
                || self.playdate_read_cell_width.is_some()
                || self.playdate_read_cell_height.is_some(),
            cfg!(feature = "playdate"),
            "playdate",
        )?;
        feature(
            self.mgtk_read_charset.is_some(),
            cfg!(feature = "mgtk"),
            "mgtk",
        )?;
        feature(self.qpf_read_family.is_some(), cfg!(feature = "qpf"), "qpf")?;
        #[cfg(feature = "playdate")]
        registry.set_playdate_read_options(fontpixel::formats::playdate::PlaydateReadOptions {
            atlas: self
                .playdate_read_atlas
                .as_deref()
                .map(fontpixel::ResourcePath::new)
                .transpose()?,
            cell_width: self.playdate_read_cell_width,
            cell_height: self.playdate_read_cell_height,
        });
        #[cfg(feature = "mgtk")]
        registry.set_mgtk_read_options(fontpixel::formats::mgtk::MgtkReadOptions {
            charset: self.mgtk_read_charset.clone(),
        });
        #[cfg(feature = "qpf")]
        registry.set_qpf_read_options(fontpixel::formats::qpf::QpfReadOptions {
            family: self.qpf_read_family.clone(),
        });
        let _ = registry;
        Ok(())
    }
}

const WRITE_OPTIONS_VLW_VERSION: [&str; 4] = ["8", "9", "10", "11"];
const WRITE_OPTIONS_ROCKBOX_FLAVOR: [&str; 2] = ["rb11", "rb12"];
const WRITE_OPTIONS_ROCKBOX_DEPTH: [&str; 2] = ["mono", "gray4"];
const WRITE_OPTIONS_PLAYDATE_FLAVOR: [&str; 3] = ["fnt-embedded", "fnt-png", "pft"];
const WRITE_OPTIONS_QPF2_GLYPH_FORMAT: [&str; 2] = ["gray8", "mono"];
const WRITE_OPTIONS_MGTK_COLUMNS: [&str; 3] = ["auto", "single", "double"];
/// Write options: explicit device font containers, bitmap storage and related input metadata.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct WriteOptions {
    /// PILfont maximum atlas width (default 800).
    #[cfg_attr(feature = "clap", arg(long, value_parser=clap::value_parser!(u32).range(1..=32767)))]
    pub pilfont_max_width: Option<u32>,
    /// Processing VLW generation (default 11).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_VLW_VERSION))]
    pub vlw_version: Option<String>,
    /// Explicit VLW smoothing hint; preserves source hint when omitted.
    #[cfg_attr(feature = "clap", arg(long, action=clap::ArgAction::Set))]
    pub vlw_smooth: Option<bool>,
    /// Rockbox file generation (default rb12).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_ROCKBOX_FLAVOR))]
    pub rockbox_flavor: Option<String>,
    /// Rockbox native coverage representation.
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_ROCKBOX_DEPTH))]
    pub rockbox_depth: Option<String>,
    /// U8/M load address in decimal.
    #[cfg_attr(feature = "clap", arg(long))]
    pub u8m_load_address: Option<u16>,
    /// Playdate descriptor/atlas or compiled font (default fnt-embedded).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_PLAYDATE_FLAVOR))]
    pub playdate_flavor: Option<String>,
    /// Playdate output atlas cell width.
    #[cfg_attr(feature = "clap", arg(long, requires="playdate_cell_height", value_parser=clap::value_parser!(u32).range(1..)))]
    pub playdate_cell_width: Option<u32>,
    /// Playdate output atlas cell height.
    #[cfg_attr(feature = "clap", arg(long, requires="playdate_cell_width", value_parser=clap::value_parser!(u32).range(1..)))]
    pub playdate_cell_height: Option<u32>,
    /// Explicit Playdate font tracking.
    #[cfg_attr(feature = "clap", arg(long, allow_hyphen_values = true))]
    pub playdate_tracking: Option<i16>,
    /// QPF2 native glyph coverage storage (default gray8).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_QPF2_GLYPH_FORMAT))]
    pub qpf2_glyph_format: Option<String>,
    /// MGTK one/two seven-bit raster strips (default auto).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_MGTK_COLUMNS))]
    pub mgtk_columns: Option<String>,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(WriteOptions);
impl WriteOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.pilfont_max_width {
            check_range("pilfont_max_width", i128::from(value), 1, 32767)?;
        }
        if let Some(value) = self.vlw_version.as_deref() {
            check_choice("vlw_version", value, &WRITE_OPTIONS_VLW_VERSION)?;
        }
        if let Some(value) = self.rockbox_flavor.as_deref() {
            check_choice("rockbox_flavor", value, &WRITE_OPTIONS_ROCKBOX_FLAVOR)?;
        }
        if let Some(value) = self.rockbox_depth.as_deref() {
            check_choice("rockbox_depth", value, &WRITE_OPTIONS_ROCKBOX_DEPTH)?;
        }
        if let Some(value) = self.playdate_flavor.as_deref() {
            check_choice("playdate_flavor", value, &WRITE_OPTIONS_PLAYDATE_FLAVOR)?;
        }
        if let Some(value) = self.playdate_cell_width {
            check_range(
                "playdate_cell_width",
                i128::from(value),
                1,
                i128::from(u32::MAX),
            )?;
        }
        if let Some(value) = self.playdate_cell_height {
            check_range(
                "playdate_cell_height",
                i128::from(value),
                1,
                i128::from(u32::MAX),
            )?;
        }
        if let Some(value) = self.qpf2_glyph_format.as_deref() {
            check_choice("qpf2_glyph_format", value, &WRITE_OPTIONS_QPF2_GLYPH_FORMAT)?;
        }
        if let Some(value) = self.mgtk_columns.as_deref() {
            check_choice("mgtk_columns", value, &WRITE_OPTIONS_MGTK_COLUMNS)?;
        }
        crate::options::check_requires(
            self.playdate_cell_width.is_some(),
            &[(
                self.playdate_cell_height.is_some(),
                "--playdate-cell-height <PLAYDATE_CELL_HEIGHT>",
            )],
        )?;
        crate::options::check_requires(
            self.playdate_cell_height.is_some(),
            &[(
                self.playdate_cell_width.is_some(),
                "--playdate-cell-width <PLAYDATE_CELL_WIDTH>",
            )],
        )?;
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
        target(self.pilfont_max_width.is_some(), id, "pilfont")?;
        #[cfg(feature = "pilfont")]
        if id == "pilfont" {
            registry.set_pilfont_write_options(fontpixel::formats::pilfont::PilfontWriteOptions {
                max_width: self.pilfont_max_width.unwrap_or(800),
                unicode_as_legacy,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        target(
            self.vlw_version.is_some() || self.vlw_smooth.is_some(),
            id,
            "vlw",
        )?;
        #[cfg(feature = "vlw")]
        if id == "vlw" {
            use fontpixel::formats::vlw::{VlwVersion, VlwWriteOptions};
            registry.set_vlw_write_options(VlwWriteOptions {
                version: match self.vlw_version.as_deref() {
                    Some("8") => VlwVersion::V8,
                    Some("9") => VlwVersion::V9,
                    Some("10") => VlwVersion::V10,
                    _ => VlwVersion::V11,
                },
                smooth: self.vlw_smooth,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        target(
            self.rockbox_flavor.is_some() || self.rockbox_depth.is_some(),
            id,
            "rockbox",
        )?;
        target(self.u8m_load_address.is_some(), id, "u8m")?;
        target(
            self.playdate_flavor.is_some()
                || self.playdate_cell_width.is_some()
                || self.playdate_cell_height.is_some()
                || self.playdate_tracking.is_some(),
            id,
            "playdate",
        )?;
        target(self.mgtk_columns.is_some(), id, "mgtk")?;
        target(self.qpf2_glyph_format.is_some(), id, "qpf2")?;
        #[cfg(feature = "rockbox")]
        if id == "rockbox" {
            registry.set_rockbox_write_options(fontpixel::formats::rockbox::RockboxWriteOptions {
                flavor: if self.rockbox_flavor.as_deref() == Some("rb11") {
                    fontpixel::formats::rockbox::RockboxFlavor::Rb11
                } else {
                    fontpixel::formats::rockbox::RockboxFlavor::Rb12
                },
                depth: self.rockbox_depth.as_deref().map(|value| {
                    if value == "gray4" {
                        fontpixel::formats::rockbox::RockboxDepth::Gray4
                    } else {
                        fontpixel::formats::rockbox::RockboxDepth::Mono
                    }
                }),
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "u8m")]
        if id == "u8m" {
            registry.set_u8m_write_options(fontpixel::formats::u8m::U8mWriteOptions {
                load_address: self.u8m_load_address,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "cybiko")]
        if id == "cybiko" {
            registry.set_cybiko_write_options(fontpixel::formats::cybiko::CybikoWriteOptions {
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "sbf")]
        if id == "sbf" {
            registry.set_sbf_write_options(fontpixel::formats::sbf::SbfWriteOptions {
                unicode_as_legacy,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "hmzk")]
        if id == "hmzk" {
            registry.set_hmzk_write_options(fontpixel::formats::hmzk::HmzkWriteOptions {
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "playdate")]
        if id == "playdate" {
            registry.set_playdate_write_options(
                fontpixel::formats::playdate::PlaydateWriteOptions {
                    flavor: match self.playdate_flavor.as_deref() {
                        Some("fnt-png") => fontpixel::formats::playdate::PlaydateFlavor::FntPng,
                        Some("pft") => fontpixel::formats::playdate::PlaydateFlavor::Pft,
                        _ => fontpixel::formats::playdate::PlaydateFlavor::FntEmbedded,
                    },
                    cell_width: self.playdate_cell_width,
                    cell_height: self.playdate_cell_height,
                    tracking: self.playdate_tracking,
                    gray_threshold: threshold,
                    limits: limits.clone(),
                    ..Default::default()
                },
            );
        }
        #[cfg(feature = "mgtk")]
        if id == "mgtk" {
            registry.set_mgtk_write_options(fontpixel::formats::mgtk::MgtkWriteOptions {
                columns: match self.mgtk_columns.as_deref() {
                    Some("single") => fontpixel::formats::mgtk::Columns::Single,
                    Some("double") => fontpixel::formats::mgtk::Columns::Double,
                    _ => fontpixel::formats::mgtk::Columns::Auto,
                },
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "qpf")]
        if id == "qpf" {
            registry.set_qpf_write_options(fontpixel::formats::qpf::QpfWriteOptions {
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "qpf2")]
        if id == "qpf2" {
            registry.set_qpf2_write_options(fontpixel::formats::qpf2::Qpf2WriteOptions {
                glyph_format: if self.qpf2_glyph_format.as_deref() == Some("mono") {
                    fontpixel::formats::qpf2::GlyphFormat::Mono
                } else {
                    fontpixel::formats::qpf2::GlyphFormat::Gray8
                },
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        let _ = (limits, threshold, unicode_as_legacy, registry);
        Ok(())
    }
}
