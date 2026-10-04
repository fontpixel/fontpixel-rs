//! Original MIT byte-authored tasprint boundary and resource fixtures.
#![cfg(feature = "tasprint")]
use fontpixel::formats::tasprint::{self, TasprintWriteOptions};
use fontpixel::model::{Codepoint, FontSet};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 1920];
    let p = 33 * 20;
    b[p] = 0x80;
    b[p + 1] = 0x40;
    b[p + 12] = 0x20;
    b
}
fn read(b: &[u8], limits: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, limits)?;
    Ok(tasprint::read(&mut c, &Default::default())?.0)
}
fn output() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.tasprint").unwrap())
}
#[test]
fn original_fixture_pixels_and_native_roundtrip() {
    let s = read(&fixture(), &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 96);
    let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert!(g.bitmap.as_bytes().iter().any(|v| *v != 0));
    let w = tasprint::write(&s, &output(), &Default::default()).unwrap();
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
        if let Ok(w) = tasprint::write(
            &s,
            &output(),
            &TasprintWriteOptions {
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
    assert!(tasprint::write(&s, &opts, &Default::default()).is_err());
    let w = tasprint::write(&s, &output(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
}
#[test]
fn multi_strike_and_every_platform() {
    use fontpixel::formats::tasprint::TasprintVersion;
    let mut b = fixture();
    b.extend_from_slice(&fixture());
    let set = read(&b, &Default::default()).unwrap();
    assert_eq!(set.fonts.len(), 2);
    let w = tasprint::write(&set, &output(), &Default::default()).unwrap();
    assert_eq!(w.output.into_single().unwrap(), b);
    let l = ResourceLimits {
        max_glyphs: 191,
        ..Default::default()
    };
    assert!(read(&b, &l).is_err());
    let s = read(&fixture(), &Default::default()).unwrap();
    for v in [
        TasprintVersion::Plus3,
        TasprintVersion::Cpc,
        TasprintVersion::Pcw,
    ] {
        let w = tasprint::write(
            &s,
            &output(),
            &TasprintWriteOptions {
                version: v,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(read(&w.output.into_single().unwrap(), &Default::default()).is_ok());
    }
    assert!(
        tasprint::write(
            &set,
            &output(),
            &TasprintWriteOptions {
                version: TasprintVersion::Plus3,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn checksums_hidden_columns_and_headerless_detection() {
    use fontpixel::formats::tasprint::{TasprintCodec, TasprintVersion};
    use fontpixel::formats::{Confidence, FormatDetector};
    let s = read(&fixture(), &Default::default()).unwrap();
    let mut b = tasprint::write(
        &s,
        &output(),
        &TasprintWriteOptions {
            version: TasprintVersion::Plus3,
            ..Default::default()
        },
    )
    .unwrap()
    .output
    .into_single()
    .unwrap();
    b[128] = 2;
    let input = Input::new(&b);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &l).unwrap();
    let (_, r) = tasprint::read(&mut c, &Default::default()).unwrap();
    assert!(r.has_loss());
    b[127] ^= 1;
    assert!(read(&b, &l).is_err());
    let b = fixture();
    let input = Input::new(&b);
    let mut c = ReadContext::new(&input, &l).unwrap();
    assert_eq!(
        TasprintCodec::default().detect(&mut c).unwrap(),
        Confidence::NoMatch
    );
}
