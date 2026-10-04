//! Original MIT byte-authored signum boundary and resource fixtures.
#![cfg(feature = "signum")]
use fontpixel::formats::signum::{self, SignumWriteOptions};
use fontpixel::model::{Codepoint, FontSet};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 652 + 127 * 4 + 6];
    b[..8].copy_from_slice(b"eset0001");
    b[8..12].copy_from_slice(&128u32.to_be_bytes());
    b[140..144].copy_from_slice(&(127u32 * 4 + 6).to_be_bytes());
    let mut at = 652;
    for cp in 0..127 {
        b[144 + cp * 4..148 + cp * 4].copy_from_slice(&((at - 652) as u32).to_be_bytes());
        if cp == 65 {
            b[at..at + 10].copy_from_slice(&[15, 3, 5, 0, 0xa0, 0, 0x40, 0, 0xe0, 0]);
            at += 10;
        } else {
            at += 4
        }
    }
    b
}
fn read(b: &[u8], limits: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, limits)?;
    Ok(signum::read(&mut c, &Default::default())?.0)
}
fn output() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.signum").unwrap())
}
#[test]
fn original_fixture_pixels_and_native_roundtrip() {
    let s = read(&fixture(), &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 1);
    let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert!(g.bitmap.as_bytes().iter().any(|v| *v != 0));
    let w = signum::write(&s, &output(), &Default::default()).unwrap();
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
        if let Ok(w) = signum::write(
            &s,
            &output(),
            &SignumWriteOptions {
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
    assert!(signum::write(&s, &opts, &Default::default()).is_err());
    let w = signum::write(&s, &output(), &Default::default()).unwrap();
    assert!(w.report.has_loss());
}
#[test]
fn authoritative_offsets_aliases_and_signed_rows() {
    let mut b = fixture();
    let p = b[144 + 65 * 4..148 + 65 * 4].to_vec();
    b[144 + 66 * 4..148 + 66 * 4].copy_from_slice(&p);
    let s = read(&b, &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(
        f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap().bitmap,
        f.glyph_for_codepoint(Codepoint::Legacy(66)).unwrap().bitmap
    );
    b[144..148].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(read(&b, &Default::default()).is_err());
    let b = fixture();
    let input = Input::new(&b);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &l).unwrap();
    assert!(
        signum::read(
            &mut c,
            &signum::SignumReadOptions {
                baseline: Some(i64::MIN),
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn every_printer_variant_roundtrips() {
    let s = read(&fixture(), &Default::default()).unwrap();
    for version in [
        signum::SignumVersion::P9,
        signum::SignumVersion::P24,
        signum::SignumVersion::L30,
    ] {
        let w = signum::write(
            &s,
            &output(),
            &SignumWriteOptions {
                version,
                ..Default::default()
            },
        )
        .unwrap();
        let b = w.output.into_single().unwrap();
        let set = read(&b, &Default::default()).unwrap();
        assert_eq!(
            set.fonts[0]
                .glyph_for_codepoint(Codepoint::Legacy(65))
                .unwrap()
                .bitmap
                .as_bytes(),
            [0xa0, 0x40, 0xe0]
        );
    }
}
