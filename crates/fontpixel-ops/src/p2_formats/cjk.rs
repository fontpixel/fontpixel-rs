//! FONTX2 and explicitly configured HZK/ASC geometry.
use super::{require_feature, require_target};
use crate::options::check_choice;
use fontpixel::{FontRegistry, ResourceLimits, Result};

const READ_OPTIONS_HZK_CODE_SPACE: [&str; 2] = ["gb2312", "ascii"];
const READ_OPTIONS_HZK_STORAGE: [&str; 2] = ["rows", "columns"];
/// Read options: fONTX2 and explicitly configured HZK/ASC geometry.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct ReadOptions {
    /// Exact charset label of FONTX2 source codes.
    #[cfg_attr(feature = "clap", arg(long))]
    pub fontx2_read_charset: Option<String>,
    /// Explicit HZK/ASC preset, e.g. HZK16, HZK24S, ASC16.
    #[cfg_attr(feature = "clap", arg(long))]
    pub hzk_preset: Option<String>,
    /// HZK glyph width; overrides a preset's geometry.
    #[cfg_attr(feature = "clap", arg(long))]
    pub hzk_width: Option<u16>,
    /// HZK glyph height; overrides a preset's geometry.
    #[cfg_attr(feature = "clap", arg(long))]
    pub hzk_height: Option<u16>,
    /// HZK first slot code, e.g. 0xA1A1.
    #[cfg_attr(feature = "clap", arg(long))]
    pub hzk_first: Option<String>,
    /// HZK last permitted slot code, inclusive.
    #[cfg_attr(feature = "clap", arg(long))]
    pub hzk_last: Option<String>,
    /// HZK slot domain for custom geometry.
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_HZK_CODE_SPACE))]
    pub hzk_code_space: Option<String>,
    /// HZK bitmap storage orientation (default rows).
    #[cfg_attr(feature = "clap", arg(long, value_parser = READ_OPTIONS_HZK_STORAGE))]
    pub hzk_storage: Option<String>,
    /// Exact HZK input or output glyph count.
    #[cfg_attr(feature = "clap", arg(long))]
    pub hzk_count: Option<usize>,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(ReadOptions);
impl ReadOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.hzk_code_space.as_deref() {
            check_choice("hzk_code_space", value, &READ_OPTIONS_HZK_CODE_SPACE)?;
        }
        if let Some(value) = self.hzk_storage.as_deref() {
            check_choice("hzk_storage", value, &READ_OPTIONS_HZK_STORAGE)?;
        }
        Ok(())
    }
}
impl ReadOptions {
    fn hzk_requested(&self) -> bool {
        self.hzk_preset.is_some()
            || self.hzk_width.is_some()
            || self.hzk_height.is_some()
            || self.hzk_first.is_some()
            || self.hzk_last.is_some()
            || self.hzk_code_space.is_some()
            || self.hzk_storage.is_some()
            || self.hzk_count.is_some()
    }
    #[cfg(feature = "hzk")]
    fn hzk_options(&self) -> Result<fontpixel::formats::hzk::HzkReadOptions> {
        use fontpixel::formats::hzk::{CodeSpace, Geometry, HzkReadOptions, Preset, Storage};
        let preset = self
            .hzk_preset
            .as_deref()
            .map(str::parse::<Preset>)
            .transpose()?;
        let explicit = self.hzk_width.is_some()
            || self.hzk_height.is_some()
            || self.hzk_first.is_some()
            || self.hzk_last.is_some()
            || self.hzk_code_space.is_some()
            || self.hzk_storage.is_some();
        let code = |s: &str| -> Result<u16> {
            u16::try_from(crate::options::parse_code(s)?)
                .map_err(|_| fontpixel::Error::InvalidInput("HZK slot code exceeds u16".into()))
        };
        let geometry = if explicit {
            let mut g = if let Some(preset) = preset {
                preset.geometry()
            } else {
                let missing = || {
                    fontpixel::Error::InvalidInput(
                    "Custom HZK geometry needs width, height, first, last and code-space, or a preset".into())
                };
                Geometry {
                    width: self.hzk_width.ok_or_else(missing)?,
                    height: self.hzk_height.ok_or_else(missing)?,
                    first_code: code(self.hzk_first.as_deref().ok_or_else(missing)?)?,
                    last_code: code(self.hzk_last.as_deref().ok_or_else(missing)?)?,
                    code_space: if self.hzk_code_space.as_deref().ok_or_else(missing)? == "ascii" {
                        CodeSpace::Ascii
                    } else {
                        CodeSpace::Gb2312
                    },
                    storage: Storage::Rows,
                }
            };
            if let Some(v) = self.hzk_width {
                g.width = v;
            }
            if let Some(v) = self.hzk_height {
                g.height = v;
            }
            if let Some(v) = &self.hzk_first {
                g.first_code = code(v)?;
            }
            if let Some(v) = &self.hzk_last {
                g.last_code = code(v)?;
            }
            if let Some(v) = &self.hzk_code_space {
                g.code_space = if v == "ascii" {
                    CodeSpace::Ascii
                } else {
                    CodeSpace::Gb2312
                };
            }
            if let Some(v) = &self.hzk_storage {
                g.storage = if v == "columns" {
                    Storage::Columns
                } else {
                    Storage::Rows
                };
            }
            Some(g)
        } else {
            None
        };
        Ok(HzkReadOptions {
            preset,
            geometry,
            glyph_count: self.hzk_count,
        })
    }
    pub(crate) fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        require_feature(
            self.fontx2_read_charset.is_some(),
            cfg!(feature = "fontx2"),
            "fontx2",
        )?;
        require_feature(self.hzk_requested(), cfg!(feature = "hzk"), "hzk")?;
        #[cfg(feature = "fontx2")]
        registry.set_fontx2_read_options(fontpixel::formats::fontx2::Fontx2ReadOptions {
            charset: self.fontx2_read_charset.clone(),
        });
        #[cfg(feature = "hzk")]
        registry.set_hzk_read_options(self.hzk_options()?);
        let _ = registry;
        Ok(())
    }
}

