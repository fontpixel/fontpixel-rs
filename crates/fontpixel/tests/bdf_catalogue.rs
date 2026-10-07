//! Explicit catalogue BDF dialect: source fidelity, rounding and bounded losses.
#![cfg(feature = "bdf")]

use fontpixel::formats::bdf::{self, BdfWriteOptions};
use fontpixel::{Error, ResourceLimits};

// Oracle: fontpixel.com pipeline/opf/ingest/bdfwrite.py, not the default
// SPEC-v3 writer. In particular the source SIZE, COMMENT and SWIDTH survive.
const INPUT: &[u8] = b"STARTFONT 2.1\nCOMMENT unquoted source\nCOMMENT \"quoted source\"\nFONT -opf-demo-medium-r-normal--8-80-72-72-p-80-iso10646-1\nSIZE  8  72 72\nFONTBOUNDINGBOX 8 1 0 0\nSTARTPROPERTIES 6\nCOPYRIGHT \"A\"\nFONT_ASCENT 1\nFONT_DESCENT 0\nPIXEL_SIZE 8\nCHARSET_REGISTRY \"ISO10646\"\nCHARSET_ENCODING \"1\"\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nSWIDTH 777 0\nDWIDTH 5 0\nBBX 8 1 0 0\nBITMAP\nAB\nENDCHAR\nENDFONT\n";

#[test]
fn catalogue_matches_pipeline_without_changing_default() {
    let (font, _) = bdf::read(INPUT, &ResourceLimits::default()).unwrap();
    let defaults = bdf::write(&font, &BdfWriteOptions::default()).unwrap().0;
    let options = BdfWriteOptions {
        catalogue_family: Some("demo".into()),
        ..Default::default()
    };
    let (actual, report) = bdf::write(&font, &options).unwrap();
    assert_eq!(actual, INPUT);
    assert!(!report.has_loss());
    assert_ne!(actual, defaults);
}

#[test]
fn catalogue_derives_missing_swidth_with_ties_to_even_and_keeps_cap() {
    let input = String::from_utf8(INPUT.to_vec())
        .unwrap()
        .replace("SWIDTH 777 0\n", "")
        .replace("PIXEL_SIZE 8", "PIXEL_SIZE 16")
        .replace("DWIDTH 5 0", "DWIDTH 1 0");
    let (font, _) = bdf::read(input.as_bytes(), &ResourceLimits::default()).unwrap();
    let mut options = BdfWriteOptions {
        catalogue_family: Some("demo".into()),
        ..Default::default()
    };
    let bytes = bdf::write(&font, &options).unwrap().0;
    assert!(String::from_utf8(bytes).unwrap().contains("SWIDTH 62 0\n"));
    options.max_output_bytes = 50;
    assert!(matches!(
        bdf::write(&font, &options),
        Err(Error::ResourceLimit { .. })
    ));
}

#[test]
fn catalogue_reports_latin1_loss_and_ignores_stale_raw_comments() {
    let (mut font, _) = bdf::read(INPUT, &ResourceLimits::default()).unwrap();
    font.headers.set("COMMENT", "新注释");
    let options = BdfWriteOptions {
        catalogue_family: Some("demo".into()),
        ..Default::default()
    };
    let (bytes, report) = bdf::write(&font, &options).unwrap();
    assert!(report.has_loss());
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("COMMENT \"???\""));
    assert!(!text.contains("COMMENT \"quoted source\""));
}

#[test]
fn catalogue_keeps_source_padding_bits_but_normalizes_hex_case() {
    let input = String::from_utf8(INPUT.to_vec())
        .unwrap()
        .replace("BBX 8 1 0 0", "BBX 3 1 0 0")
        .replace("\nAB\n", "\nab\n");
    let (font, _) = bdf::read(input.as_bytes(), &ResourceLimits::default()).unwrap();
    let options = BdfWriteOptions {
        catalogue_family: Some("demo".into()),
        ..Default::default()
    };
    let bytes = bdf::write(&font, &options).unwrap().0;
    assert!(String::from_utf8(bytes).unwrap().contains("BITMAP\nAB\n"));
}
