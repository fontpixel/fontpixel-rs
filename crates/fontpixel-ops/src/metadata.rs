//! Explicit, budgeted normalized naming changes shared by conversion commands.
use fontpixel::model::{Rational, Vector};
use fontpixel::projection::{
    BitmapProjection, EditValue, MAX_PROPERTY_EDITS, PropertyEdit, ScalableAdvance, SizeProjection,
    apply_bitmap_projection,
};
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

/// Normalized naming overrides applied to every selected strike.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct NameOptions {
    /// Explicit normalized output family; original source/XLFD provenance is retained.
    #[cfg_attr(feature = "clap", arg(long, value_parser = identity))]
    pub family: Option<String>,
    /// Explicit normalized output style/subfamily; does not infer weight or permissions.
    #[cfg_attr(feature = "clap", arg(long, value_parser = identity))]
    pub style: Option<String>,
    /// Exact full display name; otherwise a changed family/style clears a stale one.
    #[cfg_attr(feature = "clap", arg(long, value_parser = identity))]
    pub full_name: Option<String>,
    /// Exact PostScript name; otherwise a changed family/style clears a stale one.
    #[cfg_attr(feature = "clap", arg(long, value_parser = identity))]
    pub postscript_name: Option<String>,
    /// Literal normalized output version name (no implicit Version prefix).
    #[cfg_attr(feature = "clap", arg(long, value_parser = identity))]
    pub font_version: Option<String>,
    /// Explicit copyright statement; this does not infer legal permission.
    #[cfg_attr(feature = "clap", arg(long))]
    pub copyright: Option<String>,
    /// Explicit license text; unrelated original provenance is retained.
    #[cfg_attr(feature = "clap", arg(long))]
    pub license: Option<String>,
    /// Explicit license URL; does not grant permission or contact the URL.
    #[cfg_attr(feature = "clap", arg(long))]
    pub license_url: Option<String>,
    /// Explicit foundry/vendor display name.
    #[cfg_attr(feature = "clap", arg(long))]
    pub foundry: Option<String>,
}
impl NameOptions {
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

/// Explicit target-application projection; omitted means no change. Every
/// replaced or removed source value is reported as loss, so --strict rejects it.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "clap", derive(clap::Args))]
#[cfg_attr(feature = "clap", group(skip))]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct ProjectionOptions {
    /// Replace every glyph's scalable advance (BDF SWIDTH x) with
    /// round-half-even(DWIDTH x * --swidth-units / PPEM). Overwritten source
    /// values are reported as loss, so --strict rejects them.
    #[cfg_attr(feature = "clap", arg(long, value_parser = clap::value_parser!(u32).range(1..)))]
    pub swidth_ppem: Option<u32>,
    /// Scalable units per em for --swidth-ppem (BDF uses 1000).
    #[cfg_attr(feature = "clap", arg(long, requires = "swidth_ppem", value_parser = clap::value_parser!(u32).range(1..)))]
    pub swidth_units: Option<u32>,
    /// Exact FONT/XLFD name. The decomposed XLFD fields and an existing FONT
    /// property follow it; other name properties are not rewritten.
    #[cfg_attr(feature = "clap", arg(long, value_parser = identity))]
    pub font_name: Option<String>,
    /// Point size (BDF SIZE); requires --resolution.
    #[cfg_attr(feature = "clap", arg(long, requires = "resolution", value_parser = clap::value_parser!(u32).range(1..)))]
    pub point_size: Option<u32>,
    /// Resolution as X,Y dots per inch (BDF SIZE); requires --point-size.
    #[cfg_attr(feature = "clap", arg(long, requires = "point_size", value_parser = resolution))]
    pub resolution: Option<(u32, u32)>,
    /// Set an integer property, NAME=VALUE (repeatable). PIXEL_SIZE,
    /// FONT_ASCENT/DESCENT and other integer metrics keep the model in sync.
    #[cfg_attr(feature = "clap", arg(long, value_name = "NAME=VALUE"))]
    pub set_integer_property: Vec<String>,
    /// Set a text property, NAME=TEXT (repeatable).
    #[cfg_attr(feature = "clap", arg(long, value_name = "NAME=TEXT"))]
    pub set_text_property: Vec<String>,
    /// Remove every record of a property (repeatable). Comments and
    /// copyright/notice/licence properties cannot be removed.
    #[cfg_attr(feature = "clap", arg(long, value_name = "NAME"))]
    pub remove_property: Vec<String>,
}
fn resolution(value: &str) -> std::result::Result<(u32, u32), String> {
    let parse = |s: &str| {
        s.trim()
            .parse::<u32>()
            .ok()
            .filter(|v| (1..=i32::MAX as u32).contains(v))
    };
    value
        .split_once(',')
        .and_then(|(x, y)| Some((parse(x)?, parse(y)?)))
        .ok_or_else(|| "expected X,Y with two positive integers".into())
}
fn assignment(value: &str) -> Result<(&str, &str)> {
    value
        .split_once('=')
        .filter(|(name, _)| !name.is_empty())
        .ok_or_else(|| Error::InvalidInput(format!("expected NAME=VALUE, found {value:?}")))
}
impl ProjectionOptions {
    fn edit_count(&self) -> usize {
        self.set_integer_property.len() + self.set_text_property.len() + self.remove_property.len()
    }
    /// Heap bytes of the argument strings retained while the projection runs.
    fn argument_bytes(&self) -> usize {
        self.font_name.as_ref().map_or(0, String::capacity)
            + [
                &self.set_integer_property,
                &self.set_text_property,
                &self.remove_property,
            ]
            .iter()
            .map(|values| {
                values.capacity() * size_of::<String>()
                    + values.iter().map(String::capacity).sum::<usize>()
            })
            .sum::<usize>()
    }
    fn parse_edits<'a>(&'a self, edits: &mut Vec<PropertyEdit<'a>>) -> Result<()> {
        for value in &self.set_integer_property {
            let (name, number) = assignment(value)?;
            let number = number.trim().parse::<i64>().map_err(|_| {
                Error::InvalidInput(format!("property {name} needs an integer value"))
            })?;
            edits.push(PropertyEdit::Set {
                name,
                value: EditValue::Integer(number),
            });
        }
        for value in &self.set_text_property {
            let (name, text) = assignment(value)?;
            edits.push(PropertyEdit::Set {
                name,
                value: EditValue::Text(text),
            });
        }
        for name in &self.remove_property {
            edits.push(PropertyEdit::Remove { name });
        }
        Ok(())
    }
    fn projection<'a>(&'a self, edits: &'a [PropertyEdit<'a>]) -> Option<BitmapProjection<'a>> {
        let mut projection = BitmapProjection::default();
        if let Some(ppem) = self.swidth_ppem {
            projection.scalable_advance = ScalableAdvance::FromDevice {
                units_per_em: self.swidth_units.unwrap_or(1000),
                reference_ppem: ppem,
            };
        }
        projection.font_name = self.font_name.as_deref();
        if let (Some(point), Some((x, y))) = (self.point_size, self.resolution) {
            projection.size = Some(SizeProjection {
                point_size: Rational::integer(i64::from(point)),
                resolution: Vector::new(i64::from(x), i64::from(y)),
            });
        }
        projection.properties = edits;
        (!projection.is_identity()).then_some(projection)
    }
}

