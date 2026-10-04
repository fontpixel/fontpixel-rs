//! Reconstructed W6.5 metadata implementation; currently untested.
use super::report::LicenseReport;
use super::{Budget, MergeOptions, MergePart, ModificationPermission, budget_entry};
use crate::model::{Font, Property, PropertyValue};
use crate::report::{Impact, Level};
use crate::{Error, Report, Result};
use std::collections::BTreeSet;

fn property_permission(font: &Font) -> ModificationPermission {
    let mut permission = ModificationPermission::Unknown;
    for name in ["MODIFICATION_ALLOWED", "ALLOW_MODIFICATION"] {
        let declaration = match font.properties.get_case_insensitive(name) {
            Some(PropertyValue::Text(text)) => match text.trim().to_ascii_lowercase().as_str() {
                "true" | "yes" | "1" => ModificationPermission::Allowed,
                "false" | "no" | "0" => ModificationPermission::Prohibited,
                _ => ModificationPermission::Unknown,
            },
            Some(PropertyValue::Integer(0) | PropertyValue::Unsigned(0)) => {
                ModificationPermission::Prohibited
            }
            Some(PropertyValue::Integer(1) | PropertyValue::Unsigned(1)) => {
                ModificationPermission::Allowed
            }
            _ => ModificationPermission::Unknown,
        };
        if declaration == ModificationPermission::Prohibited {
            return declaration;
        }
        if declaration == ModificationPermission::Allowed {
            permission = declaration;
        }
    }
    permission
}
fn add_text<'a>(
    value: &'a str,
    values: &mut Vec<&'a str>,
    seen: &mut BTreeSet<&'a str>,
    budget: &mut Budget,
) -> Result<()> {
    if value.trim().is_empty() || seen.contains(value) {
        return Ok(());
    }
    // Conservative one-node-per-entry accounting, independent of the optimized
    // model index layout. Text itself remains borrowed until the final join.
    budget.charge(16 * (std::mem::size_of::<&str>() + std::mem::size_of::<usize>()) + 128)?;
    budget.reserve(values, 1)?;
    seen.insert(value);
    values.push(value);
    Ok(())
}
fn notice_values<'a>(
    value: &'a PropertyValue,
    values: &mut Vec<&'a str>,
    seen: &mut BTreeSet<&'a str>,
    budget: &mut Budget,
) -> Result<bool> {
    match value {
        PropertyValue::Text(text) => {
            add_text(text, values, seen, budget)?;
            Ok(true)
        }
        PropertyValue::List(items) => {
            let mut complete = true;
            for item in items {
                complete &= notice_values(item, values, seen, budget)?;
            }
            Ok(complete)
        }
        _ => Ok(false),
    }
}
fn source_text(
    font: &Font,
    primary: Option<&str>,
    keys: &[&str],
    supplement: Option<&str>,
    index: usize,
    report: &mut Report,
    budget: &mut Budget,
) -> Result<Option<String>> {
    let mut values = Vec::new();
    let mut seen = BTreeSet::new();
    if let Some(text) = primary {
        add_text(text, &mut values, &mut seen, budget)?;
    }
    for property in font
        .properties
        .iter()
        .filter(|p| keys.iter().any(|k| p.name.eq_ignore_ascii_case(k)))
    {
        // Font::retained_usage has already checked the property nesting limit.
        if !notice_values(&property.value, &mut values, &mut seen, budget)? {
            budget_entry(
                report,
                budget,
                index,
                "merge.metadata.notice_value_omitted",
                Level::Warning,
                Impact::Loss,
                format_args!(
                    "Part {index} notice property {:?} contains a non-text value that cannot be projected into merged notices; text/list elements are preserved, opaque bytes are not decoded",
                    property.name
                ),
            )?;
        }
    }
    let source = if values.is_empty() {
        None
    } else {
        Some(budget.text(|out| {
            for (index, text) in values.iter().enumerate() {
                if index != 0 {
                    out.write_char('\n')?;
                }
                out.write_str(text)?;
            }
            Ok(())
        })?)
    };
    // Preserve the established supplement semantics: compare it with the whole
    // combined source notice, rather than deduplicating individual source lines.
    match (source, supplement.filter(|text| !text.trim().is_empty())) {
        (source, None) => Ok(source),
        (None, Some(text)) => budget.text(|out| out.write_str(text)).map(Some),
        (Some(source), Some(text)) if source == text => Ok(Some(source)),
        (Some(source), Some(text)) => budget.text(|out| write!(out, "{source}\n{text}")).map(Some),
    }
}
fn identified_license(value: &str) -> Option<&'static str> {
    let mut found = None;
    for line in value.split(['\n', ';']) {
        let exact = line.trim().trim_end_matches('.').to_ascii_lowercase();
        let id = match exact.as_str() {
            "ofl-1.1"
            | "sil open font license 1.1"
            | "sil open font license version 1.1"
            | "sil open font license, version 1.1"
            | "sil open font license version 1.1 - 26 february 2007"
            | "sil open font license, version 1.1 - 26 february 2007" => Some("OFL-1.1"),
            "mit" | "mit license" => Some("MIT"),
            "apache-2.0" | "apache license, version 2.0" => Some("Apache-2.0"),
            "bsd-2-clause" => Some("BSD-2-Clause"),
            "bsd-3-clause" => Some("BSD-3-Clause"),
            "cc0-1.0" => Some("CC0-1.0"),
            "cc-by-nd-4.0" => return Some("CC-BY-ND-4.0"),
            "cc-by-nc-nd-4.0" => return Some("CC-BY-NC-ND-4.0"),
            _ => None,
        };
        if found.is_none() {
            found = id;
        }
    }
    found
}
fn parse_reserved(text: &str, result: &mut Vec<String>, maximum: usize) -> Result<()> {
    for line in text.split(['\n', ';']) {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        let suffix = lower
            .strip_prefix("reserved font names")
            .map(|s| &trimmed[trimmed.len() - s.len()..])
            .or_else(|| {
                lower
                    .strip_prefix("reserved font name")
                    .map(|s| &trimmed[trimmed.len() - s.len()..])
            });
        if let Some(suffix) = suffix {
            let suffix = suffix.trim_start_matches([' ', ':', '=']);
            if suffix.is_empty()
                || suffix.to_ascii_lowercase().starts_with("refers ")
                || suffix.starts_with("\" refers")
            {
                continue;
            }
            for name in suffix
                .split(',')
                .map(|n| n.trim().trim_matches(['\'', '"']).trim())
            {
                if !name.is_empty() && !result.iter().any(|s| s.eq_ignore_ascii_case(name)) {
                    if result.len() >= maximum {
                        return Err(Error::ResourceLimit {
                            resource: "merge reserved names",
                            limit: maximum,
                        });
                    }
                    result.push(name.into());
                }
            }
        }
    }
    Ok(())
}
pub(super) fn licenses(
    parts: &[MergePart<'_>],
    options: &MergeOptions,
    report: &mut Report,
    budget: &mut Budget,
) -> Result<Vec<LicenseReport>> {
    let mut result = Vec::new();
    budget.reserve(&mut result, parts.len())?;
    for (index, part) in parts.iter().enumerate() {
        let label = part.label_text(index, budget)?;
        let original = part.font;
        let copyright = source_text(
            original,
            original.names.copyright.as_deref(),
            &["COPYRIGHT"],
            part.license.copyright.as_deref(),
            index,
            report,
            budget,
        )?;
        let mut license = source_text(
            original,
            original.names.license.as_deref(),
            &["LICENSE", "LICENSE_TEXT", "SPDX_LICENSE_IDENTIFIER"],
            part.license.license.as_deref(),
            index,
            report,
            budget,
        )?;
        let license_url = source_text(
            original,
            original.names.license_url.as_deref(),
            &["LICENSE_URL"],
            part.license.license_url.as_deref(),
            index,
            report,
            budget,
        )?;
        let license_id = license.as_deref().and_then(identified_license);
        let copyright_id = copyright.as_deref().and_then(identified_license);
        let identifier = [license_id, copyright_id]
            .into_iter()
            .flatten()
            .find(|id| matches!(*id, "CC-BY-ND-4.0" | "CC-BY-NC-ND-4.0"))
            .or(license_id)
            .or(copyright_id);
        if license.is_none() {
            license = identifier.map(str::to_owned);
        }
        let mut modification = part.license.modification;
        let mut provenance = "caller permission override".to_owned();
        if modification == ModificationPermission::Unknown {
            modification = property_permission(original);
            provenance = "explicit modification property".into();
            if modification == ModificationPermission::Unknown {
                if let Some(id) = identifier {
                    modification = if matches!(id, "CC-BY-ND-4.0" | "CC-BY-NC-ND-4.0") {
                        ModificationPermission::Prohibited
                    } else {
                        ModificationPermission::Allowed
                    };
                    provenance = format!("exact standard identifier/title: {id}");
                } else {
                    provenance = "no recognized explicit license/permission declaration".into();
                }
            }
        }
        let mut reserved_names = part.license.reserved_names.clone();
        for p in original.properties.iter().filter(|p| {
            ["RESERVED_FONT_NAMES", "RESERVED_FONT_NAME"]
                .iter()
                .any(|k| p.name.eq_ignore_ascii_case(k))
        }) {
            match &p.value {
                PropertyValue::Text(t) => parse_reserved(
                    &format!("Reserved Font Names: {t}"),
                    &mut reserved_names,
                    options.limits.max_resources,
                )?,
                PropertyValue::List(values) => {
                    for v in values {
                        if let PropertyValue::Text(t) = v {
                            if reserved_names.len() >= options.limits.max_resources {
                                return Err(Error::ResourceLimit {
                                    resource: "merge reserved names",
                                    limit: options.limits.max_resources,
                                });
                            }
                            reserved_names.push(t.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        for value in [&copyright, &license].into_iter().flatten() {
            parse_reserved(value, &mut reserved_names, options.limits.max_resources)?;
        }
        for name in &mut reserved_names {
            *name = name.trim().trim_matches(['\'', '"']).trim().to_owned();
        }
        reserved_names.retain(|n| !n.is_empty());
        let family = options.family.to_lowercase();
        for name in &reserved_names {
            if family.contains(&name.to_lowercase()) {
                return Err(Error::InvalidInput(format!(
                    "new family {:?} contains reserved font name {:?} from part {index}",
                    options.family, name
                )));
            }
        }
        if modification != ModificationPermission::Allowed
            || license.as_deref().is_none_or(|s| s.trim().is_empty())
        {
            if modification == ModificationPermission::Prohibited {
                budget_entry(
                    report,
                    budget,
                    index,
                    "merge.license.prohibited",
                    Level::Warning,
                    Impact::None,
                    format_args!("Part {index} ({label}) explicitly prohibits modification"),
                )?;
            } else {
                budget_entry(
                    report,
                    budget,
                    index,
                    "merge.license.unknown",
                    Level::Warning,
                    Impact::None,
                    format_args!(
                        "Part {index} ({label}) has unknown license or modification permission; confirm modification and redistribution rights"
                    ),
                )?;
            }
        }
        let bytes = copyright.as_ref().map_or(0, String::len)
            + license.as_ref().map_or(0, String::len)
            + license_url.as_ref().map_or(0, String::len)
            + reserved_names.iter().map(String::len).sum::<usize>()
            + label.len()
            + provenance.len();
        budget.charge(bytes.saturating_mul(4).saturating_add(512))?;
        result.push(LicenseReport {
            part_index: index,
            label,
            copyright,
            license,
            license_url,
            modification,
            provenance,
            reserved_names,
        });
    }
    Ok(result)
}
fn notice_field(
    licenses: &[LicenseReport],
    field: fn(&LicenseReport) -> &Option<String>,
    budget: &mut Budget,
) -> Result<String> {
    budget.text(|out| {
        out.write_str("Merged by fontpixel; parts: ")?;
        for (index, item) in licenses.iter().enumerate() {
            if index != 0 {
                out.write_str(", ")?;
            }
            out.write_str(&item.label)?;
        }
        for item in licenses {
            write!(
                out,
                "\n[{}] {}",
                item.label,
                field(item).as_deref().unwrap_or("Unknown")
            )?;
        }
        Ok(())
    })
}
pub(super) fn write_names(
    font: &mut Font,
    options: &MergeOptions,
    licenses: &[LicenseReport],
    budget: &mut Budget,
) -> Result<()> {
    font.names.family = Some(budget.text(|out| out.write_str(&options.family))?);
    font.names.full = Some(budget.text(|out| out.write_str(&options.family))?);
    font.names.style = Some(budget.text(|out| out.write_str("Regular"))?);
    font.names.postscript = Some(budget.text(|out| {
        let mut empty = true;
        for character in options
            .family
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        {
            out.write_char(character)?;
            empty = false;
        }
        if empty {
            out.write_str("FontpixelMerged")?;
        }
        Ok(())
    })?);
    font.names.copyright = Some(notice_field(licenses, |l| &l.copyright, budget)?);
    font.names.license = Some(notice_field(licenses, |l| &l.license, budget)?);
    if licenses.iter().any(|l| l.license_url.is_some()) {
        font.names.license_url = Some(notice_field(licenses, |l| &l.license_url, budget)?);
    }
    // The output Font is new. Build its complete property array fallibly and
    // count each owned copy while normalized notices and reports remain alive.
    let mut properties = Vec::new();
    budget.reserve(
        &mut properties,
        2 + usize::from(font.names.license_url.is_some()),
    )?;
    for (name, value) in [
        ("COPYRIGHT", &font.names.copyright),
        ("LICENSE", &font.names.license),
        ("LICENSE_URL", &font.names.license_url),
    ] {
        if let Some(value) = value {
            properties.push(Property::new(
                budget.text(|out| out.write_str(name))?,
                budget.text(|out| out.write_str(value))?,
            ));
        }
    }
    font.properties = properties.into();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn license() -> LicenseReport {
        LicenseReport {
            part_index: 0,
            label: "quoted\" label\nwith Unicode é".into(),
            copyright: Some("Copyright first\nCopyright second".into()),
            license: Some("OFL-1.1\nAll source notice lines".into()),
            license_url: Some("https://example.invalid/license".into()),
            modification: ModificationPermission::Allowed,
            provenance: "test".into(),
            reserved_names: Vec::new(),
        }
    }

    #[test]
    fn generated_names_reserve_every_owned_buffer_before_allocation() {
        let options = MergeOptions {
            family: "New Family é".into(),
            ..Default::default()
        };
        let licenses = [license()];
        let mut output = Font::default();
        let mut measured = Budget {
            used: 0,
            max: usize::MAX,
        };
        write_names(&mut output, &options, &licenses, &mut measured).unwrap();
        let expected_license = format!(
            "Merged by fontpixel; parts: {}\n[{}] {}",
            licenses[0].label,
            licenses[0].label,
            licenses[0].license.as_deref().unwrap()
        );
        assert_eq!(
            output.names.license.as_deref(),
            Some(expected_license.as_str())
        );
        assert_eq!(output.names.postscript.as_deref(), Some("NewFamily"));
        let names = &output.names;
        let mut owned = [
            &names.family,
            &names.full,
            &names.style,
            &names.postscript,
            &names.copyright,
            &names.license,
            &names.license_url,
        ]
        .into_iter()
        .flatten()
        .map(String::capacity)
        .sum::<usize>();
        owned += output.properties.allocation_capacity() * std::mem::size_of::<Property>();
        for property in output.properties.iter() {
            owned += property.name.capacity();
            let PropertyValue::Text(text) = &property.value else {
                panic!("notice must be text")
            };
            owned += text.capacity();
        }
        assert_eq!(measured.used, owned);
        let mut exact = Budget {
            used: 0,
            max: owned,
        };
        let mut copy = Font::default();
        write_names(&mut copy, &options, &licenses, &mut exact).unwrap();
        assert_eq!(copy, output);
        let mut short = Budget {
            used: 0,
            max: owned - 1,
        };
        assert!(matches!(
            write_names(&mut Font::default(), &options, &licenses, &mut short),
            Err(Error::ResourceLimit { .. })
        ));
    }

    #[test]
    fn notice_text_reservation_checks_exact_and_one_byte_short_boundaries() {
        let source = "\n\0\"\\é".repeat(1024);
        let expected = format!("{source:?}");
        let mut exact = Budget {
            used: 0,
            max: expected.len(),
        };
        assert_eq!(
            exact.text(|out| write!(out, "{source:?}")).unwrap(),
            expected
        );
        assert_eq!(exact.used, expected.len());
        let mut short = Budget {
            used: 0,
            max: expected.len() - 1,
        };
        assert!(matches!(
            short.text(|out| write!(out, "{source:?}")),
            Err(Error::ResourceLimit { .. })
        ));
    }
}
