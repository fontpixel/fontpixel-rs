//! Macintosh resource containers and explicit legacy charsets.
use super::{require_feature, require_target};
use crate::options::check_choice;
use fontpixel::{FontRegistry, ResourceLimits, Result};

/// Read options: macintosh resource containers and explicit legacy charsets.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct ReadOptions {
    /// Explicit NFNT legacy charset label.
    #[cfg_attr(feature = "clap", arg(long))]
    pub nfnt_read_charset: Option<String>,
    /// Relative AppleDouble companion when reading the data fork.
    #[cfg_attr(feature = "clap", arg(long))]
    pub nfnt_apple_double_resource: Option<String>,
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

const WRITE_OPTIONS_NFNT_CONTAINER: [&str; 8] = [
    "bare",
    "resource-fork",
    "dfont",
    "suitcase",
    "macbinary",
    "apple-single",
    "apple-double",
    "binhex",
];
const WRITE_OPTIONS_NFNT_RESOURCE_TYPE: [&str; 2] = ["nfnt", "font"];
/// Write options: macintosh resource containers and explicit legacy charsets.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct WriteOptions {
    /// Macintosh output container (default dfont).
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_NFNT_CONTAINER))]
    pub nfnt_container: Option<String>,
    /// Resource type used for newly authored Macintosh bitmaps.
    #[cfg_attr(feature = "clap", arg(long, value_parser = WRITE_OPTIONS_NFNT_RESOURCE_TYPE))]
    pub nfnt_resource_type: Option<String>,
}
impl WriteOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        if let Some(value) = self.nfnt_container.as_deref() {
            check_choice("nfnt_container", value, &WRITE_OPTIONS_NFNT_CONTAINER)?;
        }
        if let Some(value) = self.nfnt_resource_type.as_deref() {
            check_choice(
                "nfnt_resource_type",
                value,
                &WRITE_OPTIONS_NFNT_RESOURCE_TYPE,
            )?;
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
