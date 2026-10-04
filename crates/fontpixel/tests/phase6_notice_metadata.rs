//! Bounded BDF notice projection preserves text while reporting line-break loss.
#![cfg(feature = "bdf")]

use fontpixel::{
    Error, Font, FontSet, Report, ResourceLimits, ResourcePath, WriteOptions,
    formats::bdf::{self, BdfWriteOptions},
    model::{Property, PropertyValue},
    report::{Impact, Level},
};

const SOURCE: &str = "STARTFONT 2.1\nFONT notice-test\nSIZE 8 75 75\nFONTBOUNDINGBOX 1 1 0 0\nCOMMENT \"source header notice\"\nSTARTPROPERTIES 3\nCOMMENT \"source property notice\"\nCOPYRIGHT \"original copyright\"\nLICENSE \"original license\"\nLICENSE_URL \"https://example.org/original\"\nENDPROPERTIES\nCHARS 0\nENDFONT\n";

fn source() -> Font {
    bdf::read(SOURCE.as_bytes(), &ResourceLimits::default()).unwrap().0
}
fn write(font: &Font) -> (Vec<u8>, Report) {
    bdf::write(font, &BdfWriteOptions::default()).unwrap()
}
fn read(bytes: &[u8]) -> Font {
    let (font, report) = bdf::read(bytes, &ResourceLimits::default()).unwrap();
    assert!(!report.entries.iter().any(|entry| entry.code == "bdf.parse.property_count"));
    font
}
fn comments(font: &Font) -> Vec<&str> {
    font.properties.iter().filter_map(|property| {
        if property.name.eq_ignore_ascii_case("COMMENT") {
            match &property.value { PropertyValue::Text(value) => Some(value.as_str()), _ => None }
        } else { None }
    }).collect()
}
fn notices(report: &Report) -> usize {
    report.entries.iter().filter(|entry| entry.code == "bdf.write.notice_line_breaks").count()
}

#[test]
fn every_line_break_form_has_one_delimiter_and_preserves_each_line() {
    for separator in ["\n", "\r", "\r\n"] {
        for field in ["COPYRIGHT", "LICENSE", "LICENSE_URL"] {
            let mut font = source();
            let value = Some(format!("first{separator}second"));
            match field {
                "COPYRIGHT" => font.names.copyright = value,
                "LICENSE" => font.names.license = value,
                _ => font.names.license_url = value,
            }
            let before = font.clone();
            let (bytes, report) = write(&font);
            assert_eq!(font, before);
            assert_eq!(notices(&report), 1);
            let entry = report.entries.iter().find(|entry| entry.code == "bdf.write.notice_line_breaks").unwrap();
            assert_eq!(entry.level, Level::Warning);
            assert_eq!(entry.impact, Impact::Loss);
            assert!(entry.message.contains(field));
            assert!(entry.message.contains("does not establish"));
            let decoded = read(&bytes);
            assert_eq!(decoded.properties.get(field), Some(&PropertyValue::Text("first | second".into())));
            assert_eq!(comments(&decoded), ["source property notice", "first", "second"]);
            assert_eq!(decoded.headers.get("COMMENT"), Some(&PropertyValue::Text("source header notice".into())));
        }
    }
}

#[test]
fn quotes_unicode_tabs_blank_and_trailing_lines_are_retained_verbatim() {
    let mut font = source();
    let value = "\r\n  first \"line\"\t  \r\n\rsecond😀\n";
    let expected = ["", "  first \"line\"\t  ", "", "second😀", ""];
    font.names.license = Some(value.into());
    let (bytes, report) = write(&font);
    assert_eq!(notices(&report), 1);
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("COMMENT \"  first \"line\"\t  \"\n"));
    assert!(text.contains("first \"\"line\"\""));
    let decoded = read(&bytes);
    let mut expected_comments = vec!["source property notice"];
    expected_comments.extend(expected);
    assert_eq!(comments(&decoded), expected_comments);
    assert_eq!(decoded.names.license, Some(expected.join(" | ")));
    let (again, repeated_report) = write(&decoded);
    assert_eq!(notices(&repeated_report), 0);
    let again = read(&again);
    assert_eq!(comments(&again), expected_comments);
    assert_eq!(again.names.license, decoded.names.license);
}

#[test]
fn all_three_notices_project_once_in_effective_property_order() {
    let mut font = source();
    font.names.copyright = Some("copyright one\ncopyright two".into());
    font.names.license = Some("license one\rlicense two".into());
    font.names.license_url = Some("https://example.org/one\r\nhttps://example.org/two".into());
    let (bytes, report) = write(&font);
    assert_eq!(notices(&report), 3);
    let projected: Vec<_> = report.entries.iter().filter(|entry| entry.code == "bdf.write.notice_line_breaks").collect();
    assert!(projected[0].message.starts_with("COPYRIGHT "));
    assert!(projected[1].message.starts_with("LICENSE "));
    assert!(projected[2].message.starts_with("LICENSE_URL "));
    let decoded = read(&bytes);
    assert_eq!(comments(&decoded), [
        "source property notice", "copyright one", "copyright two", "license one", "license two",
        "https://example.org/one", "https://example.org/two",
    ]);
    assert_eq!(decoded.names.copyright.as_deref(), Some("copyright one | copyright two"));
    assert_eq!(decoded.names.license.as_deref(), Some("license one | license two"));
    assert_eq!(decoded.names.license_url.as_deref(), Some("https://example.org/one | https://example.org/two"));
}