/// Apply an explicit projection after naming and before writing. Earlier
/// diagnostics, the live model and the argument storage are reserved before
/// the projection's own staging, and its diagnostics join the report before
/// strict checks.
pub(crate) fn project(
    fonts: &mut FontSet,
    arguments: &ProjectionOptions,
    report: &mut Report,
    limits: &ResourceLimits,
) -> Result<()> {
    let limit = limits.max_total_bytes;
    let error = || Error::ResourceLimit {
        resource: "CLI projection and retained diagnostics",
        limit,
    };
    let count = arguments.edit_count();
    if count > MAX_PROPERTY_EDITS {
        return Err(Error::InvalidInput(format!(
            "at most {MAX_PROPERTY_EDITS} property edits are allowed"
        )));
    }
    let mut retained = report_bytes(report, limit)?;
    add(&mut retained, arguments.argument_bytes(), limit)?;
    add(
        &mut retained,
        count
            .checked_mul(size_of::<PropertyEdit<'_>>())
            .ok_or_else(error)?,
        limit,
    )?;
    let mut edits = Vec::new();
    edits.try_reserve_exact(count).map_err(|_| error())?;
    arguments.parse_edits(&mut edits)?;
    let Some(projection) = arguments.projection(&edits) else {
        return Ok(());
    };
    add(&mut retained, fonts.retained_usage(limit)?.bytes, limit)?;
    let outcome = apply_bitmap_projection(fonts, &projection, limit - retained)?;
    add(&mut retained, outcome.retained_bytes(), limit)?;
    let count = report
        .entries
        .len()
        .checked_add(outcome.report.entries.len())
        .ok_or_else(error)?;
    if count > report.entries.capacity() {
        add(
            &mut retained,
            count
                .checked_mul(size_of::<ReportEntry>())
                .ok_or_else(error)?,
            limit,
        )?;
    }
    report
        .entries
        .try_reserve_exact(outcome.report.entries.len())
        .map_err(|_| error())?;
    report.extend(outcome.report);
    Ok(())
}
