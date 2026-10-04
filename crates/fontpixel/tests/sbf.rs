//! Original native SBF directory fixtures.
#![cfg(feature = "sbf")]
use fontpixel::formats::sbf::{self, SbfWriteOptions};
use fontpixel::model::{Codepoint, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 2052];
    b[..8].copy_from_slice(&[0x10, 0xb1, 0x7e, 0x5c, 0, 7, 5, 2]);
    let a = 65 * 8;
    b[a..a + 8].copy_from_slice(&[0, 8, 2, 2, 254, 3, 10, 7]);
    let a = 66 * 8;
    b[a..a + 8].copy_from_slice(&[4, 8, 0, 0, 0, 0, 5, 0]);
    b[2048..].copy_from_slice(&[0xaa, 0x80, 0x55, 0]);
    b
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    sbf::read(&mut ReadContext::new(&i, l)?, &Default::default()).map(|v| v.0)
}
fn options() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.sbf").unwrap())
}
#[test]
fn signed_bearings_sparse_directory_and_blank() {
    let b = fixture();
    let s = read(&b, &Default::default()).unwrap();
    let f = s.single().unwrap();
    assert_eq!(f.len(), 2);
    let g = f.glyph_for_codepoint(Codepoint::Legacy(65)).unwrap();
    assert_eq!(
        (
            g.metrics.bounds.x,
            g.metrics.bounds.y,
            g.bitmap.width(),
            g.bitmap.height()
        ),
        (-2, 1, 9, 2)
    );
    assert_eq!(g.metrics.advance, Some(Vector::new(10, 0)));
    assert_eq!(g.bitmap.as_bytes(), [0xaa, 0x80, 0x55, 0]);
    let mut o = options();
    o.strict = true;
    assert_eq!(
        sbf::write(&s, &o, &Default::default())
            .unwrap()
            .output
            .into_single()
            .unwrap(),
        b
    );
}
#[test]
fn fixed_advance_retains_record_field() {
    let mut b = fixture();
    b[4] = 12;
    let s = read(&b, &Default::default()).unwrap();
    assert_eq!(
        s.single()
            .unwrap()
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .metrics
            .advance
            .unwrap()
            .x,
        12
    );
    let mut o = options();
    o.strict = true;
    assert_eq!(
        sbf::write(&s, &o, &Default::default())
            .unwrap()
            .output
            .into_single()
            .unwrap(),
        b
    );
}
#[test]
fn malformed_and_limits() {
    let b = fixture();
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err());
    }
    for (at, x) in [(65 * 8, 1), (65 * 8 + 1, 1), (65 * 8 + 7, 255)] {
        let mut b = b.clone();
        b[at] = x;
        assert!(read(&b, &Default::default()).is_err());
    }
    let s = read(&b, &Default::default()).unwrap();
    for cap in [0, 2048, 2051, 2052, 100000] {
        let l = ResourceLimits {
            max_input_bytes: cap,
            ..Default::default()
        };
        if let Ok(w) = sbf::write(
            &s,
            &options(),
            &SbfWriteOptions {
                limits: l.clone(),
                ..Default::default()
            },
        ) {
            assert!(read(&w.output.into_single().unwrap(), &l).is_ok());
        }
    }
}
#[test]
fn final_bitmap_may_extend_past_u16_address_space() {
    use fontpixel::bitmap::{Bitmap, Bounds, Mono};
    use fontpixel::model::{Font, Glyph};
    let mut bm = Bitmap::<Mono>::new(2040, 255).unwrap();
    bm.set_pixel(2039, 254, 1).unwrap();
    let mut g = Glyph::new(bm);
    g.codepoints.push(Codepoint::Legacy(65));
    g.metrics.bounds = Bounds::new(2040, 255, 0, -255);
    g.metrics.advance = Some(Vector::new(200, 0));
    let s = Font::from_glyphs([g]).unwrap().into();
    let out = sbf::write(&s, &options(), &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert_eq!(out.len(), 2048 + 65025);
    let decoded = read(&out, &Default::default()).unwrap();
    assert_eq!(
        decoded
            .single()
            .unwrap()
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .bitmap
            .as_bytes()
            .last(),
        Some(&1)
    );
}

#[test]
fn registry_dispatch_and_strict_metadata_loss() {
    let bytes = fixture();
    let registry = fontpixel::registry();
    let mut loaded = registry
        .read(
            &Input::new(&bytes),
            &fontpixel::ReadOptions {
                format: Some("sbf".into()),
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
    let output = registry.write(&loaded.fonts, "sbf", &options).unwrap();
    assert!(output.report.has_loss());
    options.strict = true;
    assert!(registry.write(&loaded.fonts, "sbf", &options).is_err());
}
