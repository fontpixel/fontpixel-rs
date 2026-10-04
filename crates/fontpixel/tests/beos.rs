//! BeOS native eight-level coverage, sparse Unicode aliases and IEEE-754 metrics.
#![cfg(feature = "beos")]
use fontpixel::bitmap::{Bitmap, BitmapLimits, Gray8};
use fontpixel::formats::beos::*;
use fontpixel::model::{Codepoint, Glyph, PropertyValue, Rational, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{Font, FontSet, ResourceLimits, WriteOptions};
fn read(b: &[u8], o: &BeosReadOptions, l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    Ok(fontpixel::formats::beos::read(&mut ReadContext::new(&i, l)?, o)?.0)
}
fn source() -> FontSet {
    let mut f = Font::default();
    f.names.family = Some("École".into());
    f.names.style = Some("Regular".into());
    f.metrics.point_size = Some(Rational::integer(13));
    let mut g = Glyph::new(
        Bitmap::<Gray8>::from_raw(3, 2, &[0, 36, 73, 109, 146, 255], BitmapLimits::default())
            .unwrap(),
    );
    g.codepoints = vec![Codepoint::Unicode(65), Codepoint::Unicode(0x4e00)];
    g.metrics.bounds.x = -1;
    g.metrics.bounds.y = -2;
    g.metrics.advance = Some(Vector::new(4, 0));
    f.add_glyph(g).unwrap();
    let mut g = Glyph::new(Bitmap::<Gray8>::default());
    g.metrics.advance = Some(Vector::new(2, 0));
    g.codepoints.push(Codepoint::Unicode(32));
    f.add_glyph(g).unwrap();
    let mut g =
        Glyph::new(Bitmap::<Gray8>::from_raw(1, 1, &[219], BitmapLimits::default()).unwrap());
    g.metrics.advance = Some(Vector::new(1, 0));
    f.add_glyph(g).unwrap();
    f.into()
}
fn write(s: &FontSet, o: &BeosWriteOptions) -> fontpixel::Result<Vec<u8>> {
    fontpixel::formats::beos::write(s, &WriteOptions::new(ResourcePath::new("f").unwrap()), o)?
        .output
        .into_single()
}
#[test]
fn native_palette_unicode_aliases_unencoded_and_empty() {
    let s = source();
    let b = write(&s, &Default::default()).unwrap();
    let back = read(&b, &Default::default(), &Default::default()).unwrap();
    assert_eq!(back.fonts[0].len(), 3);
    for ((_, a), (_, g)) in s.fonts[0].glyphs().zip(back.fonts[0].glyphs()) {
        assert_eq!(a.bitmap, g.bitmap);
        assert_eq!(a.metrics, g.metrics);
        let mut labels = a.codepoints.clone();
        let mut got = g.codepoints.clone();
        labels.sort();
        got.sort();
        assert_eq!(labels, got);
    }
    assert_eq!(b, write(&back, &Default::default()).unwrap());
    let start = 38 + 5 + 7 + 4 * 8;
    assert_eq!(&b[start + 24..start + 28], &[0x01, 0x20, 0x34, 0x70]);
}
#[test]
fn fractional_projection_explicit_and_exact_preservation() {
    let mut s = source();
    let id = s.fonts[0].glyphs().next().unwrap().0;
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.properties.set(
                "beos.advance_bits",
                PropertyValue::Unsigned(u64::from(4.5f32.to_bits())),
            );
            Ok(())
        })
        .unwrap();
    assert!(write(&s, &Default::default()).is_err());
    let b = write(
        &s,
        &BeosWriteOptions {
            round_fractional_metrics: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(read(&b, &Default::default(), &Default::default()).is_err());
    let back = read(
        &b,
        &BeosReadOptions {
            round_fractional_metrics: true,
            ..Default::default()
        },
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        back.fonts[0]
            .glyphs()
            .next()
            .unwrap()
            .1
            .properties
            .get("beos.advance_bits"),
        Some(&PropertyValue::Unsigned(u64::from(4.5f32.to_bits())))
    );
}
#[test]
fn quantization_and_sixteen_level_compatibility() {
    let mut s = source();
    let id = s.fonts[0].glyphs().next().unwrap().0;
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.bitmap = Bitmap::<Gray8>::from_raw(
                3,
                2,
                &[0, 17, 34, 85, 170, 255],
                BitmapLimits::default(),
            )?
            .into();
            Ok(())
        })
        .unwrap();
    assert!(write(&s, &Default::default()).is_err());
    assert!(
        write(
            &s,
            &BeosWriteOptions {
                quantize_gray: true,
                ..Default::default()
            }
        )
        .is_ok()
    );
    let b = write(
        &s,
        &BeosWriteOptions {
            coverage: Some(BeosCoverage::FullNibble),
            quantize_gray: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(read(&b, &Default::default(), &Default::default()).is_err());
    assert!(
        read(
            &b,
            &BeosReadOptions {
                coverage: BeosCoverage::FullNibble,
                ..Default::default()
            },
            &Default::default()
        )
        .is_ok()
    );
}
#[test]
fn malformed_size_hash_pointer_float_and_limits() {
    let b = write(&source(), &Default::default()).unwrap();
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default(), &Default::default()).is_err());
    }
    assert!(
        read(
            &b,
            &Default::default(),
            &ResourceLimits {
                max_glyphs: 2,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        read(
            &b,
            &Default::default(),
            &ResourceLimits {
                max_pixels_per_glyph: 5,
                ..Default::default()
            }
        )
        .is_err()
    );
    let table = 38 + 5 + 7;
    let start = table + 32;
    let mut bad = b.clone();
    bad[start + 16..start + 20].copy_from_slice(&f32::NAN.to_bits().to_be_bytes());
    assert!(read(&bad, &Default::default(), &Default::default()).is_err());
    let mut bad = b.clone();
    for p in (table..start).step_by(8) {
        if bad[p..p + 4] != [255; 4] {
            bad[p..p + 4].copy_from_slice(&((start + 1) as u32).to_be_bytes());
            break;
        }
    }
    assert!(read(&bad, &Default::default(), &Default::default()).is_err());
}
