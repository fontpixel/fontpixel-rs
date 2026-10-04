//! Original MIT Dashen offset, strip, and mapping fixtures.
#![cfg(feature = "dashen")]
use fontpixel::formats::dashen::{self, DashenWriteOptions};
use fontpixel::model::{Codepoint, FontSet};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 1089];
    b[4] = 14;
    b[6] = 9;
    b[8] = 3;
    b[12] = 3;
    b[14..1038].fill(255);
    b[14 + 65 * 4..18 + 65 * 4].copy_from_slice(&0u32.to_le_bytes());
    b[14 + 66 * 4..18 + 66 * 4].copy_from_slice(&12u32.to_le_bytes());
    b[1070..1082].copy_from_slice(&[9, 1, 3, 128, 64, 32, 0, 0, 0, 0, 0, 128]);
    b[1082..].copy_from_slice(&[2, 0, 9, 255, 0, 128, 128]);
    b
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let mut c = ReadContext::new(&input, l)?;
    Ok(dashen::read(&mut c, &Default::default())?.0)
}
fn output() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.pft").unwrap())
}
#[test]
fn vertical_strips_and_native_roundtrip() {
    let s = read(&fixture(), &Default::default()).unwrap();
    let g = s.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    assert_eq!(g.bitmap.as_bytes(), &[128, 128, 64, 0, 32, 0]);
    assert_eq!(g.metrics.bounds.y, 0);
    let out = dashen::write(&s, &output(), &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let back = read(&out, &Default::default()).unwrap();
    assert_eq!(
        g.bitmap,
        back.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .bitmap
    );
}
#[test]
fn offsets_are_authoritative_with_aliases() {
    let mut b = fixture();
    b[14 + 65 * 4..18 + 65 * 4].copy_from_slice(&12u32.to_le_bytes());
    let s = read(&b, &Default::default()).unwrap();
    assert_eq!(
        s.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .bitmap
            .width(),
        2
    );
}
#[test]
fn codepoint_map_survives_native_write() {
    let mut b = fixture();
    b.extend(0..=255);
    b[1089 + 65] = 97;
    b[1089 + 66] = 98;
    let s = read(&b, &Default::default()).unwrap();
    assert!(s.fonts[0].glyph_id(Codepoint::Legacy(97)).is_some());
    let out = dashen::write(&s, &output(), &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert_eq!(out[1089 + 65], 97);
    assert!(
        read(&out, &Default::default()).unwrap().fonts[0]
            .glyph_id(Codepoint::Legacy(98))
            .is_some()
    );
}
#[test]
fn malformed_offsets_truncations_and_undefined_generation() {
    let b = fixture();
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "prefix{n}");
    }
    let mut b = b;
    b[8] = 1;
    assert!(read(&b, &Default::default()).is_err());
    b[8] = 3;
    b[14 + 65 * 4..18 + 65 * 4].copy_from_slice(&i32::MIN.to_le_bytes());
    assert!(read(&b, &Default::default()).is_err());
}
#[test]
fn limits_writer_same_limits_and_strict_loss() {
    let b = fixture();
    for l in [
        ResourceLimits {
            max_glyphs: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 1,
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
        assert!(read(&b, &l).is_err());
    }
    let mut s = read(&b, &Default::default()).unwrap();
    let v = DashenWriteOptions {
        limits: ResourceLimits {
            max_total_bytes: 1024 * 1024,
            ..Default::default()
        },
        ..Default::default()
    };
    let out = dashen::write(&s, &output(), &v)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert!(read(&out, &v.limits).is_ok());
    s.fonts[0].names.copyright = Some("test".into());
    let mut o = output();
    o.strict = true;
    assert!(dashen::write(&s, &o, &v).is_err());
}
#[test]
fn v4_embedded_printer_has_shared_glyph_budget() {
    let mut s = read(&fixture(), &Default::default()).unwrap();
    s.fonts.push(s.fonts[0].clone());
    let v = DashenWriteOptions {
        version: dashen::DashenVersion::V4,
        codepoint_map: true,
        ..Default::default()
    };
    let b = dashen::write(&s, &output(), &v)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let back = read(&b, &Default::default()).unwrap();
    assert_eq!(back.fonts.len(), 2);
    assert_eq!(
        back.fonts[1]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .bitmap,
        s.fonts[1]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .bitmap
    );
    assert!(
        read(
            &b,
            &ResourceLimits {
                max_glyphs: 3,
                ..Default::default()
            }
        )
        .is_err()
    );
}
