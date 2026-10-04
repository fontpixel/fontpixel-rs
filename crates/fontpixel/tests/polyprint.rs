//! Original MIT byte-authored polyprint boundary and resource fixtures.
#![cfg(feature = "polyprint")]
use fontpixel::formats::polyprint::{self, PolyprintWriteOptions};
use fontpixel::model::{Codepoint, FontSet};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 12224];
    b[2] = 25;
    b[3..28].fill(b' ');
    b[29] = 32;
    for i in 0..144 {
        let p = 128 + i * 84;
        b[p] = 1;
        b[p + 21] = 3;
        b[p + 42] = 1;
    }
    b[128 + 33 * 84 + 1] = 0x80;
    b[128 + 33 * 84 + 2] = 0x40;
    b
}
fn read(b: &[u8], limits: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, limits)?;
    Ok(polyprint::read(&mut c, &Default::default())?.0)
}
fn output() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.polyprint").unwrap())
}
#[test]
fn original_fixture_pixels_and_native_roundtrip() {
    let s = read(&fixture(), &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 144);
    let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert!(g.bitmap.as_bytes().iter().any(|v| *v != 0));
    let w = polyprint::write(&s, &output(), &Default::default()).unwrap();
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
        if let Ok(w) = polyprint::write(
            &s,
            &output(),
            &PolyprintWriteOptions {
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
    assert!(polyprint::write(&s, &opts, &Default::default()).is_err());
    let w = polyprint::write(&s, &output(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
}
#[test]
fn signed_bearings_and_bounded_override() {
    let mut b = fixture();
    b[128 + 33 * 84] = 4;
    let s = read(&b, &Default::default()).unwrap();
    let g = s.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    assert_eq!(g.metrics.bounds.x, -3);
    assert_eq!(s.fonts[0].metrics.bounds.unwrap().x, -3);
    assert_eq!(s.fonts[0].metrics.bounds.unwrap().width, 23);
    let out = polyprint::write(&s, &output(), &Default::default()).unwrap();
    assert_eq!(
        read(&out.output.into_single().unwrap(), &Default::default())
            .unwrap()
            .fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .metrics
            .bounds
            .x,
        -3
    );
    let input = Input::new(&b);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &l).unwrap();
    assert!(
        polyprint::read(
            &mut c,
            &polyprint::PolyprintReadOptions {
                baseline: Some(i64::MIN)
            }
        )
        .is_err()
    );
}
#[test]
fn cpm_record_padding_retained() {
    let mut b = fixture();
    b.resize(12288, 0x5a);
    let s = read(&b, &Default::default()).unwrap();
    let out = polyprint::write(&s, &output(), &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert_eq!(&out[12224..], &[0x5a; 64]);
}
