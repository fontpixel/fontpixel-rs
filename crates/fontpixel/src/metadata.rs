//! Atomic, resource-bounded edits to normalized font names and notices.
//!
//! This module changes [`FontNames`] only. Original XLFD names, localized names,
//! properties, headers and source provenance remain untouched. Writers reconcile
//! those source records according to their existing policies. In particular, a
//! same-value override does not force an independently edited raw property back
//! to its original value. An empty notice clears its normalized text; it does not
//! delete source records or establish permission to modify/distribute a font.

use crate::{
    Error, FontSet, Report, ReportEntry, ResourceLimits, Result,
    model::FontNames,
    report::{Impact, Level, Location},
};
use std::mem::size_of;

/// Explicit normalized name replacements, applied to every strike.
///
/// `None` leaves a field alone. `Some` preserves the supplied text exactly;
/// whitespace-only identity fields are rejected rather than silently trimmed.
/// An actual family/style change clears an existing full/PostScript name unless
/// the corresponding replacement is supplied. Empty notice/foundry strings are
/// permitted, without deleting any original source metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NameOverrides {
    /// Family name; must contain a non-whitespace character when supplied.
    pub family: Option<String>,
    /// Style name; must contain a non-whitespace character when supplied.
    pub style: Option<String>,
    /// Full display name; must contain a non-whitespace character when supplied.
    pub full_name: Option<String>,
    /// PostScript name; format-specific character restrictions remain writer checks.
    pub postscript_name: Option<String>,
    /// Literal version text, with no automatically added or removed prefix.
    pub version: Option<String>,
    /// Copyright text; an empty string explicitly clears the normalized text.
    pub copyright: Option<String>,
    /// License text/identifier; editing it does not establish legal permission.
    pub license: Option<String>,
    /// License URL; an empty string explicitly clears the normalized text.
    pub license_url: Option<String>,
    /// Foundry/vendor name; an empty string is permitted.
    pub foundry: Option<String>,
}
impl NameOverrides {
    /// Whether no replacements were supplied.
    pub fn is_empty(&self) -> bool {
        Field::ALL.iter().all(|field| field.input(self).is_none())
    }
}

#[derive(Clone, Copy)]
enum Field {
    Family,
    Style,
    Full,
    Postscript,
    Version,
    Copyright,
    License,
    LicenseUrl,
    Foundry,
}
impl Field {
    const ALL: [Self; 9] = [
        Self::Family,
        Self::Style,
        Self::Full,
        Self::Postscript,
        Self::Version,
        Self::Copyright,
        Self::License,
        Self::LicenseUrl,
        Self::Foundry,
    ];
    fn input(self, names: &NameOverrides) -> &Option<String> {
        match self {
            Self::Family => &names.family,
            Self::Style => &names.style,
            Self::Full => &names.full_name,
            Self::Postscript => &names.postscript_name,
            Self::Version => &names.version,
            Self::Copyright => &names.copyright,
            Self::License => &names.license,
            Self::LicenseUrl => &names.license_url,
            Self::Foundry => &names.foundry,
        }
    }
    fn value(self, names: &FontNames) -> &Option<String> {
        match self {
            Self::Family => &names.family,
            Self::Style => &names.style,
            Self::Full => &names.full,
            Self::Postscript => &names.postscript,
            Self::Version => &names.version,
            Self::Copyright => &names.copyright,
            Self::License => &names.license,
            Self::LicenseUrl => &names.license_url,
            Self::Foundry => &names.foundry,
        }
    }
    fn assign(self, names: &mut FontNames, value: Option<String>) {
        *match self {
            Self::Family => &mut names.family,
            Self::Style => &mut names.style,
            Self::Full => &mut names.full,
            Self::Postscript => &mut names.postscript,
            Self::Version => &mut names.version,
            Self::Copyright => &mut names.copyright,
            Self::License => &mut names.license,
            Self::LicenseUrl => &mut names.license_url,
            Self::Foundry => &mut names.foundry,
        } = value;
    }
    fn identity(self) -> bool {
        matches!(
            self,
            Self::Family | Self::Style | Self::Full | Self::Postscript | Self::Version
        )
    }
    fn audit(self, value: Option<&str>) -> (&'static str, &'static str) {
        if value.is_none() {
            let code = match self {
                Self::Full => "metadata.clear_derived.full_name",
                Self::Postscript => "metadata.clear_derived.postscript_name",
                _ => unreachable!("only derived names are cleared"),
            };
            return (
                code,
                "Cleared a stale normalized derived name after an explicit family/style change; original source records remain unchanged",
            );
        }
        let code = match self {
            Self::Family => "metadata.override.family",
            Self::Style => "metadata.override.style",
            Self::Full => "metadata.override.full_name",
            Self::Postscript => "metadata.override.postscript_name",
            Self::Version => "metadata.override.version",
            Self::Copyright => "metadata.override.copyright",
            Self::License => "metadata.override.license",
            Self::LicenseUrl => "metadata.override.license_url",
            Self::Foundry => "metadata.override.foundry",
        };
        let message = if matches!(self, Self::Copyright | Self::License | Self::LicenseUrl) {
            if value == Some("") {
                "Explicitly cleared normalized notice text; original source records remain unchanged. This does not establish permission to modify or distribute the font"
            } else {
                "Explicitly replaced normalized notice text; original source records remain unchanged. This does not establish permission to modify or distribute the font"
            }
        } else {
            "Applied an explicit normalized name override; original source records remain unchanged"
        };
        (code, message)
    }
}

