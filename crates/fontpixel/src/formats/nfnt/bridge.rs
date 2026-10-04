//! Private native container/raster reuse for the Apple formats.
#[cfg(feature = "lisa")]
use super::NfntReadOptions;
use super::{NfntWriteOptions, support::Budget};
#[cfg(feature = "lisa")]
use crate::Font;
#[cfg(feature = "fbit")]
use crate::{FontSet, Report};
use crate::{ResourceLimits, Result};
#[cfg(feature = "fbit")]
pub(crate) fn envelope_signature(data: &[u8]) -> bool {
    super::container::envelope_signature(data)
}
#[cfg(feature = "fbit")]
pub(crate) fn resources(
    data: &[u8],
    limits: &ResourceLimits,
    used: &mut usize,
) -> Result<(FontSet, Report)> {
    let mut budget = Budget::new(*used, limits)?;
    let result = super::container::parse_resources(data, limits, &mut budget)?;
    *used = budget.used;
    Ok(result)
}
#[cfg(feature = "fbit")]
pub(crate) fn wrap_resources(
    set: &FontSet,
    options: &NfntWriteOptions,
    path: &crate::resources::ResourcePath,
    used: &mut usize,
) -> Result<crate::resources::OutputBundle> {
    let mut budget = Budget::new(*used, &options.limits)?;
    let mut report = Report::default();
    let fork = super::writer::encode_fork(set, options, &mut budget, &mut report)?;
    let out = super::writer::wrap(fork, set, options, path, &mut budget)?;
    if report.has_loss() {
        return Err(super::support::invalid(
            "opaque resource generation unexpectedly changes native semantics",
        ));
    }
    *used = budget.used;
    Ok(out)
}
#[cfg(feature = "lisa")]
pub(crate) fn raster(
    data: &[u8],
    charset: Option<String>,
    limits: &ResourceLimits,
    used: &mut usize,
) -> Result<Font> {
    let mut budget = Budget::new(*used, limits)?;
    let f = super::raster::parse(
        data,
        &NfntReadOptions {
            charset,
            ..Default::default()
        },
        limits,
        &mut budget,
    )?;
    *used = budget.used;
    Ok(f)
}
#[cfg(feature = "lisa")]
pub(crate) fn encode_raster(
    font: &Font,
    options: &NfntWriteOptions,
    used: &mut usize,
) -> Result<Vec<u8>> {
    let mut budget = Budget::new(*used, &options.limits)?;
    let mut report = crate::Report::default();
    let bytes = super::writer::encode_font(font, options, &mut budget, &mut report, 0, None)?;
    *used = budget.used;
    Ok(bytes)
}
