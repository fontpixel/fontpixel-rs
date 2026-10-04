//! Explicit P2 codec options shared by inspection and conversion.
mod cjk;
mod console;
mod editor;
mod legacy;
mod mac;
mod tex;
use clap::Args;
use fontpixel::{Error, FontRegistry, ResourceLimits, Result};

#[derive(Args, Default)]
#[group(skip)]
pub(crate) struct ReadArguments {
    #[command(flatten)]
    console: console::ReadArguments,
    #[command(flatten)]
    cjk: cjk::ReadArguments,
    #[command(flatten)]
    mac: mac::ReadArguments,
    #[command(flatten)]
    editor: editor::ReadArguments,
    #[command(flatten)]
    legacy: legacy::ReadArguments,
}
impl ReadArguments {
    pub fn apply(&self, registry: &mut FontRegistry) -> Result<()> {
        self.console.apply(registry)?;
        self.cjk.apply(registry)?;
        self.mac.apply(registry)?;
        self.editor.apply(registry)?;
        self.legacy.apply(registry)
    }
}
#[derive(Args, Default)]
#[group(skip)]
pub(crate) struct WriteArguments {
    #[command(flatten)]
    console: console::WriteArguments,
    #[command(flatten)]
    cjk: cjk::WriteArguments,
    #[command(flatten)]
    mac: mac::WriteArguments,
    #[command(flatten)]
    editor: editor::WriteArguments,
    #[command(flatten)]
    tex: tex::WriteArguments,
    #[command(flatten)]
    legacy: legacy::WriteArguments,
}
impl WriteArguments {
    pub fn apply(
        &self,
        id: &str,
        destination: &std::path::Path,
        input: &ReadArguments,
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
