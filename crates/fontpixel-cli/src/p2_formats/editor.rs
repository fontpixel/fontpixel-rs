//! BMFont descriptor/page choices and native editor formats.
use super::{require_feature, require_target};
use clap::Args;
use fontpixel::{FontRegistry, ResourceLimits, Result};

#[derive(Args, Default)]
#[group(skip)]
pub(super) struct ReadArguments {
    /// Explicitly threshold imported BMFont grayscale; default preserves samples.
    #[arg(long)]
    bmfont_read_threshold: Option<u8>,
}
impl ReadArguments {
    pub fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
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

#[derive(Args, Default)]
#[group(skip)]
pub(super) struct WriteArguments {
    /// BMFont descriptor encoding (default text).
    #[arg(long, value_parser = ["text", "xml", "binary"])]
    bmfont_encoding: Option<String>,
    /// BMFont PNG page width in pixels.
    #[arg(long)]
    bmfont_page_width: Option<u32>,
    /// BMFont PNG page height in pixels.
    #[arg(long)]
    bmfont_page_height: Option<u32>,
    /// Empty pixels between BMFont glyphs.
    #[arg(long)]
    bmfont_padding: Option<u32>,
}
impl WriteArguments {
    pub fn apply(
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
