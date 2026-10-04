//! Native VFNT normal/bold half-cell fallback and edited pool reconstruction.
#![cfg(feature = "vtfont")]
use fontpixel::formats::vtfont::*;
use fontpixel::model::{Codepoint, GlyphBitmap};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
fn fixture() -> Vec<u8> {
    let mut b = vec![0; 32];
    b[..8].copy_from_slice(b"VFNT0002");
    b[8] = 5;
    b[9] = 2;
    b[12..16].copy_from_slice(&6u32.to_be_bytes());
    for (i, n) in [1u32, 1, 2, 2].iter().enumerate() {
        b[16 + 4 * i..20 + 4 * i].copy_from_slice(&n.to_be_bytes());
    }
    b.extend_from_slice(&[
        0x80, 0x40, 0xf8, 0x88, 0x40, 0xe0, 0xa8, 0xd8, 0x70, 0x98, 0x18, 0xc0,
    ]);
    for (src, dst, len) in [
        (65u32, 1u16, 1u16),
        (65, 2, 0),
        (65, 3, 0),
        (66, 0, 0),
        (65, 0, 0),
        (66, 4, 0),
    ] {
        b.extend_from_slice(&src.to_be_bytes());
        b.extend_from_slice(&dst.to_be_bytes());
        b.extend_from_slice(&len.to_be_bytes());
    }
    b
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    Ok(fontpixel::formats::vtfont::read(&mut ReadContext::new(&i, l)?, &Default::default())?.0)
}
fn write(s: &FontSet, strict: bool, o: &VtfontWriteOptions) -> fontpixel::Result<Vec<u8>> {
    let mut w = WriteOptions::new(ResourcePath::new("f").unwrap());
    w.strict = strict;
    fontpixel::formats::vtfont::write(s, &w, o)?
        .output
        .into_single()
}
fn rows(s: &FontSet, font: usize, code: u32) -> String {
    let g = s.fonts[font]
        .glyph_for_codepoint(Codepoint::Unicode(code))
        .unwrap();
    match &g.bitmap {
        GlyphBitmap::Mono(b) => (0..b.height())
            .map(|y| {
                (0..b.width())
                    .map(|x| {
                        if b.get_pixel(x, y).unwrap() != 0 {
                            '#'
                        } else {
                            '.'
                        }
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => panic!("expected mono"),
    }
}
#[test]
fn native_independent_fallback_and_lossless_rewrite() {
    let b = fixture();
    let s = read(&b, &Default::default()).unwrap();
    assert_eq!(s.fonts.len(), 2);
    assert_eq!(rows(&s, 0, 65), "#####.#...\n#...####..");
    assert_eq!(rows(&s, 1, 65), "#.#.#.#...\n##.#####..");
    assert_eq!(rows(&s, 0, 66), ".#...\n###..");
    assert_eq!(rows(&s, 1, 66), ".#....###.\n###..#..##");
    assert!(
        s.fonts[0]
            .glyphs()
            .any(|(_, g)| g.properties.get("vtfont.unused_index").is_some())
    );
    assert_eq!(b, write(&s, true, &Default::default()).unwrap());
}
#[test]
fn edits_rebuild_native_rasters_and_preserve_effective_ink() {
    let b = fixture();
    let mut s = read(&b, &Default::default()).unwrap();
    let id = s.fonts[1].glyph_id(Codepoint::Unicode(65)).unwrap();
    s.fonts[1]
        .edit_glyph(id, |g| {
            if let GlyphBitmap::Mono(b) = &mut g.bitmap {
                b.set_pixel(0, 0, 0)?;
            }
            Ok(())
        })
        .unwrap();
    let out = write(&s, false, &Default::default()).unwrap();
    assert_ne!(b, out);
    let back = read(&out, &Default::default()).unwrap();
    for font in 0..2 {
        for cp in [65, 66] {
            assert_eq!(rows(&s, font, cp), rows(&back, font, cp));
        }
    }
    assert!(write(&s, true, &Default::default()).is_err());
}
#[test]
fn malformed_mappings_and_budget() {
    let b = fixture();
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err(), "prefix {n}");
    }
    assert!(
        read(
            &b,
            &ResourceLimits {
                max_glyphs: 5,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        read(
            &b,
            &ResourceLimits {
                max_pixels_per_glyph: 19,
                ..Default::default()
            }
        )
        .is_err()
    );
    for (at, data) in [
        (8, vec![0]),
        (12, vec![0, 0, 0, 0]),
        (44, 0xd800u32.to_be_bytes().to_vec()),
        (48, 65535u16.to_be_bytes().to_vec()),
    ] {
        let mut bad = b.clone();
        bad[at..at + data.len()].copy_from_slice(&data);
        assert!(read(&bad, &Default::default()).is_err());
    }
}
