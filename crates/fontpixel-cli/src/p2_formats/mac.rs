//! Macintosh resource containers and explicit legacy charsets.
use super::{require_feature, require_target};
use clap::Args;
use fontpixel::{FontRegistry, ResourceLimits, Result};

#[derive(Args, Default)]
#[group(skip)]
pub(super) struct ReadArguments {
    /// Explicit NFNT legacy charset label.
    #[arg(long)]
    nfnt_read_charset: Option<String>,
    /// Relative AppleDouble companion when reading the data fork.
    #[arg(long)]
    nfnt_apple_double_resource: Option<String>,
}
impl ReadArguments {
    pub fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        require_feature(
            self.nfnt_read_charset.is_some() || self.nfnt_apple_double_resource.is_some(),
            cfg!(feature = "nfnt"),
            "nfnt",
        )?;
        #[cfg(feature = "nfnt")]
        registry.set_nfnt_read_options(fontpixel::formats::nfnt::NfntReadOptions {
            charset: self.nfnt_read_charset.clone(),
            apple_double_resource: self
                .nfnt_apple_double_resource
                .clone()
                .map(fontpixel::ResourcePath::new)
                .transpose()?,
        });
        let _ = registry;
        Ok(())
    }
}

#[derive(Args, Default)]
#[group(skip)]
pub(super) struct WriteArguments {
    /// Macintosh output container (default dfont).
    #[arg(long, value_parser = ["bare", "resource-fork", "dfont", "suitcase", "macbinary", "apple-single", "apple-double", "binhex"])]
    nfnt_container: Option<String>,
    /// Resource type used for newly authored Macintosh bitmaps.
    #[arg(long, value_parser = ["nfnt", "font"])]
    nfnt_resource_type: Option<String>,
}
impl WriteArguments {
    pub fn apply(
        &self,
        id: &str,
        limits: &ResourceLimits,
        threshold: Option<u8>,
        registry: &mut FontRegistry,
    ) -> Result<()> {
        let requested = self.nfnt_container.is_some() || self.nfnt_resource_type.is_some();
        require_target(requested, id, "nfnt")?;
        require_feature(requested || id == "nfnt", cfg!(feature = "nfnt"), "nfnt")?;
        #[cfg(feature = "nfnt")]
        if id == "nfnt" {
            use fontpixel::formats::nfnt::{Container, NfntWriteOptions, ResourceType};
            registry.set_nfnt_write_options(NfntWriteOptions {
                container: match self.nfnt_container.as_deref() {
                    Some("bare") => Container::Bare,
                    Some("resource-fork") => Container::ResourceFork,
                    Some("suitcase") => Container::Suitcase,
                    Some("macbinary") => Container::MacBinary,
                    Some("apple-single") => Container::AppleSingle,
                    Some("apple-double") => Container::AppleDouble,
                    Some("binhex") => Container::BinHex,
                    _ => Container::Dfont,
                },
                resource_type: if self.nfnt_resource_type.as_deref() == Some("font") {
                    ResourceType::Font
                } else {
                    ResourceType::Nfnt
                },
                gray_threshold: threshold,
                limits: limits.clone(),
                ..Default::default()
            });
        }
        let _ = (limits, threshold, registry);
        Ok(())
    }
}