struct PlannedEdit<'a> {
    font_index: usize,
    field: Field,
    value: Option<&'a str>,
}
struct StagedEdit {
    font_index: usize,
    field: Field,
    value: Option<String>,
}
fn changes<'a>(
    set: &'a FontSet,
    overrides: &'a NameOverrides,
) -> impl Iterator<Item = PlannedEdit<'a>> {
    set.fonts
        .iter()
        .enumerate()
        .flat_map(move |(font_index, font)| {
            let identity_changed = [Field::Family, Field::Style].iter().any(|field| {
                field
                    .input(overrides)
                    .as_ref()
                    .is_some_and(|value| field.value(&font.names).as_ref() != Some(value))
            });
            Field::ALL.into_iter().filter_map(move |field| {
                let previous = field.value(&font.names).as_deref();
                let replacement = if let Some(value) = field.input(overrides).as_deref() {
                    (previous != Some(value)).then_some(Some(value))
                } else if identity_changed && matches!(field, Field::Full | Field::Postscript) {
                    previous.map(|_| None)
                } else {
                    None
                };
                replacement.map(|value| PlannedEdit {
                    font_index,
                    field,
                    value,
                })
            })
        })
}

#[derive(Clone, Copy)]
struct Budget {
    used: usize,
    maximum: usize,
}
impl Budget {
    fn error(&self) -> Error {
        Error::ResourceLimit {
            resource: "metadata working bytes",
            limit: self.maximum,
        }
    }
    fn charge(&mut self, bytes: usize) -> Result<()> {
        self.used = self.used.checked_add(bytes).ok_or_else(|| self.error())?;
        if self.used > self.maximum {
            return Err(self.error());
        }
        Ok(())
    }
    fn array<T>(&mut self, count: usize) -> Result<()> {
        self.charge(
            count
                .checked_mul(size_of::<T>())
                .ok_or_else(|| self.error())?,
        )
    }
    fn vector<T>(&mut self, count: usize) -> Result<Vec<T>> {
        self.array::<T>(count)?;
        let mut result = Vec::new();
        result.try_reserve_exact(count).map_err(|_| self.error())?;
        self.array::<T>(result.capacity() - count)?;
        Ok(result)
    }
    fn string(&mut self, text: &str) -> Result<String> {
        self.charge(text.len())?;
        let mut result = String::new();
        result
            .try_reserve_exact(text.len())
            .map_err(|_| self.error())?;
        self.charge(result.capacity() - text.len())?;
        result.push_str(text);
        Ok(result)
    }
}