const WRITE_OPTIONS_FONTX2_MODE: [&str; 3] = ["auto", "single", "double"];
/// Write options: fONTX2 and explicitly configured HZK/ASC geometry.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct WriteOptions {
    /// FONTX2 single-byte ANK or double-byte block tables (default auto).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_FONTX2_MODE))]
    pub fontx2_mode: Option<String>,
    /// Charset used for exact FONTX2 output mapping.
    #[cfg_attr(feature = "clap", arg(long))]
    pub fontx2_charset: Option<String>,
    /// FONTX2 output cell width.
    #[cfg_attr(feature = "clap", arg(long))]
    pub fontx2_width: Option<u8>,
    /// FONTX2 output cell height.
    #[cfg_attr(feature = "clap", arg(long))]
    pub fontx2_height: Option<u8>,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(WriteOptions);
impl WriteOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.fontx2_mode.as_deref() {
            check_choice("fontx2_mode", value, &WRITE_OPTIONS_FONTX2_MODE)?;
        }
        Ok(())
    }
}
impl WriteOptions {
    pub(crate) fn apply(
        &self,
        id: &str,
        input: &ReadOptions,
        limits: &ResourceLimits,
        threshold: Option<u8>,
        registry: &mut FontRegistry,
    ) -> Result<()> {
        let fontx2_requested = self.fontx2_mode.is_some()
            || self.fontx2_charset.is_some()
            || self.fontx2_width.is_some()
            || self.fontx2_height.is_some();
        require_target(fontx2_requested, id, "fontx2")?;
        require_feature(
            id == "fontx2" || fontx2_requested,
            cfg!(feature = "fontx2"),
            "fontx2",
        )?;
        require_feature(id == "hzk", cfg!(feature = "hzk"), "hzk")?;
        require_feature(id == "pf2", cfg!(feature = "pf2"), "pf2")?;
        #[cfg(feature = "fontx2")]
        if id == "fontx2" {
            use fontpixel::formats::fontx2::{CodeMode, Fontx2WriteOptions};
            registry.set_fontx2_write_options(Fontx2WriteOptions {
                mode: match self.fontx2_mode.as_deref() {
                    Some("single") => CodeMode::SingleByte,
                    Some("double") => CodeMode::DoubleByte,
                    _ => CodeMode::Auto,
                },
                charset: self.fontx2_charset.clone(),
                width: self.fontx2_width,
                height: self.fontx2_height,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "hzk")]
        if id == "hzk" {
            let o = input.hzk_options()?;
            registry.set_hzk_write_options(fontpixel::formats::hzk::HzkWriteOptions {
                preset: o.preset,
                geometry: o.geometry,
                glyph_count: o.glyph_count,
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        #[cfg(feature = "pf2")]
        if id == "pf2" {
            registry.set_pf2_write_options(fontpixel::formats::pf2::Pf2WriteOptions {
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        let _ = (input, limits, threshold, registry);
        Ok(())
    }
}
