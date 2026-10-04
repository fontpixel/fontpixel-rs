//! Migration-only implementation of downloads.py::_renamed_props.
//!
//! This is deliberately separate from normalized name overrides. The caller
//! supplies Python's weight.capitalize() result; Rust does no Unicode casing.
//! Source models are changed only after all fallible staging has succeeded.
use fontpixel::model::{Properties, Property, PropertyValue};
use fontpixel::report::{Impact, Level, Location};
use fontpixel::{Error, Font, Report, ReportEntry, ResourceLimits, Result};
use std::mem::size_of;

pub struct ExportName<'a> {
    pub name: &'a str,
    pub size: i64,
    pub weight: &'a str,
    pub weight_capitalized: &'a str,
    pub width: &'a str,
    pub spacing: &'a str,
    pub script_subset: Option<&'a str>,
}

pub fn limit(maximum: usize) -> Error {
    Error::ResourceLimit { resource: "catalogue export-name owned bytes", limit: maximum }
}

/// Owned-storage ledger, excluding stack, allocator bookkeeping and global state.
/// Reserve the requested size before allocation, then charge any spare capacity.
pub struct Budget { pub used: usize, pub maximum: usize }
impl Budget {
    pub fn charge(&mut self, bytes: usize) -> Result<()> {
        self.used = self.used.checked_add(bytes).filter(|&n| n <= self.maximum)
            .ok_or_else(|| limit(self.maximum))?;
        Ok(())
    }
    pub fn array<T>(&mut self, count: usize) -> Result<()> {
        self.charge(count.checked_mul(size_of::<T>()).ok_or_else(|| limit(self.maximum))?)
    }
    pub fn vector<T>(&mut self, count: usize) -> Result<Vec<T>> {
        self.array::<T>(count)?;
        let mut result = Vec::new();
        result.try_reserve_exact(count).map_err(|_| limit(self.maximum))?;
        self.array::<T>(result.capacity().saturating_sub(count))?;
        Ok(result)
    }
    pub fn text(&mut self, text: &str) -> Result<String> {
        let mut result = self.text_buffer(text.len())?;
        result.push_str(text);
        Ok(result)
    }
    fn text_buffer(&mut self, length: usize) -> Result<String> {
        self.charge(length)?;
        let mut result = String::new();
        result.try_reserve_exact(length).map_err(|_| limit(self.maximum))?;
        self.charge(result.capacity().saturating_sub(length))?;
        Ok(result)
    }
}

fn add(a: usize, b: usize, maximum: usize) -> Result<usize> {
    a.checked_add(b).filter(|&n| n <= maximum).ok_or_else(|| limit(maximum))
}

fn copy_value(value: &PropertyValue, budget: &mut Budget, depth: usize) -> Result<PropertyValue> {
    Ok(match value {
        PropertyValue::Text(s) => PropertyValue::Text(budget.text(s)?),
        PropertyValue::Decimal(s) => PropertyValue::Decimal(budget.text(s)?),
        PropertyValue::Integer(n) => PropertyValue::Integer(*n),
        PropertyValue::Unsigned(n) => PropertyValue::Unsigned(*n),
        PropertyValue::Flag => PropertyValue::Flag,
        PropertyValue::Bytes(bytes) => {
            let mut out = budget.vector(bytes.len())?;
            out.extend_from_slice(bytes);
            PropertyValue::Bytes(out)
        }
        PropertyValue::List(values) => {
            if depth >= 128 { return Err(limit(budget.maximum)); }
            let mut out = budget.vector(values.len())?;
            for value in values { out.push(copy_value(value, budget, depth + 1)?); }
            PropertyValue::List(out)
        }
    })
}

