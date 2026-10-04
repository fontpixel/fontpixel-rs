//! Original byte-authored RB11/RB12 and grayscale fixtures.
#![cfg(feature = "rockbox")]
use fontpixel::bitmap::{Bitmap, Mono};
use fontpixel::formats::rockbox::{self, RockboxDepth, RockboxFlavor, RockboxWriteOptions};
use fontpixel::model::{Codepoint, Font, Glyph, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
fn p16(b: &mut [u8], p: usize, n: u16) {
    b[p..p + 2].copy_from_slice(&n.to_le_bytes());
}
fn p32(b: &mut [u8], p: usize, n: u32) {
    b[p..p + 4].copy_from_slice(&n.to_le_bytes());
}
fn fixture(old: bool) -> Vec<u8> {
    let head: usize = if old { 324 } else { 4 };
    let bytes = if old { 60 } else { 54 };
    let os = if old { 4 } else { 2 };
    let table = (head + 32 + bytes).div_ceil(os) * os;
    let mut b = vec![0; table + 3 * os + 3];
    b[..4].copy_from_slice(if old { b"RB11" } else { b"RB12" });
    if old {
        b[4..12].copy_from_slice(b"Original");
        b[68..71].copy_from_slice(b"MIT");
    }
    p16(&mut b, head, 9);
    p16(&mut b, head + 2, 10);
    p16(&mut b, head + 4, 8);
    for (at, n) in [
        (8, 65),
        (12, 65),
        (16, 3),
        (20, if old { 30 } else { 54 }),
        (24, 3),
        (28, 3),
    ] {
        p32(&mut b, head + at, n);
    }
    for g in 0..3 {
        for y in 0..10 {
            for x in 0..9 {
                if (x * 3 + y * 5 + g) % 7 < 2 {
                    if old {
                        b[head + 32 + g * 20 + y * 2 + usize::from(x < 8)] |= 0x80 >> (x % 8);
                    } else {
                        b[head + 32 + g * 18 + (y / 8) * 9 + x] |= 1 << (y % 8);
                    }
                }
            }
        }
        if old {
            p32(&mut b, table + g * 4, (g * 10) as u32);
        } else {
            p16(&mut b, table + g * 2, (g * 18) as u16);
        }
        b[table + 3 * os + g] = 9;
    }
    b
}
fn read(b: &[u8], l: &ResourceLimits) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    rockbox::read(&mut ReadContext::new(&i, l)?, &Default::default()).map(|v| v.0)
}
fn options() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.fnt").unwrap())
}
#[test]
fn rb11_rb12_width9_two_vertical_bands() {
    for old in [false, true] {
        let b = fixture(old);
        let s = read(&b, &Default::default()).unwrap();
        for (g, (_, glyph)) in s.single().unwrap().glyphs().enumerate() {
            assert_eq!(glyph.metrics.bounds.y, -2);
            for y in 0..10 {
                for x in 0..9 {
                    let bit = glyph.bitmap.as_bytes()[y * 2 + x / 8] & (0x80 >> (x % 8)) != 0;
                    assert_eq!(bit, (x * 3 + y * 5 + g) % 7 < 2);
                }
            }
        }
        let v = RockboxWriteOptions {
            flavor: if old {
                RockboxFlavor::Rb11
            } else {
                RockboxFlavor::Rb12
            },
            ..Default::default()
        };
        let mut o = options();
        o.strict = true;
        let out = rockbox::write(&s, &o, &v).unwrap();
        let again = read(&out.output.into_single().unwrap(), &Default::default()).unwrap();
        assert_eq!(s, again);
    }
}
#[test]
fn native_four_bit_low_nibble_first() {
    let mut b = vec![0; 44];
    b[..4].copy_from_slice(b"RB12");
    p16(&mut b, 4, 3);
    p16(&mut b, 6, 3);
    p16(&mut b, 8, 2);
    p16(&mut b, 10, 1);
    for (at, n) in [(12, 65), (16, 65), (20, 1), (24, 5), (28, 1), (32, 0)] {
        p32(&mut b, at, n);
    }
    b[36..41].copy_from_slice(&[0xf0, 0x87, 0x21, 0xff, 0x05]);
    let s = read(&b, &Default::default()).unwrap();
    assert_eq!(
        s.single()
            .unwrap()
            .glyphs()
            .next()
            .unwrap()
            .1
            .bitmap
            .as_bytes(),
        [255, 0, 136, 119, 238, 221, 0, 0, 170]
    );
    let mut o = options();
    o.strict = true;
    assert!(
        !rockbox::write(&s, &o, &Default::default())
            .unwrap()
            .report
            .has_loss()
    );
}
#[test]
fn large_offsets_cross_actual_rockbox_threshold() {
    let mut f = Font::default();
    f.metrics.ascent = Some(64);
    f.metrics.line_height = Some(64);
    for i in 0..33 {
        let mut bm = Bitmap::<Mono>::new(255, 64).unwrap();
        bm.set_pixel(i * 3, 63, 1).unwrap();
        let mut g = Glyph::new(bm);
        g.codepoints.push(Codepoint::Unicode(65 + i));
        g.metrics.advance = Some(Vector::new(255, 0));
        f.add_glyph(g).unwrap();
    }
    let out = rockbox::write(&f.into(), &options(), &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let bits = u32::from_le_bytes(out[24..28].try_into().unwrap()) as usize;
    assert!(bits >= 0xffdb);
    let s = read(&out, &Default::default()).unwrap();
    assert_eq!(s.single().unwrap().len(), 33);
    for i in 0..33 {
        let g = s
            .single()
            .unwrap()
            .glyph_for_codepoint(Codepoint::Unicode(65 + i))
            .unwrap();
        assert_eq!(
            g.bitmap.as_bytes()[63 * 32 + (i * 3) as usize / 8] & (0x80 >> ((i * 3) % 8)),
            0x80 >> ((i * 3) % 8)
        );
    }
}
#[test]
fn malformed_and_same_limits() {
    let b = fixture(false);
    for n in 0..b.len() {
        assert!(read(&b[..n], &Default::default()).is_err());
    }
    let mut b2 = b.clone();
    p32(&mut b2, 28, 2);
    assert!(read(&b2, &Default::default()).is_err());
    let s = read(&b, &Default::default()).unwrap();
    for total in [0, 4096, 16000, 128000] {
        let l = ResourceLimits {
            max_total_bytes: total,
            ..Default::default()
        };
        if let Ok(w) = rockbox::write(
            &s,
            &options(),
            &RockboxWriteOptions {
                limits: l.clone(),
                depth: Some(RockboxDepth::Mono),
                ..Default::default()
            },
        ) {
            assert!(read(&w.output.into_single().unwrap(), &l).is_ok());
        }
    }
}
#[test]
fn offsetless_native_cells_are_word_aligned() {
    let mut b = vec![0; 42];
    b[..4].copy_from_slice(b"RB12");
    p16(&mut b, 4, 1);
    p16(&mut b, 6, 1);
    p16(&mut b, 8, 1);
    for (at, n) in [(12, 65), (16, 65), (20, 3), (24, 6)] {
        p32(&mut b, at, n);
    }
    b[36..].copy_from_slice(&[1, 0, 0, 0, 1, 0]);
    let s = read(&b, &Default::default()).unwrap();
    for (cp, expected) in [(65, 128), (66, 0), (67, 128)] {
        assert_eq!(
            s.single()
                .unwrap()
                .glyph_for_codepoint(Codepoint::Unicode(cp))
                .unwrap()
                .bitmap
                .as_bytes(),
            [expected]
        );
    }
}

#[test]
fn registry_dispatch_and_strict_metadata_loss() {
    let bytes = fixture(false);
    let registry = fontpixel::registry();
    let mut loaded = registry
        .read(
            &Input::new(&bytes),
            &fontpixel::ReadOptions {
                format: Some("rockbox".into()),
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
    let output = registry.write(&loaded.fonts, "rockbox", &options).unwrap();
    assert!(output.report.has_loss());
    options.strict = true;
    assert!(registry.write(&loaded.fonts, "rockbox", &options).is_err());
}
