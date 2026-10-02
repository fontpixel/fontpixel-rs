//! Exact independently generated Python SPEC-v3 parser/writer comparisons.
#![cfg(feature = "bdf")]
#[path = "support/bdf_snapshot.rs"]
mod snapshot;
use fontpixel::ResourceLimits;
use fontpixel::formats::bdf::{self, BdfWriteOptions};
use serde_json::Value;
#[test]
fn python_generated_bdf_parity() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("data/bdf_reference.json")).unwrap();
    assert_eq!(cases.len(), 924);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let read = bdf::read(
            case["source"].as_str().unwrap().as_bytes(),
            &ResourceLimits::default(),
        );
        if let Some(expected) = case.get("parse_error") {
            assert_eq!(
                read.unwrap_err().to_string(),
                expected.as_str().unwrap(),
                "{name}"
            );
            continue;
        }
        let (font, report) = read.unwrap_or_else(|e| panic!("{name}: {e}"));
        let warnings: Vec<_> = report
            .entries
            .iter()
            .filter(|e| e.code.starts_with("bdf.parse."))
            .map(|e| e.message.as_str())
            .collect();
        assert_eq!(
            serde_json::json!(warnings),
            case["warnings"],
            "{name}: warnings"
        );
        assert_eq!(
            snapshot::snapshot(&font),
            case["metadata"],
            "{name}: metadata"
        );
        let output = bdf::write(&font, &BdfWriteOptions::default());
        if let Some(expected) = case.get("write_error") {
            assert_eq!(
                output.unwrap_err().to_string(),
                expected.as_str().unwrap(),
                "{name}: write error"
            );
        } else {
            assert_eq!(
                output.unwrap().0,
                case["output"].as_str().unwrap().as_bytes(),
                "{name}: BDF bytes"
            );
        }
    }
}
#[test]
fn licensed_real_font_goldens() {
    for (source, golden) in [
        (
            include_bytes!("../../../tests/data/bdf/spleen-5x8.bdf").as_slice(),
            include_bytes!("../../../tests/data/bdf/spleen-5x8.golden.bdf").as_slice(),
        ),
        (
            include_bytes!("../../../tests/data/bdf/3x5-block.bdf").as_slice(),
            include_bytes!("../../../tests/data/bdf/3x5-block.golden.bdf").as_slice(),
        ),
    ] {
        let (font, _) = bdf::read(source, &ResourceLimits::default()).unwrap();
        let (bytes, report) = bdf::write(&font, &BdfWriteOptions::default()).unwrap();
        assert_eq!(bytes, golden);
        assert!(!report.has_loss());
        let (again, _) = bdf::read(&bytes, &ResourceLimits::default()).unwrap();
        assert_eq!(snapshot::snapshot(&font), snapshot::snapshot(&again));
        assert_eq!(
            bytes,
            bdf::write(&again, &BdfWriteOptions::default()).unwrap().0
        );
    }
}
