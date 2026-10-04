//! Original MGTK wire fixtures generated from the public toolkit record.
#![cfg(feature = "mgtk")]
use fontpixel::formats::mgtk::{self, Columns, MgtkReadOptions, MgtkWriteOptions};
use fontpixel::model::{Codepoint, Property, Vector};
use fontpixel::{Bitmap, FontSet, ResourceLimits};
fn wire() -> Vec<u8> {
    vec![0x80, 1, 2, 7, 10, 0x41, 0x2a, 0, 5, 0x22, 0x55, 0, 2]
}
fn read(data: &[u8]) -> FontSet {
    mgtk::read(data, &Default::default(), &Default::default())
        .unwrap()
        .0
}
#[test]
fn seven_bit_reversal_and_separate_strips_are_native() {
    let set = read(&wire());
    let f = &set.fonts[0];
    assert_eq!(f.len(), 2);
    assert_eq!(
        f.glyph_for_codepoint(Codepoint::Legacy(0))
            .unwrap()
            .bitmap
            .as_bytes(),
        [0x82, 0x44]
    );
    let g = f.glyph_for_codepoint(Codepoint::Legacy(1)).unwrap();
    assert_eq!(g.bitmap.as_bytes(), [0x55, 0x40, 0xaa, 0x80]);
    assert_eq!(g.metrics.advance, Some(Vector::new(10, 0)));
    let (out, r) = mgtk::write(&set, &Default::default()).unwrap();
    assert_eq!(out, wire());
    assert!(!r.has_loss(), "{r}");
}
#[test]
fn ignored_high_bits_and_trailing_bytes_are_preserved() {
    let mut data = wire();
    data[5] |= 0x80;
    data.extend([0x19, 0x90]);
    let set = read(&data);
    let (out, r) = mgtk::write(&set, &Default::default()).unwrap();
    assert_eq!(out, data);
    assert!(!r.has_loss());
    let mut edited = set;
    edited.fonts[0].properties.remove("mgtk.source");
    let (canonical, _) = mgtk::write(&edited, &Default::default()).unwrap();
    assert_eq!(canonical, wire());
}
#[test]
fn zero_width_glyph_has_native_height_and_bounded_empty_raster() {
    let mut data = vec![0, 0, 16, 0];
    data.extend([0; 16]);
    let set = read(&data);
    let glyph = set.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(0))
        .unwrap();
    assert_eq!((glyph.bitmap.width(), glyph.bitmap.height()), (0, 16));
    assert_eq!(mgtk::write(&set, &Default::default()).unwrap().0, data);
}
#[test]
fn all_truncations_invalid_dimensions_and_strip_options() {
    for end in 0..wire().len() {
        assert!(mgtk::read(&wire()[..end], &Default::default(), &Default::default()).is_err());
    }
    for (at, v) in [(0, 1), (2, 0), (2, 17), (3, 15)] {
        let mut data = wire();
        data[at] = v;
        assert!(mgtk::read(&data, &Default::default(), &Default::default()).is_err());
    }
    assert!(
        mgtk::write(
            &read(&wire()),
            &MgtkWriteOptions {
                columns: Columns::Single,
                ..Default::default()
            }
        )
        .is_err()
    );
    let mut data = vec![0, 0, 1, 7, 0x41];
    let set = read(&data);
    let (out, r) = mgtk::write(
        &set,
        &MgtkWriteOptions {
            columns: Columns::Double,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(out, [0x80, 0, 1, 7, 0x41, 0]);
    assert!(r.has_loss());
    data[4] = 0xff;
    assert_eq!(
        read(&data).fonts[0]
            .glyphs()
            .next()
            .unwrap()
            .1
            .bitmap
            .as_bytes(),
        [0xfe]
    );
}
#[test]
fn geometry_normalization_and_metadata_losses_are_explicit() {
    let mut set = read(&wire());
    let id = set.fonts[0].glyph_id(Codepoint::Legacy(0)).unwrap();
    set.fonts[0]
        .edit_glyph(id, |g| {
            g.metrics.bounds.x = -1;
            g.name = Some("named".into());
            g.metrics.vertical_advance = Some(Vector::new(0, 2));
            Ok(())
        })
        .unwrap();
    set.fonts[0].names.license = Some("original".into());
    set.properties.push(Property::new("collection", "x"));
    let (out, r) = mgtk::write(&set, &Default::default()).unwrap();
    for code in [
        "mgtk.names",
        "mgtk.glyphs",
        "mgtk.collection_metadata",
        "mgtk.properties",
    ] {
        assert!(r.entries.iter().any(|e| e.code == code), "{code}: {r}");
    }
    assert_eq!(
        read(&out).fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(0))
            .unwrap()
            .bitmap
            .width(),
        8
    );
    let mut opts = fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("out.fnt").unwrap());
    opts.strict = true;
    assert!(matches!(
        fontpixel::write(&set, "mgtk", &opts),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn explicit_gray_threshold_charset_and_limits() {
    let mut set = read(&wire());
    let id = set.fonts[0].glyph_id(Codepoint::Legacy(0)).unwrap();
    set.fonts[0]
        .edit_glyph(id, |g| {
            g.bitmap = Bitmap::<fontpixel::bitmap::Gray8>::from_raw(
                7,
                2,
                &[127, 128, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                Default::default(),
            )?
            .into();
            Ok(())
        })
        .unwrap();
    assert!(mgtk::write(&set, &Default::default()).is_err());
    let (out, r) = mgtk::write(
        &set,
        &MgtkWriteOptions {
            gray_threshold: Some(128),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(r.has_loss());
    assert_eq!(
        read(&out).fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(0))
            .unwrap()
            .bitmap
            .as_bytes(),
        [0x50, 0]
    );
    let (set, _) = mgtk::read(
        &wire(),
        &MgtkReadOptions {
            charset: Some("ascii".into()),
        },
        &Default::default(),
    )
    .unwrap();
    assert!(mgtk::write(&set, &Default::default()).unwrap().1.has_loss());
    for limits in [
        ResourceLimits {
            max_glyphs: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: 100,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 10,
            ..Default::default()
        },
        ResourceLimits {
            max_input_bytes: 10,
            ..Default::default()
        },
    ] {
        assert!(mgtk::read(&wire(), &Default::default(), &limits).is_err());
        assert!(
            mgtk::write(
                &set,
                &MgtkWriteOptions {
                    limits,
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
}

#[test]
fn trailing_removed_glyph_slot_is_a_model_loss() {
    let (mut set, _) =
        mgtk::read(&[0, 0, 1, 1, 1], &Default::default(), &Default::default()).unwrap();
    let id = set.fonts[0]
        .add_glyph(fontpixel::model::Glyph::default())
        .unwrap();
    set.fonts[0].remove_glyph(id);
    assert!(mgtk::write(&set, &Default::default()).unwrap().1.has_loss());
}
