//! WSF native mono/alpha, byte/bit ordering, padding and resource limits.
#![cfg(feature = "wsfont")]
use fontpixel::formats::wsfont::*;
use fontpixel::model::{Codepoint, Glyph, PropertyValue, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{Bitmap, Font, FontSet, ResourceLimits, WriteOptions};
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    Ok(fontpixel::formats::wsfont::read(&mut ReadContext::new(&i, l)?, &Default::default())?.0)
}
fn source(alpha: bool) -> FontSet {
    let mut f = Font::default();
    f.names.family = Some("WSF École".into());
    for code in [65, 67] {
        let bm = "#.......##\n.##......#\n#####...#."
            .parse::<Bitmap>()
            .unwrap();
        let mut g = Glyph::new(if alpha {
            fontpixel::model::GlyphBitmap::Gray8(bm.to_gray8().unwrap())
        } else {
            bm.into()
        });
        g.metrics.advance = Some(Vector::new(10, 0));
        g.codepoints.push(Codepoint::Legacy(code));
        f.add_glyph(g).unwrap();
    }
    f.into()
}
fn write(s: &FontSet, o: &WsfontWriteOptions) -> Vec<u8> {
    fontpixel::formats::wsfont::write(s, &WriteOptions::new(ResourcePath::new("f").unwrap()), o)
        .unwrap()
        .output
        .into_single()
        .unwrap()
}
#[test]
fn native_depths_all_orders_and_padding() {
    for alpha in [false, true] {
        for bit in [
            WsfontOrder::Known,
            WsfontOrder::LeftToRight,
            WsfontOrder::RightToLeft,
        ] {
            for byte in [
                WsfontOrder::Known,
                WsfontOrder::LeftToRight,
                WsfontOrder::RightToLeft,
            ] {
                let s = source(alpha);
                let o = WsfontWriteOptions {
                    bit_order: Some(bit),
                    byte_order: Some(byte),
                    stride: Some(if alpha { 12 } else { 3 }),
                    ..Default::default()
                };
                let b = write(&s, &o);
                let back = read(&b, &Default::default()).unwrap();
                assert_eq!(back.fonts[0].len(), 3);
                for (_, g) in s.fonts[0].glyphs() {
                    let got = back.fonts[0].glyph_for_codepoint(g.codepoints[0]).unwrap();
                    assert_eq!(g.bitmap, got.bitmap);
                    assert_eq!(g.metrics, got.metrics);
                }
                assert_eq!(b, write(&back, &Default::default()));
            }
        }
    }
}
#[test]
fn preserve_nonzero_unused_bits_bytes_and_name_tail() {
    let b = write(
        &source(false),
        &WsfontWriteOptions {
            stride: Some(3),
            ..Default::default()
        },
    );
    let mut f = read(&b, &Default::default()).unwrap();
    let id = f.fonts[0].glyphs().next().unwrap().0;
    f.fonts[0]
        .edit_glyph(id, |g| {
            g.properties.set(
                "wsfont.padding",
                PropertyValue::Bytes(vec![0x35, 0xaa, 0x2a, 0xbb, 0x11, 0xcc]),
            );
            Ok(())
        })
        .unwrap();
    let mut name = vec![0; 64];
    name[..5].copy_from_slice(b"name\0");
    name[63] = 77;
    f.fonts[0].names.family = Some("name".into());
    f.fonts[0]
        .properties
        .set("wsfont.name_field", PropertyValue::Bytes(name));
    let out = write(&f, &Default::default());
    let back = read(&out, &Default::default()).unwrap();
    assert_eq!(out[67], 77);
    assert_eq!(out, write(&back, &Default::default()));
}
#[test]
fn malformed_limits_and_gray_permission() {
    let b = write(&source(false), &Default::default());
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "prefix {n}");
    }
    assert!(
        read(
            &b,
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
            &ResourceLimits {
                max_pixels_per_glyph: 29,
                ..Default::default()
            }
        )
        .is_err()
    );
    for (at, v) in [(80, 0), (84, 0), (88, 1), (92, 3), (96, 3)] {
        let mut bad = b.clone();
        bad[at] = v;
        assert!(read(&bad, &Default::default()).is_err());
    }
    let gray = source(true);
    let w = WriteOptions::new(ResourcePath::new("f").unwrap());
    assert!(
        fontpixel::formats::wsfont::write(
            &gray,
            &w,
            &WsfontWriteOptions {
                depth: Some(WsfontDepth::Mono),
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        fontpixel::formats::wsfont::write(
            &gray,
            &w,
            &WsfontWriteOptions {
                depth: Some(WsfontDepth::Mono),
                gray_threshold: Some(128),
                ..Default::default()
            }
        )
        .is_ok()
    );
}