/// Nonallocating estimate of property storage; used by the companion's writer
/// reservation. Nested provenance counts exactly like other retained metadata.
pub fn property_bytes(properties: &Properties, maximum: usize) -> Result<usize> {
    fn value(v: &PropertyValue, budget: &mut Budget, depth: usize) -> Result<()> {
        match v {
            PropertyValue::Text(s) | PropertyValue::Decimal(s) => budget.charge(s.capacity()),
            PropertyValue::Bytes(v) => budget.charge(v.capacity()),
            PropertyValue::List(v) => {
                if depth >= 128 { return Err(limit(budget.maximum)); }
                budget.array::<PropertyValue>(v.capacity())?;
                for item in v { value(item, budget, depth + 1)?; }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    let mut budget = Budget { used: 0, maximum };
    // Public Properties does not expose outer capacity. Writer clones retain
    // initialized records only; spare source capacity is in retained_usage().
    budget.array::<Property>(properties.len())?;
    for property in properties.iter() {
        budget.charge(property.name.capacity())?;
        budget.charge(property.raw.as_ref().map_or(0, Vec::capacity))?;
        value(&property.value, &mut budget, 0)?;
    }
    Ok(budget.used)
}

/// Copy source records, replacing only the last exact-case target. Keep all
/// other records, their order, duplicate keys, source spelling and raw bytes.
fn properties_with<const N: usize>(
    source: &Properties, edits: [(&str, &str); N], budget: &mut Budget,
) -> Result<Properties> {
    let positions: [Option<usize>; N] = std::array::from_fn(|i| {
        source.iter().enumerate().rev().find(|(_, p)| p.name == edits[i].0).map(|(j, _)| j)
    });
    let extra = positions.iter().filter(|p| p.is_none()).count();
    let count = add(source.len(), extra, budget.maximum)?;
    let mut records = budget.vector(count)?;
    for (i, property) in source.iter().enumerate() {
        let replacement = positions.iter().position(|&p| p == Some(i));
        let value = if let Some(j) = replacement {
            PropertyValue::Text(budget.text(edits[j].1)?)
        } else { copy_value(&property.value, budget, 0)? };
        let raw = if replacement.is_some() { None } else if let Some(raw) = &property.raw {
            let mut bytes = budget.vector(raw.len())?;
            bytes.extend_from_slice(raw);
            Some(bytes)
        } else { None };
        records.push(Property { name: budget.text(&property.name)?, value, raw });
    }
    for (i, (key, value)) in edits.iter().enumerate() {
        if positions[i].is_none() {
            records.push(Property { name: budget.text(key)?, value: PropertyValue::Text(budget.text(value)?), raw: None });
        }
    }
    Ok(records.into())
}

/// Format signed decimal on the stack, including i64::MIN.
fn decimal(value: i64, bytes: &mut [u8; 20]) -> &str {
    let mut n = value.unsigned_abs();
    let mut start = bytes.len();
    loop {
        start -= 1;
        bytes[start] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 { break; }
    }
    if value < 0 { start -= 1; bytes[start] = b'-'; }
    std::str::from_utf8(&bytes[start..]).expect("decimal contains only ASCII")
}

fn font_name(font: &Font, rename: &ExportName<'_>, budget: &mut Budget, maximum: usize) -> Result<String> {
    // Native BDF has FONT in headers. Native PCF also retains its actual FONT
    // property; prefer its last value to a separately synthesized header view.
    let property = if font.source_format.as_deref() == Some("pcf") {
        font.properties.get("FONT").or_else(|| font.headers.get("FONT"))
    } else { font.headers.get("FONT") };
    let original = match property { Some(PropertyValue::Text(s)) => s.as_str(), _ => "" };
    if original.starts_with('-') && original.split('-').count() == 15 {
        let old_family = original.split('-').nth(2).unwrap_or("");
        let length = add(original.len() - old_family.len(), rename.name.len(), maximum)?;
        let mut result = budget.text_buffer(length)?;
        for (i, field) in original.split('-').enumerate() {
            if i != 0 { result.push('-'); }
            result.push_str(if i == 2 { rename.name } else { field });
        }
        return Ok(result);
    }
    let mut digits = [0; 20];
    let size = decimal(rename.size, &mut digits);
    let parts = [Some(rename.name), Some(size),
        (rename.weight != "regular").then_some(rename.weight),
        (rename.width != "normal").then_some(rename.width), Some(rename.spacing),
        rename.script_subset.filter(|s| !s.is_empty())];
    let mut length = 2; // The literal "px" after size.
    let mut count = 0;
    for part in parts.into_iter().flatten() { length = add(length, part.len(), maximum)?; count += 1; }
    length = add(length, count - 1, maximum)?;
    let mut result = budget.text_buffer(length)?;
    for (index, part) in parts.into_iter().enumerate() {
        if let Some(part) = part {
            if index != 0 { result.push('-'); }
            if index == 0 {
                // Python replace(' ', '-') changes ASCII spaces only.
                for ch in part.chars() { result.push(if ch == ' ' { '-' } else { ch }); }
            } else { result.push_str(part); }
            if index == 1 { result.push_str("px"); }
        }
    }
    Ok(result)
}

/// Stage only metadata, never the glyph table/bitmaps/indexes. Every allocation
/// coexists with the charged source until commit. The ledger charges staged
/// buffers once; there is no second retained-model check subtracting them again.
/// Caller-owned descriptor strings are outside this function's storage budget.
pub fn apply(font: &mut Font, rename: &ExportName<'_>, limits: &ResourceLimits) -> Result<Report> {
    if rename.name.is_empty() { return Ok(Report::default()); }
    for text in [rename.name, rename.weight, rename.weight_capitalized, rename.width, rename.spacing, rename.script_subset.unwrap_or("")] {
        if text.len() > limits.max_table_bytes { return Err(limit(limits.max_table_bytes)); }
        if text.contains(['\n', '\r', '\0']) {
            return Err(Error::InvalidInput("catalogue export-name metadata contains a record delimiter".into()));
        }
    }
    font.validate(limits)?;
    let mut budget = Budget { used: font.retained_usage(limits.max_total_bytes)?.bytes, maximum: limits.max_total_bytes };
    let name = font_name(font, rename, &mut budget, limits.max_table_bytes)?;
    let has_full = font.properties.get("FULL_NAME").is_some();
    let has_face = font.properties.get("FACE_NAME").is_some();
    let full = if has_full || has_face {
        let length = add(add(rename.name.len(), 1, limits.max_table_bytes)?, rename.weight_capitalized.len(), limits.max_table_bytes)?;
        let mut full = budget.text_buffer(length)?;
        full.push_str(rename.name); full.push(' '); full.push_str(rename.weight_capitalized);
        Some(full)
    } else { None };
    let headers = properties_with(&font.headers, [("FONT", name.as_str())], &mut budget)?;
    // Four small edits are folded into one property-copy pass, avoiding temporary
    // full property collections. Missing aliases remain missing.
    let mut edits = [("FAMILY_NAME", rename.name); 4];
    let mut count = 1;
    if font.properties.get("FONT").is_some() { edits[count] = ("FONT", &name); count += 1; }
    if has_full { edits[count] = ("FULL_NAME", full.as_deref().unwrap()); count += 1; }
    if has_face { edits[count] = ("FACE_NAME", full.as_deref().unwrap()); count += 1; }
    let properties = match count {
        1 => properties_with(&font.properties, [edits[0]], &mut budget)?,
        2 => properties_with(&font.properties, [edits[0], edits[1]], &mut budget)?,
        3 => properties_with(&font.properties, [edits[0], edits[1], edits[2]], &mut budget)?,
        _ => properties_with(&font.properties, edits, &mut budget)?,
    };
    let xlfd = budget.text(&name)?;
    let field_count = name.strip_prefix('-').map_or(0, |s| s.split('-').count());
    let mut fields = budget.vector(field_count)?;
    if let Some(s) = name.strip_prefix('-') { for field in s.split('-') { fields.push(budget.text(field)?); } }
    let family = budget.text(rename.name)?;
    let full_mirror = if has_face { Some(budget.text(full.as_deref().unwrap())?) } else { None };
    let mut report = Report { entries: budget.vector(1)? };
    report.entries.push(ReportEntry {
        level: Level::Info, impact: Impact::None,
        code: budget.text("catalogue.export_name")?,
        location: Location { font_index: Some(0), ..Default::default() },
        message: budget.text("Applied explicit catalogue export-name policy; original notices and source provenance retained")?,
    });
    // All fallible work is complete. These moves cannot allocate.
    font.headers = headers;
    font.properties = properties;
    font.names.xlfd = Some(xlfd);
    font.names.xlfd_fields = fields;
    font.names.family = Some(family);
    if has_face { font.names.full = full_mirror; }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "STARTFONT 2.1\nFONT -foundry-Old-medium-r-normal--8-80-75-75-c-80-iso10646-1\nSIZE 8 75 75\nFONTBOUNDINGBOX 8 1 0 0\nCOMMENT unchanged notice\nSTARTPROPERTIES 6\nFAMILY_NAME \"Old\"\nFULL_NAME \"Old Full\"\nFACE_NAME \"Old Face\"\nCOPYRIGHT \"Copyright original\"\nLICENSE \"OFL-1.1\"\nLICENSE_URL \"https://example.invalid/license\"\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nDWIDTH 8 0\nBBX 8 1 0 0\nBITMAP\n81\nENDCHAR\nENDFONT\n";
    fn descriptor() -> ExportName<'static> {
        ExportName { name: "New Family", size: 8, weight: "regular", weight_capitalized: "Regular", width: "normal", spacing: "mono", script_subset: None }
    }
    fn source() -> Font { fontpixel::formats::bdf::read(SOURCE.as_bytes(), &ResourceLimits::default()).unwrap().0 }
    fn text<'a>(props: &'a Properties, key: &str) -> &'a str {
        match props.get(key).unwrap() { PropertyValue::Text(s) => s, _ => panic!("not text") }
    }
    #[test]
    fn xlfd_aliases_and_only_intended_model_fields_change() {
        let mut font = source(); let original = font.clone();
        let report = apply(&mut font, &descriptor(), &ResourceLimits::default()).unwrap();
        assert_eq!(text(&font.headers, "FONT"), "-foundry-New Family-medium-r-normal--8-80-75-75-c-80-iso10646-1");
        assert_eq!(text(&font.properties, "FULL_NAME"), "New Family Regular");
        assert_eq!(font.names.full.as_deref(), Some("New Family Regular"));
        assert_eq!(font.glyphs().collect::<Vec<_>>(), original.glyphs().collect::<Vec<_>>());
        assert_eq!(font.metrics, original.metrics); assert_eq!(font.charset, original.charset);
        for key in ["COPYRIGHT", "LICENSE", "LICENSE_URL"] { assert_eq!(font.properties.get(key), original.properties.get(key)); }
        for key in ["COMMENT", "bdf.source.headers", "bdf.source.properties"] { assert_eq!(font.headers.get(key), original.headers.get(key)); }
        assert_eq!(font.names.copyright, original.names.copyright);
        assert_eq!(font.names.license, original.names.license);
        assert_eq!(font.names.license_url, original.names.license_url);
        assert_eq!(font.source_format, original.source_format);
        assert_eq!(report.entries.len(), 1); assert!(!report.has_loss());
        let bytes = fontpixel::formats::bdf::write(&font, &Default::default()).unwrap().0;
        let output = String::from_utf8(bytes).unwrap();
        assert!(output.contains("FAMILY_NAME \"New Family\""));
        assert!(output.contains("COPYRIGHT \"Copyright original\""));
        assert!(output.contains("LICENSE \"OFL-1.1\""));
        assert!(output.contains("COMMENT \"unchanged notice\""));
    }
    #[test]
    fn synthesized_name_and_python_unicode_capitalization_are_literal() {
        let mut font = source(); font.headers.set("FONT", "not-an-xlfd");
        let rename = ExportName { name: "New\u{a0}Family Name", size: -12, weight: "ǳABC", weight_capitalized: "ǲabc", width: "condensed", spacing: "mono", script_subset: Some("latin") };
        apply(&mut font, &rename, &ResourceLimits::default()).unwrap();
        assert_eq!(text(&font.headers, "FONT"), "New\u{a0}Family-Name--12px-ǳABC-condensed-mono-latin");
        assert_eq!(text(&font.properties, "FACE_NAME"), "New\u{a0}Family Name ǲabc");
    }
    #[test]
    fn absent_aliases_stay_absent_and_other_names_stay_unchanged() {
        let mut font = source(); font.properties.remove("FULL_NAME"); font.properties.remove("FACE_NAME");
        let full = font.names.full.clone();
        apply(&mut font, &descriptor(), &ResourceLimits::default()).unwrap();
        assert!(font.properties.get("FULL_NAME").is_none()); assert!(font.properties.get("FACE_NAME").is_none());
        assert_eq!(font.names.full, full);
    }
    #[test]
    fn failure_at_each_staging_budget_leaves_source_identical() {
        let original = source();
        let retained = original.retained_usage(usize::MAX).unwrap().bytes;
        let mut saw_success = false;
        for extra in (0..16_384).step_by(127) {
            let mut font = original.clone();
            let limits = ResourceLimits { max_total_bytes: retained + extra, ..Default::default() };
            match apply(&mut font, &descriptor(), &limits) {
                Ok(_) => { saw_success = true; break; }
                Err(Error::ResourceLimit { .. }) => assert_eq!(font, original),
                Err(error) => panic!("unexpected {error}"),
            }
        }
        assert!(saw_success);
        let mut font = original.clone(); let mut rename = descriptor(); rename.name = "line\nbreak";
        assert!(apply(&mut font, &rename, &Default::default()).is_err()); assert_eq!(font, original);
    }
    #[test]
    fn empty_policy_is_noop_and_checked_lengths_do_not_overflow() {
        let mut font = source(); let original = font.clone(); let mut rename = descriptor(); rename.name = "";
        assert!(apply(&mut font, &rename, &Default::default()).unwrap().entries.is_empty()); assert_eq!(font, original);
        let mut budget = Budget { used: usize::MAX, maximum: usize::MAX };
        assert!(budget.charge(1).is_err()); assert!(budget.array::<String>(usize::MAX).is_err());
        let mut digits = [0; 20]; assert_eq!(decimal(i64::MIN, &mut digits), "-9223372036854775808");
    }
    #[test]
    fn exact_case_last_record_updates_preserve_other_records_and_raw_notices() {
        let mut font = source();
        font.properties.push(Property { name: "FAMILY_NAME".into(), value: "Last old family".into(), raw: Some(b"FAMILY_NAME \"Last old family\"\r\n".to_vec()) });
        font.properties.push(Property { name: "copyright".into(), value: "Second preserved notice".into(), raw: Some(b"copyright \"Second preserved notice\"\r\n".to_vec()) });
        let original = font.clone();
        apply(&mut font, &descriptor(), &Default::default()).unwrap();
        let old_records: Vec<_> = original.properties.iter().filter(|p| p.name == "FAMILY_NAME").collect();
        let new_records: Vec<_> = font.properties.iter().filter(|p| p.name == "FAMILY_NAME").collect();
        assert_eq!(old_records.len(), new_records.len()); assert_eq!(new_records[0], old_records[0]);
        assert_eq!(new_records.last().unwrap().value, PropertyValue::Text("New Family".into()));
        assert!(new_records.last().unwrap().raw.is_none());
        assert_eq!(font.properties.iter().last(), original.properties.iter().last());
    }
    #[test]
    fn pcf_uses_last_font_property_and_preserves_original_xlfd_fields_other_than_family() {
        let mut font = source(); font.source_format = Some("pcf".into());
        font.properties.push(Property::new("FONT", "-vendor-Actual-bold-i-condensed--9-90-72-72-p-90-iso8859-1"));
        font.headers.set("FONT", "stale synthesized header");
        apply(&mut font, &descriptor(), &Default::default()).unwrap();
        let expected = "-vendor-New Family-bold-i-condensed--9-90-72-72-p-90-iso8859-1";
        assert_eq!(text(&font.headers, "FONT"), expected); assert_eq!(text(&font.properties, "FONT"), expected);
        assert_eq!(font.names.xlfd.as_deref(), Some(expected));
    }
}
