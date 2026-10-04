//! ASCII metadata batching preserves exact YAFF escaping and resource bounds.
#![cfg(feature = "yaff")]

use fontpixel::formats::yaff::{self, YaffWriteOptions};
use fontpixel::model::Property;
use fontpixel::{Error, Font, FontSet, Glyph, ResourceLimits, ResourcePath, WriteOptions};
use serde::Serialize;

// Independent copy of the original per-character formatter. Comparing complete
// metadata records catches ordering, escaping and run-boundary changes together.
struct LegacyAsciiJson;
impl serde_json::ser::Formatter for LegacyAsciiJson {
    fn write_string_fragment<W: ?Sized + std::io::Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> std::io::Result<()> {
        for c in fragment.chars() {
            if c.is_ascii() && !c.is_control() {
                writer.write_all(&[c as u8])?;
            } else {
                let mut utf16 = [0_u16; 2];
                for value in c.encode_utf16(&mut utf16) {
                    write!(writer, "\\u{value:04x}")?;
                }
            }
        }
        Ok(())
    }
}

fn metadata_model(text: &str) -> FontSet {
    let property = Property::new(format!("key-{text}"), text);
    let mut glyph = Glyph::default();
    glyph.properties.push(property.clone());
    let mut font = Font::from_glyphs([glyph]).unwrap();
    font.names.localized.push(property.clone());
    font.headers.push(property.clone());
    font.properties.push(property.clone());
    let mut set = FontSet::from(font);
    set.properties.push(property);
    set
}

#[test]
fn ascii_unicode_and_control_fragments_match_legacy_bytes() {
    let ascii: String = (0x20_u8..=0x7e).map(char::from).collect();
    let controls: String = (0..=0x1f)
        .chain(0x7f..=0x9f)
        .map(char::from_u32)
        .map(Option::unwrap)
        .collect();
    for text in [
        String::new(),
        ascii.repeat(24),
        controls,
        "\u{7f}".into(),
        "\u{7f}ASCII-tail".into(),
        "ASCII-head\u{7f}".into(),
        "\u{7f}\u{7f}\u{7f}".into(),
        "ASCII\u{7f}\u{7f}middle\u{7f}tail".into(),
        "\u{7f}\0ASCII\n\"\\é\u{7f}😀\tASCII\u{7f}".into(),
        "é日本語😀\u{10000}\u{10ffff}".into(),
        "ASCIIéASCII😀ASCII\u{7f}ASCII\"ASCII\\ASCII\nASCII\tASCII".into(),
        "éstart-middle-end😀".into(),
    ] {
        let set = metadata_model(&text);
        let (bytes, report) = yaff::write(&set, &YaffWriteOptions::default()).unwrap();
        assert!(report.entries.is_empty());
        let output = std::str::from_utf8(&bytes).unwrap();
        let mut records = 0;
        for line in output
            .lines()
            .filter(|line| line.trim_start().starts_with("fontpixel."))
        {
            let (_, json) = line.split_once(": ").unwrap();
            let value: serde_json::Value = serde_json::from_str(json).unwrap();
            let mut legacy = Vec::new();
            value
                .serialize(&mut serde_json::Serializer::with_formatter(
                    &mut legacy,
                    LegacyAsciiJson,
                ))
                .unwrap();
            assert_eq!(json.as_bytes(), legacy);
            assert!(json.is_ascii());
            records += 1;
        }
        assert_eq!(records, 3);
        let (back, read_report) = yaff::read(&bytes, &ResourceLimits::default()).unwrap();
        assert_eq!(back, set);
        assert!(read_report.entries.is_empty());
        assert_eq!(
            yaff::write(&back, &YaffWriteOptions::default()).unwrap().0,
            bytes
        );
    }
}

#[test]
fn utf16_surrogates_and_serde_escapes_have_exact_spelling() {
    let set = metadata_model("lefté😀\u{7f}\u{85}\0\x08\x0c\n\r\t\"\\right");
    let (bytes, _) = yaff::write(&set, &YaffWriteOptions::default()).unwrap();
    let output = std::str::from_utf8(&bytes).unwrap();
    let expected = r#""Text":"left\u00e9\ud83d\ude00\u007f\u0085\u0000\b\f\n\r\t\"\\right""#;
    assert_eq!(output.matches(expected).count(), 5);
}

#[test]
fn every_global_output_boundary_still_rejects_truncation() {
    let set = metadata_model("unchanged-ASCII-run-é😀-tail");
    let copy = set.clone();
    let (expected, report) = yaff::write(&set, &YaffWriteOptions::default()).unwrap();
    for maximum in 0..expected.len() {
        let options = YaffWriteOptions {
            max_output_bytes: maximum,
            ..Default::default()
        };
        assert!(
            matches!(
                yaff::write(&set, &options),
                Err(Error::ResourceLimit { .. })
            ),
            "limit {maximum}"
        );
    }
    let options = YaffWriteOptions {
        max_output_bytes: expected.len(),
        ..Default::default()
    };
    let (bytes, exact_report) = yaff::write(&set, &options).unwrap();
    assert_eq!(bytes, expected);
    assert_eq!(exact_report, report);
    assert_eq!(set, copy);
}

#[test]
fn metadata_record_limit_counts_encoded_ascii_and_unicode_bytes() {
    for text in ["ASCII-run-".repeat(256), "é😀\u{7f}".repeat(256)] {
        let mut set = FontSet::from(Font::default());
        set.source_format = Some(text);
        let (expected, report) = yaff::write(&set, &YaffWriteOptions::default()).unwrap();
        let length = std::str::from_utf8(&expected)
            .unwrap()
            .lines()
            .find(|line| line.starts_with("fontpixel.container:"))
            .unwrap()
            .len();
        let mut options = YaffWriteOptions::default();
        options.limits.max_table_bytes = length - 1;
        assert!(
            matches!(yaff::write(&set, &options), Err(Error::ResourceLimit { resource: "YAFF metadata output bytes", limit }) if limit == length - 1)
        );
        options.limits.max_table_bytes = length;
        let (bytes, exact_report) = yaff::write(&set, &options).unwrap();
        assert_eq!(bytes, expected);
        assert_eq!(exact_report, report);
    }
}

#[test]
fn strict_registry_write_keeps_exact_bytes_and_empty_report() {
    let set = metadata_model("ASCII-é-😀-\u{7f}-\"-\\-tail");
    let (expected, report) = yaff::write(&set, &YaffWriteOptions::default()).unwrap();
    let mut options = WriteOptions::new(ResourcePath::new("out.yaff").unwrap());
    options.strict = true;
    let written = fontpixel::registry().write(&set, "yaff", &options).unwrap();
    assert_eq!(written.output.into_single().unwrap(), expected);
    assert_eq!(written.report, report);
    assert!(report.entries.is_empty());
}