#[test]
fn strict_registry_refuses_projection_while_ordinary_output_retains_notices() {
    let mut font = source();
    font.names.copyright = Some("Copyright A\nCopyright B".into());
    let fonts = FontSet::from(font);
    let before = fonts.clone();
    let mut options = WriteOptions::new(ResourcePath::new("notice.bdf").unwrap());
    let written = fontpixel::write(&fonts, "bdf", &options).unwrap();
    assert_eq!(notices(&written.report), 1);
    assert!(written.report.has_loss());
    let decoded = read(&written.output.into_single().unwrap());
    assert!(comments(&decoded).contains(&"Copyright B"));
    options.strict = true;
    let error = fontpixel::write(&fonts, "bdf", &options).unwrap_err();
    match error {
        Error::StrictLoss(report) => assert_eq!(notices(&report), 1),
        other => panic!("expected strict loss, found {other}"),
    }
    assert_eq!(fonts, before);
}

#[test]
fn arbitrary_multiline_fields_and_existing_comment_values_still_fail() {
    let edits: [fn(&mut Font); 7] = [
        |font| font.names.family = Some("bad\nfamily".into()),
        |font| font.names.style = Some("bad\rstyle".into()),
        |font| font.names.xlfd = Some("bad\nFONT".into()),
        |font| font.properties.set("UNRELATED", "bad\nvalue"),
        |font| font.properties.set("COPYRIGHT_EXTRA", "bad\nvalue"),
        |font| font.properties.set("COMMENT", "bad\ncomment"),
        |font| font.headers.set("COMMENT", "bad\rcomment"),
    ];
    for edit in edits {
        let mut font = source();
        edit(&mut font);
        let before = font.clone();
        assert!(matches!(bdf::write(&font, &BdfWriteOptions::default()), Err(Error::InvalidInput(_))));
        assert_eq!(font, before);
    }
}

#[test]
fn raw_property_edits_and_duplicate_precedence_remain_authoritative() {
    let mut font = source();
    font.properties.set("COPYRIGHT", "raw first\nraw second");
    let (bytes, report) = write(&font);
    assert_eq!(notices(&report), 1);
    assert_eq!(read(&bytes).names.copyright.as_deref(), Some("raw first | raw second"));
    // Normalized text still equals the source snapshot, so the raw edit wins.
    assert_eq!(font.names.copyright.as_deref(), Some("original copyright"));
    font.properties.push(Property::new("copyright", "discarded first\ndiscarded second"));
    font.properties.push(Property::new("CoPyRiGhT", "effective first\reffective second"));
    let (bytes, report) = write(&font);
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("CoPyRiGhT \"effective first | effective second\""));
    assert!(!text.contains("discarded first"));
    assert!(!text.contains("COMMENT \"raw first\""));
    assert_eq!(notices(&report), 1);
    assert!(report.entries.iter().any(|entry| entry.code == "bdf.write.duplicate_property"));
    // A genuine normalized change still supersedes the raw value under the
    // existing reconciliation contract, without mutating source properties.
    font.names.copyright = Some("normalized first\nnormalized second".into());
    let before = font.clone();
    let (bytes, report) = write(&font);
    assert_eq!(notices(&report), 1);
    assert_eq!(read(&bytes).names.copyright.as_deref(), Some("normalized first | normalized second"));
    assert_eq!(font, before);
}

#[test]
fn unchanged_single_line_and_empty_notice_bytes_keep_existing_behavior() {
    let font = source();
    let (bytes, report) = write(&font);
    assert_eq!(bytes, SOURCE.as_bytes());
    assert_eq!(notices(&report), 0);
    let mut empty = font;
    empty.names.copyright = Some(String::new());
    let (bytes, report) = write(&empty);
    assert_eq!(notices(&report), 0);
    assert!(std::str::from_utf8(&bytes).unwrap().contains("COPYRIGHT \"\"\n"));
    assert_eq!(comments(&read(&bytes)), ["source property notice"]);
}

#[test]
fn exact_output_limit_counts_comment_wrappers_quotes_and_utf8_bytes() {
    let mut font = source();
    font.names.license = Some("\r\n\"quote\"😀\r\n\nlast\n".into());
    let (expected, expected_report) = write(&font);
    let options = BdfWriteOptions { max_output_bytes: expected.len(), ..Default::default() };
    let (actual, report) = bdf::write(&font, &options).unwrap();
    assert_eq!(actual, expected);
    assert_eq!(report, expected_report);
    let short = BdfWriteOptions { max_output_bytes: expected.len() - 1, ..Default::default() };
    assert!(matches!(bdf::write(&font, &short), Err(Error::ResourceLimit { resource: "BDF output bytes", .. })));
    font.names.license = Some("\n".repeat(1000));
    let small = BdfWriteOptions { max_output_bytes: 256, ..Default::default() };
    assert!(matches!(bdf::write(&font, &small), Err(Error::ResourceLimit { resource: "BDF output bytes", limit: 256 })));
}
