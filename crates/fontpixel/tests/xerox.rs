//! Original MIT Xerox native families and primary-spec edge cases.
#![cfg(feature = "xerox")]
use fontpixel::formats::xerox::{
    self, XeroxBitbltLayout, XeroxReadOptions, XeroxVariant, XeroxWriteOptions,
};
use fontpixel::model::{Codepoint, FontSet, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, WriteOptions};
fn p16(b: &mut [u8], at: usize, n: u16) {
    b[at..at + 2].copy_from_slice(&n.to_be_bytes());
}
fn p32(b: &mut [u8], at: usize, n: u32) {
    b[at..at + 4].copy_from_slice(&n.to_be_bytes());
}
fn alto() -> Vec<u8> {
    let mut b = vec![0; 152];
    p16(&mut b, 0, 5);
    b[2] = 132;
    b[3] = 10;
    for cp in 0..66 {
        p16(&mut b, 4 + cp * 2, ((138 - 4 - cp * 2) / 2) as u16);
    }
    p16(&mut b, 138, 1);
    p16(&mut b, 134, 7);
    b[142..148].copy_from_slice(&[128, 128, 64, 0, 255, 128]);
    p16(&mut b, 148, 21);
    b[150] = 1;
    b[151] = 3;
    b
}
fn strike() -> Vec<u8> {
    let mut b = vec![0; 30];
    for (at, n) in [
        (0, 0x8000),
        (2, 65),
        (4, 65),
        (6, 9),
        (8, 11),
        (10, 2),
        (12, 1),
        (16, 1),
        (24, 0),
        (26, 9),
        (28, 10),
    ] {
        p16(&mut b, at, n);
    }
    b[18..24].copy_from_slice(&[128, 192, 64, 64, 255, 192]);
    b
}
fn ac() -> Vec<u8> {
    let mut b = vec![0; 76];
    p16(&mut b, 0, 0x100c);
    p16(&mut b, 2, 1);
    b[4] = 4;
    b[5..9].copy_from_slice(b"TEST");
    p16(&mut b, 24, 0x300b);
    b[26] = 1;
    b[28] = 65;
    b[29] = 65;
    p16(&mut b, 30, 423);
    p32(&mut b, 34, 24);
    p32(&mut b, 38, 14);
    p16(&mut b, 42, 720);
    p16(&mut b, 44, 720);
    p16(&mut b, 46, 1);
    p32(&mut b, 48, 4 * 65536);
    p32(&mut b, 52, (-65536i32) as u32);
    p16(&mut b, 56, 65535);
    p16(&mut b, 58, 65535);
    p16(&mut b, 60, 3);
    p16(&mut b, 62, 3);
    p32(&mut b, 64, 2);
    p16(&mut b, 68, 0x0403);
    b[70..].copy_from_slice(&[0xa0, 0, 0xc0, 0, 0x80, 0]);
    b
}
fn read(b: &[u8], l: &ResourceLimits, variant: XeroxVariant) -> fontpixel::Result<FontSet> {
    let i = Input::new(b);
    let mut c = ReadContext::new(&i, l)?;
    Ok(xerox::read(
        &mut c,
        &XeroxReadOptions {
            variant: Some(variant),
            ..Default::default()
        },
    )?
    .0)
}
fn output() -> WriteOptions {
    WriteOptions::new(ResourcePath::new("font.xerox").unwrap())
}
#[test]
fn alto_primary_baseline_formula_and_extensions() {
    let mut s = read(&alto(), &Default::default(), XeroxVariant::Alto).unwrap();
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(65)).unwrap();
    let g = s.fonts[0].glyph(id).unwrap();
    assert_eq!(g.metrics.bounds.y, 0);
    assert_eq!(g.metrics.advance, Some(Vector::new(10, 0)));
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.bitmap = fontpixel::bitmap::Bitmap::<fontpixel::bitmap::Mono>::from_raw(
                32,
                3,
                &[128, 0, 0, 1, 64, 0, 1, 0, 255, 255, 255, 255],
                Default::default(),
            )?
            .into();
            g.metrics.bounds.width = 32;
            g.metrics.advance = Some(Vector::new(30, 0));
            Ok(())
        })
        .unwrap();
    let v = XeroxWriteOptions {
        variant: XeroxVariant::Alto,
        ..Default::default()
    };
    let out = xerox::write(&s, &output(), &v)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let back = read(&out, &Default::default(), XeroxVariant::Alto).unwrap();
    assert_eq!(
        s.fonts[0].glyph(id).unwrap().bitmap,
        back.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .bitmap
    );
    assert_eq!(
        back.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .metrics
            .advance,
        Some(Vector::new(30, 0))
    );
    let mut bad = alto();
    p16(&mut bad, 148, 130);
    assert!(read(&bad, &Default::default(), XeroxVariant::Alto).is_err());
}
#[test]
fn all_four_bitblt_layouts_and_dummy() {
    let s = read(&strike(), &Default::default(), XeroxVariant::Bitblt).unwrap();
    assert_eq!(s.fonts[0].len(), 2);
    assert!(s.fonts[0].metrics.default_glyph.is_some());
    let g = s.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    for layout in [
        XeroxBitbltLayout::Plain,
        XeroxBitbltLayout::Kerned,
        XeroxBitbltLayout::PlainIndex,
        XeroxBitbltLayout::KernedIndex,
    ] {
        let v = XeroxWriteOptions {
            bitblt_layout: layout,
            ..Default::default()
        };
        let out = xerox::write(&s, &output(), &v)
            .unwrap()
            .output
            .into_single()
            .unwrap();
        let back = read(&out, &Default::default(), XeroxVariant::Bitblt).unwrap();
        let h = back.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap();
        assert_eq!(g.bitmap, h.bitmap, "{layout:?}");
        assert_eq!(g.metrics.bounds, h.metrics.bounds);
        assert_eq!(g.metrics.advance, h.metrics.advance);
        assert!(back.fonts[0].metrics.default_glyph.is_some());
    }
}
#[test]
fn unsigned_kerned_spacing_and_negative_font_offset() {
    let mut s = read(&strike(), &Default::default(), XeroxVariant::Bitblt).unwrap();
    let id = s.fonts[0].glyph_id(Codepoint::Legacy(65)).unwrap();
    s.fonts[0]
        .edit_glyph(id, |g| {
            g.metrics.bounds.x = 130;
            g.metrics.advance = Some(Vector::new(200, 0));
            Ok(())
        })
        .unwrap();
    let out = xerox::write(&s, &output(), &Default::default())
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let back = read(&out, &Default::default(), XeroxVariant::Bitblt).unwrap();
    let g = back.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    assert_eq!(g.metrics.bounds.x, 130);
    assert_eq!(g.metrics.advance, Some(Vector::new(200, 0)));
}
#[test]
fn ac_signed_vectors_true_height_and_multiple_segments() {
    let mut s = read(&ac(), &Default::default(), XeroxVariant::Prepress).unwrap();
    let g = s.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap();
    assert_eq!(g.bitmap.height(), 3);
    assert_eq!(g.bitmap.as_bytes(), &[128, 64, 224]);
    assert_eq!(g.metrics.advance, Some(Vector::new(4, -1)));
    s.fonts.push(s.fonts[0].clone());
    s.fonts[1].names.family = Some("SECOND".into());
    let v = XeroxWriteOptions {
        variant: XeroxVariant::Prepress,
        ..Default::default()
    };
    let out = xerox::write(&s, &output(), &v)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    let back = read(&out, &Default::default(), XeroxVariant::Prepress).unwrap();
    assert_eq!(back.fonts.len(), 2);
    assert_eq!(back.fonts[1].names.family.as_deref(), Some("SECOND"));
    assert_eq!(
        back.fonts[0]
            .glyph_for_codepoint(Codepoint::Legacy(65))
            .unwrap()
            .bitmap
            .as_bytes(),
        &[128, 64, 224]
    );
}
#[test]
fn ac_rotation_and_fractional_metrics_report_loss() {
    let mut b = ac();
    p16(&mut b, 32, 5400);
    p32(&mut b, 48, 4 * 65536 + 32768);
    let i = Input::new(&b);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&i, &l).unwrap();
    let (s, r) = xerox::read(
        &mut c,
        &XeroxReadOptions {
            variant: Some(XeroxVariant::Prepress),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(r.entries.iter().any(|e| e.code == "xerox.ac_rotation"));
    assert!(
        r.entries
            .iter()
            .any(|e| e.code == "xerox.ac_fractional_width")
    );
    let v = XeroxWriteOptions {
        variant: XeroxVariant::Prepress,
        ..Default::default()
    };
    let w = xerox::write(&s, &output(), &v).unwrap();
    let out = w.output.into_single().unwrap();
    assert_eq!(&out[32..34], &5400u16.to_be_bytes());
    let mut o = output();
    o.strict = true;
    assert!(xerox::write(&s, &o, &v).is_err());
}
#[test]
fn truncation_limits_and_output_readability() {
    for (variant, b) in [
        (XeroxVariant::Alto, alto()),
        (XeroxVariant::Bitblt, strike()),
        (XeroxVariant::Prepress, ac()),
    ] {
        for n in 0..b.len() {
            assert!(
                read(&b[..n], &Default::default(), variant).is_err(),
                "{variant:?} prefix{n}"
            );
        }
        for l in [
            ResourceLimits {
                max_glyphs: 0,
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
            assert!(read(&b, &l, variant).is_err());
        }
        let s = read(&b, &Default::default(), variant).unwrap();
        let v = XeroxWriteOptions {
            variant,
            limits: ResourceLimits {
                max_total_bytes: 1024 * 1024,
                ..Default::default()
            },
            ..Default::default()
        };
        let out = xerox::write(&s, &output(), &v)
            .unwrap()
            .output
            .into_single()
            .unwrap();
        assert!(read(&out, &v.limits, variant).is_ok());
    }
}
#[test]
fn index_bad_body_reference_and_exact_plain_strict() {
    let s = read(&strike(), &Default::default(), XeroxVariant::Bitblt).unwrap();
    let mut o = output();
    o.strict = true;
    let v = XeroxWriteOptions {
        bitblt_layout: XeroxBitbltLayout::Plain,
        ..Default::default()
    };
    assert!(xerox::write(&s, &o, &v).is_ok());
    let mut out = xerox::write(
        &s,
        &output(),
        &XeroxWriteOptions {
            bitblt_layout: XeroxBitbltLayout::PlainIndex,
            ..Default::default()
        },
    )
    .unwrap()
    .output
    .into_single()
    .unwrap();
    p16(&mut out, 14, 0x7f00);
    assert!(read(&out, &Default::default(), XeroxVariant::Bitblt).is_err());
}
#[test]
fn indexed_strikes_cross_body_boundary() {
    let mut s = read(&strike(), &Default::default(), XeroxVariant::Bitblt).unwrap();
    let original = s.fonts[0]
        .glyph_for_codepoint(Codepoint::Legacy(65))
        .unwrap()
        .clone();
    for cp in 0..300 {
        if cp != 65 {
            let mut g = original.clone();
            g.codepoints = vec![Codepoint::Legacy(cp)];
            s.fonts[0].add_glyph(g).unwrap();
        }
    }
    for bitblt_layout in [
        XeroxBitbltLayout::PlainIndex,
        XeroxBitbltLayout::KernedIndex,
    ] {
        let b = xerox::write(
            &s,
            &output(),
            &XeroxWriteOptions {
                bitblt_layout,
                ..Default::default()
            },
        )
        .unwrap()
        .output
        .into_single()
        .unwrap();
        let back = read(&b, &Default::default(), XeroxVariant::Bitblt).unwrap();
        assert_eq!(back.fonts[0].len(), 301);
        for cp in [0, 255, 256, 299] {
            let g = back.fonts[0]
                .glyph_for_codepoint(Codepoint::Legacy(cp))
                .unwrap();
            assert_eq!(g.bitmap, original.bitmap);
            assert_eq!(g.metrics.advance, original.metrics.advance);
        }
    }
}
#[test]
fn detector_validates_declared_strike_end_and_native_index_tables() {
    use fontpixel::formats::Confidence;
    assert_eq!(xerox::detect(&ac()), Confidence::Possible);
    assert_eq!(xerox::detect(&alto()), Confidence::NoMatch);
    let s = read(&strike(), &Default::default(), XeroxVariant::Bitblt).unwrap();
    for bitblt_layout in [
        XeroxBitbltLayout::Plain,
        XeroxBitbltLayout::Kerned,
        XeroxBitbltLayout::PlainIndex,
        XeroxBitbltLayout::KernedIndex,
    ] {
        let mut b = xerox::write(
            &s,
            &output(),
            &XeroxWriteOptions {
                bitblt_layout,
                ..Default::default()
            },
        )
        .unwrap()
        .output
        .into_single()
        .unwrap();
        assert_eq!(xerox::detect(&b), Confidence::Possible);
        b.extend([0; 16]);
        assert_eq!(xerox::detect(&b), Confidence::NoMatch);
    }
    let mut palm = vec![0; 128];
    p16(&mut palm, 0, 0x9000);
    p16(&mut palm, 2, 0);
    p16(&mut palm, 4, 0);
    assert_eq!(xerox::detect(&palm), Confidence::NoMatch);
}
