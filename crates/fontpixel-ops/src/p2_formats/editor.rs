//! BMFont descriptor/page choices and native editor formats.
use super::{require_feature, require_target};
use crate::options::check_choice;
use fontpixel::{FontRegistry, ResourceLimits, Result};

/// Read options: bMFont descriptor/page choices and native editor formats.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct ReadOptions {
    /// Explicitly threshold imported BMFont grayscale; default preserves samples.
    #[cfg_attr(feature = "clap", arg(long))]
    pub bmfont_read_threshold: Option<u8>,
}
impl ReadOptions {
    /// Check the value constraints the command-line parser enforces; this
    /// group has none beyond its field types.
    pub fn validate(&self) -> fontpixel::Result<()> {
        Ok(())
    }
}
impl ReadOptions {
    pub(crate) fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        require_feature(
            self.bmfont_read_threshold.is_some(),
            cfg!(feature = "bmfont"),
            "bmfont",
        )?;
        #[cfg(feature = "bmfont")]
        registry.set_bmfont_read_options(fontpixel::formats::bmfont::BmfontReadOptions {
            gray_threshold: self.bmfont_read_threshold,
        });
        let _ = registry;
        Ok(())
    }
}

const WRITE_OPTIONS_BMFONT_ENCODING: [&str; 4] = ["text", "xml", "binary", "json"];
/// Write options: bMFont descriptor/page choices and native editor formats.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct WriteOptions {
    /// BMFont descriptor encoding (default text).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_BMFONT_ENCODING))]
    pub bmfont_encoding: Option<String>,
    /// BMFont PNG page width in pixels.
    #[cfg_attr(feature = "clap", arg(long))]
    pub bmfont_page_width: Option<u32>,
    /// BMFont PNG page height in pixels.
    #[cfg_attr(feature = "clap", arg(long))]
    pub bmfont_page_height: Option<u32>,
    /// Empty pixels between BMFont glyphs.
    #[cfg_attr(feature = "clap", arg(long))]
    pub bmfont_padding: Option<u32>,
}
impl WriteOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.bmfont_encoding.as_deref() {
            check_choice("bmfont_encoding", value, &WRITE_OPTIONS_BMFONT_ENCODING)?;
        }
        Ok(())
    }
}
impl WriteOptions {
    pub(crate) fn apply(
        &self,
        id: &str,
        limits: &ResourceLimits,
        registry: &mut FontRegistry,
    ) -> Result<()> {
        let requested = self.bmfont_encoding.is_some()
            || self.bmfont_page_width.is_some()
            || self.bmfont_page_height.is_some()
            || self.bmfont_padding.is_some();
        require_target(requested, id, "bmfont")?;
        require_feature(
            requested || id == "bmfont",
            cfg!(feature = "bmfont"),
            "bmfont",
        )?;
        require_feature(id == "sfd", cfg!(feature = "sfd"), "sfd")?;
        require_feature(id == "kbits", cfg!(feature = "kbits"), "kbits")?;
        #[cfg(feature = "bmfont")]
        if id == "bmfont" {
            use fontpixel::formats::bmfont::{BmfontEncoding, BmfontWriteOptions};
            let mut options = BmfontWriteOptions {
                limits: limits.clone(),
                ..Default::default()
            };
            if let Some(v) = &self.bmfont_encoding {
                options.encoding = match v.as_str() {
                    "xml" => BmfontEncoding::Xml,
                    "binary" => BmfontEncoding::Binary,
                    "json" => BmfontEncoding::Json,
                    _ => BmfontEncoding::Text,
                };
            }
            if let Some(v) = self.bmfont_page_width {
                options.page_width = v;
            }
            if let Some(v) = self.bmfont_page_height {
                options.page_height = v;
            }
            if let Some(v) = self.bmfont_padding {
                options.padding = v;
            }
            registry.set_bmfont_write_options(options);
        }
        #[cfg(feature = "sfd")]
        if id == "sfd" {
            registry.set_sfd_write_options(fontpixel::formats::sfd::SfdWriteOptions {
                limits: limits.clone(),
            });
        }
        #[cfg(feature = "kbits")]
        if id == "kbits" {
            registry.set_kbits_write_options(fontpixel::formats::kbits::KbitsWriteOptions {
                limits: limits.clone(),
            });
        }
        let _ = (limits, registry);
        Ok(())
    }
}
