//! Explicit P2 codec options shared by inspection and conversion.
pub mod cjk;
pub mod console;
pub mod editor;
pub mod legacy;
pub mod mac;
pub mod tex;
use fontpixel::{Error, FontRegistry, ResourceLimits, Result};

/// Read options: explicit P2 codec options shared by inspection and conversion.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct ReadOptions {
    /// Console options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub console: console::ReadOptions,
    /// Cjk options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub cjk: cjk::ReadOptions,
    /// Mac options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub mac: mac::ReadOptions,
    /// Editor options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub editor: editor::ReadOptions,
    /// Legacy options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub legacy: legacy::ReadOptions,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(ReadOptions);
impl ReadOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        self.console.validate()?;
        self.cjk.validate()?;
        self.mac.validate()?;
        self.editor.validate()?;
        self.legacy.validate()?;
        Ok(())
    }
}
impl ReadOptions {
    pub(crate) fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        self.console.apply(registry)?;
        self.cjk.apply(registry)?;
        self.mac.apply(registry)?;
        self.editor.apply(registry)?;
        self.legacy.apply(registry)
    }
}
/// Write options: explicit P2 codec options shared by inspection and conversion.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default, remote = "Self")
)]
pub struct WriteOptions {
    /// Console options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub console: console::WriteOptions,
    /// Cjk options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub cjk: cjk::WriteOptions,
    /// Mac options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub mac: mac::WriteOptions,
    /// Editor options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub editor: editor::WriteOptions,
    /// Tex options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub tex: tex::WriteOptions,
    /// Legacy options.
    #[cfg_attr(feature = "clap", command(flatten))]
    #[cfg_attr(
        feature = "serde",
        serde(flatten, deserialize_with = "crate::serde_keys::flattened")
    )]
    pub legacy: legacy::WriteOptions,
}
#[cfg(feature = "serde")]
crate::serde_keys::strict!(WriteOptions);
impl WriteOptions {
    /// Check the value constraints the command-line parser enforces (choice
    /// lists, numeric ranges and value syntax); operations call this first.
    pub fn validate(&self) -> fontpixel::Result<()> {
        self.console.validate()?;
        self.cjk.validate()?;
        self.mac.validate()?;
        self.editor.validate()?;
        self.tex.validate()?;
        self.legacy.validate()?;
        Ok(())
    }
}
impl WriteOptions {
    pub(crate) fn apply(
        &self,
        id: &str,
        destination: &std::path::Path,
        input: &ReadOptions,
        limits: &ResourceLimits,
        threshold: Option<u8>,
        registry: &mut FontRegistry,
    ) -> Result<()> {
        self.console
            .apply(id, destination, &input.console, limits, threshold, registry)?;
        self.cjk
            .apply(id, &input.cjk, limits, threshold, registry)?;
        self.mac.apply(id, limits, threshold, registry)?;
        self.editor.apply(id, limits, registry)?;
        self.tex.apply(id, limits, threshold, registry)?;
        self.legacy.apply(id, limits, threshold, registry)
    }
}
fn require_feature(requested: bool, enabled: bool, id: &str) -> Result<()> {
    if requested && !enabled {
        return Err(Error::FeatureDisabled { format: id.into() });
    }
    Ok(())
}
fn require_target(requested: bool, target: &str, id: &str) -> Result<()> {
    if requested && target != id {
        return Err(Error::InvalidInput(format!(
            "{id} output options require --to {id}"
        )));
    }
    Ok(())
}
