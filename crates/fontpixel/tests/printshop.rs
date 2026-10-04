//! Original MIT byte-authored printshop boundary and resource fixtures.
#![cfg(feature = "printshop")]
use fontpixel::formats::printshop::{self, PrintshopWriteOptions};
use fontpixel::model::{Codepoint, FontSet};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 607];
    b[0] = 9;
    b[1] = 3;
    b[4 + 33] = 9;
    b[99 + 33] = 3;
    b[592] = 3;
    b[601..].copy_from_slice(&[0x80, 0x80, 0x40, 0, 0xff, 0x80]);
    b
}
fn read(b: &[u8], limits: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, limits)?;
    Ok(printshop::read(&mut c, &Default::default())?.0)
}
fn output() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.printshop").unwrap())
}
#[test]
fn original_fixture_pixels_and_native_roundtrip() {
    let s = read(&fixture(), &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 95);
    let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert!(g.bitmap.as_bytes().iter().any(|v| *v != 0));
    let w = printshop::write(&s, &output(), &Default::default()).unwrap();
    let back = read(&w.output.into_single().unwrap(), &Default::default()).unwrap();
    let h = back
        .single()
        .unwrap()
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    assert_eq!(g.bitmap, h.bitmap);
    assert_eq!(g.metrics.bounds, h.metrics.bounds);
    assert_eq!(g.metrics.advance, h.metrics.advance);
}
#[test]
fn every_truncation_and_trailing_byte_rejected() {
    let b = fixture();
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "prefix {n}");
    }
    let mut bad = b;
    bad.push(0);
    assert!(read(&bad, &Default::default()).is_err());
}
#[test]
fn reader_budgets_and_writer_same_limits() {
    let b = fixture();
    for limits in [
        ResourceLimits {
            max_glyphs: 0,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 0,
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: b.len(),
            ..Default::default()
        },
    ] {
        assert!(read(&b, &limits).is_err());
    }
    let s = read(&b, &Default::default()).unwrap();
    let mut successes = 0;
    for cap in [0, 1024, 65536, 262144, 1048576] {
        let limits = ResourceLimits {
            max_total_bytes: cap,
            ..Default::default()
        };
        if let Ok(w) = printshop::write(
            &s,
            &output(),
            &PrintshopWriteOptions {
                limits: limits.clone(),
                ..Default::default()
            },
        ) {
            assert!(read(&w.output.into_single().unwrap(), &limits).is_ok());
            successes += 1;
        }
    }
    assert!(successes > 0);
}
#[test]
fn strict_rejects_unsupported_metadata() {
    let mut s = read(&fixture(), &Default::default()).unwrap();
    s.fonts[0].names.copyright = Some("original test metadata".into());
    let mut opts = output();
    opts.strict = true;
    assert!(printshop::write(&s, &opts, &Default::default()).is_err());
    let w = printshop::write(&s, &output(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
}
#[test]
fn logo_and_opaque_fields_preserved() {
    let b = fixture();
    let mut with_logo = b[..601].to_vec();
    with_logo[590] = 2;
    with_logo[591] = 2;
    with_logo[384] = 0x55;
    with_logo.extend_from_slice(&[0xaa, 0x55, 0x33, 0xcc]);
    with_logo.extend_from_slice(&b[601..]);
    let s = read(&with_logo, &Default::default()).unwrap();
    let out = printshop::write(&s, &output(), &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert_eq!(&out[601..605], &[0xaa, 0x55, 0x33, 0xcc]);
    assert_eq!(out[384], 0x55);
    let input = Input::new(&b);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &l).unwrap();
    assert!(
        printshop::read(
            &mut c,
            &printshop::PrintshopReadOptions {
                baseline: Some(i64::MAX)
            }
        )
        .is_err()
    );
}
