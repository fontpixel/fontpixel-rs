//! Phase-6 reconstruction from recorded edits; pending compilation.
//! Explicit, budgeted normalized naming changes shared by conversion commands.
use clap::Args;
use fontpixel::{
    Error, FontSet, Report, ReportEntry, ResourceLimits, Result, metadata::NameOverrides,
};
use std::mem::size_of;

fn identity(value: &str) -> std::result::Result<String, String> {
    if value.trim().is_empty() {
        Err("an explicit identity value cannot be empty or whitespace".into())
    } else {
        Ok(value.to_owned())
    }
}

#[derive(Args, Default)]
#[group(skip)]
pub(crate) struct NameArguments {
    /// Explicit normalized output family; original source/XLFD provenance is retained.
    #[arg(long, value_parser = identity)]
    family: Option<String>,
    /// Explicit normalized output style/subfamily; does not infer weight or permissions.
    #[arg(long, value_parser = identity)]
    style: Option<String>,
    /// Exact full display name; otherwise a changed family/style clears a stale one.
    #[arg(long, value_parser = identity)]
    full_name: Option<String>,
    /// Exact PostScript name; otherwise a changed family/style clears a stale one.
    #[arg(long, value_parser = identity)]
    postscript_name: Option<String>,
    /// Literal normalized output version name (no implicit Version prefix).
    #[arg(long, value_parser = identity)]
    font_version: Option<String>,
    /// Explicit copyright statement; this does not infer legal permission.
    #[arg(long)]
    copyright: Option<String>,
    /// Explicit license text; unrelated original provenance is retained.
    #[arg(long)]
    license: Option<String>,
    /// Explicit license URL; does not grant permission or contact the URL.
    #[arg(long)]
    license_url: Option<String>,
    /// Explicit foundry/vendor display name.
    #[arg(long)]
    foundry: Option<String>,
}
impl NameArguments {
    pub(crate) fn into_overrides(self) -> NameOverrides {
        NameOverrides {
            family: self.family,
            style: self.style,
            full_name: self.full_name,
            postscript_name: self.postscript_name,
            version: self.font_version,
            copyright: self.copyright,
            license: self.license,
            license_url: self.license_url,
            foundry: self.foundry,
        }
    }
}
fn add(total: &mut usize, amount: usize, limit: usize) -> Result<()> {
    *total = total
        .checked_add(amount)
        .filter(|value| *value <= limit)
        .ok_or(Error::ResourceLimit {
            resource: "CLI metadata and retained diagnostics",
            limit,
        })?;
    Ok(())
}
fn report_bytes(report: &Report, limit: usize) -> Result<usize> {
    let mut bytes = size_of::<Report>();
    add(
        &mut bytes,
        report
            .entries
            .capacity()
            .checked_mul(size_of::<ReportEntry>())
            .ok_or(Error::ResourceLimit {
                resource: "CLI metadata report entries",
                limit,
            })?,
        limit,
    )?;
    for entry in &report.entries {
        add(&mut bytes, entry.code.capacity(), limit)?;
        add(&mut bytes, entry.message.capacity(), limit)?;
        add(
            &mut bytes,
            entry.location.asset.as_ref().map_or(0, String::capacity),
            limit,
        )?;
    }
    Ok(bytes)
}

pub(crate) fn apply(
    fonts: &mut FontSet,
    names: &NameOverrides,
    report: &mut Report,
    limits: &ResourceLimits,
) -> Result<()> {
    if names.is_empty() {
        return Ok(());
    }
    let existing_report = report_bytes(report, limits.max_total_bytes)?;
    let mut available = limits.clone();
    available.max_total_bytes = available
        .max_total_bytes
        .checked_sub(existing_report)
        .ok_or(Error::ResourceLimit {
            resource: "CLI metadata and retained diagnostics",
            limit: limits.max_total_bytes,
        })?;
    let audit = fontpixel::metadata::apply_names(fonts, names, &available)?;
    // The library accounts model, staging and its returned report. The CLI also
    // owns earlier read diagnostics and must reserve the receiving Vec before
    // combining them. Failure here still precedes every writer/persistence call.
    let mut retained = existing_report;
    add(
        &mut retained,
        report_bytes(&audit, limits.max_total_bytes)?,
        limits.max_total_bytes,
    )?;
    let count = report
        .entries
        .len()
        .checked_add(audit.entries.len())
        .ok_or(Error::ResourceLimit {
            resource: "CLI metadata report entries",
            limit: limits.max_total_bytes,
        })?;
    if count > report.entries.capacity() {
        add(
            &mut retained,
            count
                .checked_mul(size_of::<ReportEntry>())
                .ok_or(Error::ResourceLimit {
                    resource: "CLI metadata report entries",
                    limit: limits.max_total_bytes,
                })?,
            limits.max_total_bytes,
        )?;
    }
    fonts.retained_usage(limits.max_total_bytes - retained)?;
    report
        .entries
        .try_reserve_exact(audit.entries.len())
        .map_err(|_| Error::ResourceLimit {
            resource: "CLI metadata report allocation",
            limit: limits.max_total_bytes,
        })?;
    report.extend(audit);
    Ok(())
}