/// Atomically apply normalized name overrides to all strikes.
///
/// No glyphs, bitmaps, indexes or unrelated metadata are cloned or changed.
/// Every actual explicit change and derived-name clearing is audited, in strike
/// and field order, as [`Level::Info`] / [`Impact::None`]: intentional metadata
/// editing is distinct from a writer's lossy conversion. Notice edits never
/// establish legal permission. Identical replacements produce no audit entry.
///
/// Input values, the model and all allocations are validated before mutation.
/// Each supplied UTF-8 value is bounded by `max_table_bytes`; `max_total_bytes`
/// covers the retained source model, staged edits/strings and the complete owned
/// report. Both old and replacement strings count while staging. Immutable
/// caller-owned options, allocator bookkeeping and stack/process-wide storage
/// are outside this owned-storage budget. Allocation failures and checked-size
/// overflow return [`Error::ResourceLimit`]. Any error leaves the set unchanged.
///
/// Original raw records remain authoritative under each writer's existing
/// reconciliation rules. For example, a same-value normalized override does not
/// undo a separate raw-property edit made after parsing. Rename operations that
/// also require changing XLFD or raw aliases must explicitly edit those fields.
///
/// ```
/// use fontpixel::{Font, FontSet, ResourceLimits, metadata::{NameOverrides, apply_names}};
/// let mut fonts = FontSet::from(Font::default());
/// let report = apply_names(&mut fonts, &NameOverrides {
///     family: Some("Example Pixel".into()),
///     ..Default::default()
/// }, &ResourceLimits::default())?;
/// assert_eq!(fonts.fonts[0].names.family.as_deref(), Some("Example Pixel"));
/// assert!(!report.has_loss());
/// # Ok::<(), fontpixel::Error>(())
/// ```
pub fn apply_names(
    fonts: &mut FontSet,
    overrides: &NameOverrides,
    limits: &ResourceLimits,
) -> Result<Report> {
    for field in Field::ALL {
        if let Some(value) = field.input(overrides) {
            if value.len() > limits.max_table_bytes {
                return Err(Error::ResourceLimit {
                    resource: "metadata name bytes",
                    limit: limits.max_table_bytes,
                });
            }
            if field.identity() && value.trim().is_empty() {
                return Err(Error::InvalidInput(format!(
                    "{} requires nonempty identity text",
                    field.audit(Some(value)).0
                )));
            }
        }
    }
    fonts.validate(limits)?;
    let mut budget = Budget {
        used: fonts.retained_usage(limits.max_total_bytes)?.bytes,
        maximum: limits.max_total_bytes,
    };
    // This complete, nonallocating preflight happens before either vector or
    // any replacement/report string is allocated. A second capacity-aware
    // accounting pass below guards allocator-provided spare capacity as well.
    let mut projected = budget;
    let mut count = 0usize;
    for edit in changes(fonts, overrides) {
        count = count.checked_add(1).ok_or_else(|| budget.error())?;
        projected.array::<StagedEdit>(1)?;
        projected.array::<ReportEntry>(1)?;
        projected.charge(edit.value.map_or(0, str::len))?;
        let (code, message) = edit.field.audit(edit.value);
        projected.charge(code.len())?;
        projected.charge(message.len())?;
    }
    let mut staged = budget.vector::<StagedEdit>(count)?;
    let mut report = Report {
        entries: budget.vector::<ReportEntry>(count)?,
    };
    for edit in changes(fonts, overrides) {
        let value = edit.value.map(|text| budget.string(text)).transpose()?;
        let (code, message) = edit.field.audit(edit.value);
        report.entries.push(ReportEntry {
            level: Level::Info,
            impact: Impact::None,
            code: budget.string(code)?,
            location: Location {
                font_index: Some(edit.font_index),
                ..Default::default()
            },
            message: budget.string(message)?,
        });
        staged.push(StagedEdit {
            font_index: edit.font_index,
            field: edit.field,
            value,
        });
    }
    // Everything fallible is finished. Each destination was enumerated above
    // under this exclusive borrow; moving prepared strings cannot allocate.
    for edit in staged {
        edit.field
            .assign(&mut fonts.fonts[edit.font_index].names, edit.value);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_checked_add_and_multiply_fail_without_allocating() {
        let mut addition = Budget {
            used: usize::MAX,
            maximum: usize::MAX,
        };
        assert!(matches!(
            addition.charge(1),
            Err(Error::ResourceLimit { .. })
        ));
        let mut multiplication = Budget {
            used: 0,
            maximum: usize::MAX,
        };
        assert!(matches!(
            multiplication.vector::<StagedEdit>(usize::MAX),
            Err(Error::ResourceLimit { .. })
        ));
    }

    #[test]
    fn budget_reserves_and_charges_all_actual_string_and_vector_capacity() {
        let mut budget = Budget {
            used: 0,
            maximum: 1024,
        };
        let values = budget.vector::<usize>(7).unwrap();
        let text = budget.string("metadata").unwrap();
        assert_eq!(
            budget.used,
            values.capacity() * size_of::<usize>() + text.capacity()
        );
        let mut empty = Budget {
            used: 0,
            maximum: 0,
        };
        assert!(empty.vector::<StagedEdit>(0).unwrap().is_empty());
        assert!(empty.string("").unwrap().is_empty());
        assert_eq!(empty.used, 0);
    }
}
