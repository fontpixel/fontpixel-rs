//! Byte-authored original HMZK fixtures, project MIT license.
#![cfg(feature = "hmzk")]
use fontpixel::formats::hmzk::{self, HmzkWriteOptions};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 84];
    b[..4].copy_from_slice(b"HMZK");
    b[4..14].fill(255);
    b[4] = 1;
    b[14] = 4;
    b[16] = 65;
    b[18] = 66;
    for i in 0..64 {
        b[20 + i] = (i * 37) as u8;
    }
    b
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    hmzk::read(&mut ReadContext::new(&input, l)?, &Default::default()).map(|v| v.0)
}
fn options() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.hmzk").unwrap())
}
#[test]
fn native_directory_all_pixels_header_and_strict() {
    let b = fixture();
    let s = read(&b, &Default::default()).unwrap();
    assert_eq!(s.single().unwrap().len(), 2);
    let g = s
        .single()
        .unwrap()
        .glyph_for_codepoint(fontpixel::model::Codepoint::Unicode(66))
        .unwrap();
    assert_eq!(g.bitmap.as_bytes(), &b[52..84]);
    let mut o = options();
    o.strict = true;
    let out = hmzk::write(&s, &o, &Default::default()).unwrap();
    assert!(!out.report.has_loss());
    assert_eq!(out.output.into_single().unwrap(), b);
}
#[test]
fn malformed_lengths_mappings_and_pixel_limit() {
    let b = fixture();
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "{n}");
    }
    for (p, x) in [(14, 3), (18, 65), (17, 0xd8)] {
        let mut bad = b.clone();
        bad[p] = x;
        assert!(read(&bad, &Default::default()).is_err());
    }
    assert!(
        read(
            &b,
            &ResourceLimits {
                max_pixels_per_glyph: 255,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn full_metadata_loss_and_same_limits() {
    let mut s = read(&fixture(), &Default::default()).unwrap();
    s.fonts[0].names.family = Some("not representable".into());
    let mut o = options();
    o.strict = true;
    assert!(hmzk::write(&s, &o, &Default::default()).is_err());
    o.strict = false;
    assert!(
        hmzk::write(&s, &o, &Default::default())
            .unwrap()
            .report
            .has_loss()
    );
    for total in [0, 1000, 10000, 100000] {
        let l = ResourceLimits {
            max_total_bytes: total,
            ..Default::default()
        };
        if let Ok(w) = hmzk::write(
            &s,
            &o,
            &HmzkWriteOptions {
                limits: l.clone(),
                ..Default::default()
            },
        ) {
            assert!(read(&w.output.into_single().unwrap(), &l).is_ok());
        }
    }
}

#[test]
fn registry_dispatch_and_strict_metadata_loss() {
    let bytes = fixture();
    let registry = fontpixel::registry();
    let mut loaded = registry
        .read(
            &Input::new(&bytes),
            &fontpixel::ReadOptions {
                format: Some("hmzk".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(loaded.fonts, read(&bytes, &Default::default()).unwrap());
    loaded.fonts.fonts[0]
        .properties
        .push(fontpixel::model::Property::new(
            "e1.unrepresentable",
            123i64,
        ));
    let mut options = options();
    let output = registry.write(&loaded.fonts, "hmzk", &options).unwrap();
    assert!(output.report.has_loss());
    options.strict = true;
    assert!(registry.write(&loaded.fonts, "hmzk", &options).is_err());
}
