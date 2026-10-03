//! Embedded bitmap table and native registry regressions.
#![cfg(feature = "sfnt")]
use fontpixel::sfnt::bitmap::{
    self as sbit, Bearing, BitmapGlyph, Kind, Limits, LineMetrics, Metrics, Strike,
};
use fontpixel::{Bitmap, model::GlyphBitmap};
use std::collections::BTreeMap;
fn glyph() -> BitmapGlyph {
    BitmapGlyph {
        metrics: Metrics {
            width: 3,
            height: 2,
            horizontal: Some(Bearing {
                x: -1,
                y: 1,
                advance: 4,
            }),
            vertical: None,
        },
        bitmap: GlyphBitmap::Mono("#.#\n.#.".parse().unwrap()),
    }
}
fn strike() -> Strike {
    Strike {
        ppem_x: 14,
        ppem_y: 14,
        bit_depth: 1,
        flags: 1,
        horizontal: LineMetrics([11, 253, 3, 1, 0, 0, 0, 0, 0, 0, 0, 0]),
        vertical: LineMetrics::default(),
        glyphs: BTreeMap::from([(1, glyph())]),
    }
}
fn p16(b: &mut Vec<u8>, n: u16) {
    b.extend(n.to_be_bytes());
}
fn p32(b: &mut Vec<u8>, n: u32) {
    b.extend(n.to_be_bytes());
}
fn raw(index: u16, image: u16, depth: u8, payload: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let mut data = vec![0, 2, 0, 0];
    data.extend(payload);
    let mut sub = Vec::new();
    p16(&mut sub, index);
    p16(&mut sub, image);
    p32(&mut sub, 4);
    match index {
        1 => {
            p32(&mut sub, 0);
            p32(&mut sub, payload.len() as u32);
        }
        2 => {
            p32(&mut sub, payload.len() as u32);
            sub.extend([2, 3, 255, 1, 4, 254, 2, 5]);
        }
        3 => {
            p16(&mut sub, 0);
            p16(&mut sub, payload.len() as u16);
        }
        4 => {
            p32(&mut sub, 1);
            p16(&mut sub, 1);
            p16(&mut sub, 0);
            p16(&mut sub, 2);
            p16(&mut sub, payload.len() as u16);
        }
        5 => {
            p32(&mut sub, payload.len() as u32);
            sub.extend([2, 3, 255, 1, 4, 254, 2, 5]);
            p32(&mut sub, 1);
            p16(&mut sub, 1);
        }
        _ => unreachable!(),
    }
    let mut loc = vec![0, 2, 0, 0, 0, 0, 0, 1];
    p32(&mut loc, 56);
    p32(&mut loc, 8 + sub.len() as u32);
    p32(&mut loc, 1);
    p32(&mut loc, 0);
    loc.extend([11, 253, 3, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
    loc.extend([0; 12]);
    p16(&mut loc, 1);
    p16(&mut loc, 1);
    loc.extend([14, 14, depth, 1]);
    p16(&mut loc, 1);
    p16(&mut loc, 1);
    p32(&mut loc, 8);
    loc.extend(sub);
    (loc, data)
}
#[test]
fn every_valid_index_and_direct_image_format() {
    let limits = Limits::default();
    for index in [1, 3, 4] {
        for image in [1, 2, 6, 7] {
            let mut payload = if image >= 6 {
                vec![2, 3, 255, 1, 4, 254, 2, 5]
            } else {
                vec![2, 3, 255, 1, 4]
            };
            payload.extend(if [1, 6].contains(&image) {
                vec![0xa0, 0x40]
            } else {
                vec![0xa8]
            });
            let (l, d) = raw(index, image, 1, &payload);
            let s = sbit::decode(&l, &d, Kind::OpenType, 3, &limits).unwrap();
            assert_eq!(s[0].glyphs[&1].bitmap, glyph().bitmap);
            assert_eq!(
                s[0].glyphs[&1].metrics.horizontal,
                glyph().metrics.horizontal
            );
            assert_eq!(s[0].glyphs[&1].metrics.vertical.is_some(), image >= 6);
        }
    }
    for index in [2, 5] {
        let (l, d) = raw(index, 5, 1, &[0xa8]);
        let s = sbit::decode(&l, &d, Kind::OpenType, 3, &limits).unwrap();
        assert_eq!(s[0].glyphs[&1].bitmap, glyph().bitmap);
    }
}
#[test]
fn exact_gray_depths_and_vertical_small_metrics() {
    for (depth, pixels) in [
        (2, vec![0x18, 0xe4]),
        (4, vec![0x05, 0xa0, 0xfa, 0x50]),
        (8, vec![0, 85, 170, 255, 170, 85]),
    ] {
        let mut data = vec![2, 3, 1, 255, 5];
        data.extend(pixels);
        let (l, d) = raw(1, 1, depth, &data);
        let s = sbit::decode(&l, &d, Kind::OpenType, 3, &Limits::default()).unwrap();
        assert_eq!(
            s[0].glyphs[&1].bitmap.as_bytes(),
            &[0, 85, 170, 255, 170, 85]
        );
        let mut vertical = l;
        vertical[47 + 8] = 2;
        let s = sbit::decode(&vertical, &d, Kind::Apple, 3, &Limits::default()).unwrap();
        assert_eq!(
            s[0].glyphs[&1].metrics.vertical,
            Some(Bearing {
                x: -1,
                y: 1,
                advance: 5
            })
        );
        assert_eq!(s[0].glyphs[&1].metrics.horizontal, None);
    }
}
#[test]
fn canonical_roundtrip_sparse_multistrike_and_empty() {
    let mut a = strike();
    a.glyphs.insert(
        4,
        BitmapGlyph {
            metrics: Metrics {
                horizontal: Some(Bearing {
                    advance: 9,
                    ..Default::default()
                }),
                ..Default::default()
            },
            bitmap: GlyphBitmap::Mono(Bitmap::default()),
        },
    );
    let mut b = a.clone();
    b.ppem_x = 12;
    b.ppem_y = 12;
    for kind in [Kind::OpenType, Kind::Apple] {
        let original = vec![a.clone(), b.clone()];
        let t = sbit::encode(&original, kind, 5, &Limits::default()).unwrap();
        let mut expected = original;
        if kind == Kind::Apple {
            expected.reverse();
        }
        assert_eq!(
            sbit::decode(&t.location, &t.data, kind, 5, &Limits::default()).unwrap(),
            expected
        );
    }
}
#[test]
fn composite_pixels_cycles_and_bounds() {
    let mut base = strike();
    base.glyphs.insert(2, glyph());
    let t = sbit::encode(&[base], Kind::OpenType, 3, &Limits::default()).unwrap();
    // Replace one contiguous range by two independent index records, one direct and one composite.
    let mut loc = t.location[..56].to_vec();
    loc[12..16].copy_from_slice(&48u32.to_be_bytes());
    loc[16..20].copy_from_slice(&2u32.to_be_bytes());
    p16(&mut loc, 1);
    p16(&mut loc, 1);
    p32(&mut loc, 16);
    p16(&mut loc, 2);
    p16(&mut loc, 2);
    p32(&mut loc, 32);
    p16(&mut loc, 1);
    p16(&mut loc, 1);
    p32(&mut loc, 4);
    p32(&mut loc, 0);
    p32(&mut loc, 7);
    p16(&mut loc, 1);
    p16(&mut loc, 8);
    p32(&mut loc, 11);
    p32(&mut loc, 0);
    p32(&mut loc, 12);
    let mut data = vec![
        0, 2, 0, 0, 2, 3, 255, 1, 4, 0xa0, 0x40, 2, 3, 255, 1, 4, 0, 0, 1, 0, 1, 0, 0,
    ];
    let s = sbit::decode(&loc, &data, Kind::OpenType, 3, &Limits::default()).unwrap();
    assert_eq!(s[0].glyphs[&1].bitmap, s[0].glyphs[&2].bitmap);
    data[20] = 2;
    assert!(sbit::decode(&loc, &data, Kind::OpenType, 3, &Limits::default()).is_err());
    data[20] = 1;
    data[21] = 1;
    assert!(sbit::decode(&loc, &data, Kind::OpenType, 3, &Limits::default()).is_err());
}
#[test]
fn ebsc_roundtrip_and_validation() {
    let s = sbit::Scale {
        horizontal: strike().horizontal,
        vertical: LineMetrics::default(),
        target: [28, 28],
        source: [14, 14],
    };
    let b = sbit::encode_scaling(std::slice::from_ref(&s), &Limits::default()).unwrap();
    assert_eq!(
        sbit::decode_scaling(&b, &Limits::default())
            .unwrap()
            .as_slice(),
        std::slice::from_ref(&s)
    );
    assert!(sbit::encode_scaling(&[s.clone(), s], &Limits::default()).is_err());
    for i in 0..b.len() {
        assert!(sbit::decode_scaling(&b[..i], &Limits::default()).is_err());
    }
}
#[test]
fn every_truncation_mutation_and_tight_budget_is_bounded() {
    let t = sbit::encode(&[strike()], Kind::OpenType, 3, &Limits::default()).unwrap();
    for i in 0..t.location.len() {
        assert!(
            sbit::decode(
                &t.location[..i],
                &t.data,
                Kind::OpenType,
                3,
                &Limits::default()
            )
            .is_err()
        );
    }
    for i in 0..t.data.len() {
        assert!(
            sbit::decode(
                &t.location,
                &t.data[..i],
                Kind::OpenType,
                3,
                &Limits::default()
            )
            .is_err()
        );
    }
    for i in 0..t.location.len() {
        let mut b = t.location.clone();
        b[i] ^= 255;
        let _ = sbit::decode(&b, &t.data, Kind::OpenType, 3, &Limits::default());
    }
    for (field, n) in [(0, 0), (1, 0), (2, 0), (3, 0)] {
        let mut l = Limits::default();
        match field {
            0 => l.max_strikes = n,
            1 => l.max_pixel_work = n,
            2 => l.max_visits = n,
            _ => l.sfnt.resources.max_total_bytes = n,
        };
        assert!(sbit::decode(&t.location, &t.data, Kind::OpenType, 3, &l).is_err());
    }
    let mut l = Limits::default();
    l.sfnt.resources.max_pixels_per_glyph = 5;
    assert!(sbit::decode(&t.location, &t.data, Kind::OpenType, 3, &l).is_err());
}
#[test]
fn property_random_bitmap_roundtrip() {
    use proptest::prelude::*;
    let mut runner = proptest::test_runner::TestRunner::new(proptest::test_runner::Config {
        cases: 256,
        ..Default::default()
    });
    runner
        .run(&(1u8..33, 1u8..33, any::<u64>()), |(w, h, mut seed)| {
            let mut b = Bitmap::new(w as u32, h as u32).unwrap();
            for y in 0..h as u32 {
                for x in 0..w as u32 {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    b.set_pixel(x, y, (seed >> 63) as u8).unwrap();
                }
            }
            let mut s = strike();
            s.glyphs.get_mut(&1).unwrap().metrics.width = w;
            s.glyphs.get_mut(&1).unwrap().metrics.height = h;
            s.glyphs.get_mut(&1).unwrap().bitmap = GlyphBitmap::Mono(b);
            let t = sbit::encode(&[s.clone()], Kind::OpenType, 2, &Limits::default()).unwrap();
            prop_assert_eq!(
                sbit::decode(&t.location, &t.data, Kind::OpenType, 2, &Limits::default()).unwrap(),
                vec![s]
            );
            Ok(())
        })
        .unwrap();
}
#[cfg(feature = "otb")]
#[test]
fn native_registry_multistrike_aliases_and_strict_loss() {
    use fontpixel::{
        Font, FontSet, Glyph, Input, ReadOptions, ResourcePath, WriteOptions,
        model::{Codepoint, Vector},
    };
    let mut f = Font::default();
    f.metrics.pixel_size = Some(14);
    f.metrics.ascent = Some(11);
    f.metrics.descent = Some(3);
    let mut g = Glyph::new(glyph().bitmap);
    g.codepoints = vec![Codepoint::Unicode(65), Codepoint::Unicode(0x10000)];
    g.name = Some("A".into());
    g.metrics.advance = Some(Vector::new(4, 0));
    g.metrics.bounds.x = -1;
    g.metrics.bounds.y = -1;
    f.add_glyph(g).unwrap();
    let mut second = f.clone();
    second.metrics.pixel_size = Some(12);
    let set = FontSet {
        fonts: vec![f, second],
        ..Default::default()
    };
    let reg = fontpixel::registry();
    let options = WriteOptions::new(ResourcePath::new("test.otb").unwrap());
    let written = reg.write(&set, "otb", &options).unwrap();
    let bytes = written.output.into_single().unwrap();
    let loaded = reg
        .read(&Input::new(&bytes), &ReadOptions::default())
        .unwrap();
    assert_eq!(loaded.format, "otb");
    assert_eq!(loaded.fonts.fonts.len(), 2);
    for f in &loaded.fonts.fonts {
        let g = f.glyph_for_codepoint(Codepoint::Unicode(65)).unwrap();
        assert_eq!(
            g.codepoints,
            [Codepoint::Unicode(65), Codepoint::Unicode(0x10000)]
        );
        assert_eq!(g.bitmap, glyph().bitmap);
        assert_eq!(g.metrics.advance, Some(Vector::new(4, 0)));
    }
    let mut strict = options;
    strict.strict = true;
    let mut set = set;
    set.fonts[0]
        .properties
        .push(fontpixel::model::Property::new("UNREPRESENTED", 7i64));
    assert!(matches!(
        reg.write(&set, "otb", &strict),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[cfg(feature = "color")]
fn png_bytes() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, 3, 2);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let mut w = e.write_header().unwrap();
        w.write_image_data(&[
            0, 0, 0, 255, 255, 0, 0, 128, 255, 255, 255, 0, 0, 255, 0, 64, 0, 0, 255, 192, 255,
            255, 255, 255,
        ])
        .unwrap();
    }
    out
}
#[cfg(feature = "color")]
#[test]
fn cbdt_png_formats_and_bgra_alpha_darkness() {
    for (index, image) in [(1, 17), (3, 18), (2, 19), (5, 19)] {
        let mut payload = if image == 17 {
            vec![2, 3, 255, 1, 4]
        } else if image == 18 {
            vec![2, 3, 255, 1, 4, 254, 2, 5]
        } else {
            vec![]
        };
        let png = png_bytes();
        p32(&mut payload, png.len() as u32);
        payload.extend(png);
        let (mut l, mut d) = raw(index, image, 32, &payload);
        l[1] = 3;
        d[1] = 3;
        let s = sbit::decode(&l, &d, Kind::Color, 3, &Limits::default()).unwrap();
        assert_eq!(
            s[0].glyphs[&1].bitmap.as_bytes(),
            [255, 128, 0, 64, 192, 255]
        );
        let s = sbit::decode_with_color(&l, &d, Kind::Color, 3, &Limits::default(), true).unwrap();
        assert_eq!(s[0].glyphs[&1].bitmap.as_bytes(), [255, 89, 0, 27, 170, 0]);
        for i in 0..d.len() {
            assert!(sbit::decode(&l, &d[..i], Kind::Color, 3, &Limits::default()).is_err());
        }
        let mut lim = Limits::default();
        lim.sfnt.resources.max_decompressed_bytes = 1;
        assert!(sbit::decode(&l, &d, Kind::Color, 3, &lim).is_err());
    }
    let (mut l, mut d) = raw(
        1,
        1,
        32,
        &[
            2, 3, 255, 1, 4, 0, 0, 0, 255, 0, 0, 128, 128, 0, 0, 0, 0, 0, 64, 0, 64, 192, 0, 0,
            192, 255, 255, 255, 255,
        ],
    );
    l[1] = 3;
    d[1] = 3;
    let s = sbit::decode(&l, &d, Kind::Color, 3, &Limits::default()).unwrap();
    assert_eq!(
        s[0].glyphs[&1].bitmap.as_bytes(),
        [255, 128, 0, 64, 192, 255]
    );
    d[9] = 255;
    d[12] = 0;
    assert!(sbit::decode(&l, &d, Kind::Color, 3, &Limits::default()).is_err());
}
#[cfg(feature = "color")]
fn sbix_bytes() -> Vec<u8> {
    let mut a = Vec::new();
    p16(&mut a, 1);
    p16(&mut a, 65535);
    a.extend(b"png ");
    a.extend(png_bytes());
    let mut dup = Vec::new();
    p16(&mut dup, 2);
    p16(&mut dup, 3);
    dup.extend(b"dupe");
    p16(&mut dup, 1);
    let mut strike = Vec::new();
    p16(&mut strike, 14);
    p16(&mut strike, 72);
    for n in [20, 20, 20 + a.len(), 20 + a.len() + dup.len()] {
        p32(&mut strike, n as u32);
    }
    strike.extend(a);
    strike.extend(dup);
    let mut data = vec![0, 1, 0, 1, 0, 0, 0, 1, 0, 0, 0, 12];
    data.extend(strike);
    data
}
#[cfg(feature = "color")]
#[test]
fn sbix_png_duplicate_metrics_cycles_and_budget() {
    let mut data = sbix_bytes();
    let s = sbit::color::decode_sbix(&data, 3, &Limits::default(), false).unwrap();
    assert_eq!(s.strikes[0].glyphs.len(), 2);
    assert_eq!(
        s.strikes[0].glyphs[&1].bitmap,
        s.strikes[0].glyphs[&2].bitmap
    );
    assert_eq!(s.strikes[0].glyphs[&2].origin, [1, -1]);
    for i in 0..data.len() {
        assert!(sbit::color::decode_sbix(&data[..i], 3, &Limits::default(), false).is_err());
    }
    let n = data.len();
    data[n - 1] = 2;
    assert!(sbit::color::decode_sbix(&data, 3, &Limits::default(), false).is_err());
    data[n - 1] = 0;
    assert!(sbit::color::decode_sbix(&data, 3, &Limits::default(), false).is_err());
    let mut lim = Limits::default();
    lim.sfnt.resources.max_pixels_per_glyph = 1;
    assert!(sbit::color::decode_sbix(&sbix_bytes(), 3, &lim, false).is_err());
    let mut data = sbix_bytes();
    data[36..40].copy_from_slice(b"jpg ");
    assert!(sbit::color::decode_sbix(&data, 3, &Limits::default(), false).is_err());
}
#[cfg(any(feature = "otb", feature = "bdat", feature = "color"))]
#[test]
fn native_capability_flags_remain_honest() {
    let registry = fontpixel::registry();
    for s in registry.formats() {
        match s.format.id {
            "otb" => assert_eq!(
                (s.detect, s.read, s.write),
                (
                    cfg!(feature = "otb"),
                    cfg!(feature = "otb"),
                    cfg!(feature = "otb")
                )
            ),
            "bdat" => assert_eq!(
                (s.detect, s.read, s.write),
                (
                    cfg!(feature = "bdat"),
                    cfg!(feature = "bdat"),
                    cfg!(feature = "bdat")
                )
            ),
            "color" => assert_eq!(
                (s.detect, s.read, s.write),
                (cfg!(feature = "color"), cfg!(feature = "color"), false)
            ),
            "ttf" | "otf" | "ttc" | "woff" | "woff2" | "vector-raster" => {
                let enabled = s.format.feature_enabled;
                assert_eq!(
                    (s.detect, s.read, s.write),
                    (enabled && s.format.id != "vector-raster", enabled, enabled)
                );
            }
            _ => (),
        }
    }
}
#[cfg(feature = "otb")]
#[test]
fn native_sparse_sizes_gray_vertical_metrics_and_limits() {
    use fontpixel::{
        Font, FontSet, Glyph,
        bitmap::{BitmapLimits, Gray8},
        formats::embedded,
        model::{Codepoint, Vector},
    };
    let mut f = Font::default();
    f.metrics.pixel_size = Some(18);
    f.metrics.ascent = Some(11);
    f.metrics.descent = Some(3);
    f.metrics.line_gap = Some(2);
    f.metrics.underline_position = Some(-2);
    f.metrics.underline_thickness = Some(1);
    let mut g = Glyph::new(
        Bitmap::<Gray8>::from_raw(3, 2, &[0, 17, 85, 128, 170, 255], BitmapLimits::default())
            .unwrap(),
    );
    g.codepoints.push(Codepoint::Unicode(65));
    g.name = Some("A".into());
    g.metrics.advance = Some(Vector::new(4, 0));
    g.metrics.bounds = fontpixel::bitmap::Bounds::new(3, 2, -1, -1);
    g.metrics.vertical_advance = Some(Vector::new(0, -6));
    g.metrics.vertical_origin = Some(Vector::new(1, 2));
    f.add_glyph(g.clone()).unwrap();
    let mut other = Font::default();
    other.metrics.pixel_size = Some(12);
    other.metrics.ascent = Some(11);
    other.metrics.descent = Some(3);
    g.codepoints = vec![Codepoint::Unicode(66)];
    g.name = Some("B".into());
    other.add_glyph(g.clone()).unwrap();
    let fonts = FontSet {
        fonts: vec![f, other],
        ..Default::default()
    };
    for kind in [Kind::OpenType, Kind::Apple] {
        let (bytes, _) = embedded::write(&fonts, kind, &Default::default()).unwrap();
        let (read, _) = embedded::read(&bytes, kind, &Default::default()).unwrap();
        for (cp, ppem) in [(65, 18), (66, 12)] {
            let f = read
                .fonts
                .iter()
                .find(|f| f.metrics.ppem_y == Some(ppem))
                .unwrap();
            let actual = f.glyph_for_codepoint(Codepoint::Unicode(cp)).unwrap();
            assert_eq!(actual.bitmap, g.bitmap);
            assert_eq!(actual.metrics.bounds, g.metrics.bounds);
            assert_eq!(actual.metrics.advance, g.metrics.advance);
            assert_eq!(actual.metrics.vertical_advance, g.metrics.vertical_advance);
            assert_eq!(actual.metrics.vertical_origin, g.metrics.vertical_origin);
        }
        let first = read
            .fonts
            .iter()
            .find(|f| f.metrics.ppem_y == Some(18))
            .unwrap();
        assert_eq!(first.metrics.line_gap, Some(2));
        assert_eq!(first.metrics.underline_position, Some(-2));
        let mut options = embedded::EmbeddedReadOptions::default();
        options.limits.sfnt.resources.max_total_bytes = 1024;
        assert!(embedded::read(&bytes, kind, &options).is_err());
    }
    let mut options = embedded::EmbeddedWriteOptions::default();
    options.limits.sfnt.resources.max_total_bytes = 1024;
    assert!(embedded::write(&fonts, Kind::OpenType, &options).is_err());
}
#[cfg(feature = "color")]
#[test]
fn independent_native_color_fixtures_and_structural_mutations() {
    use fontpixel::formats::embedded;
    let fixtures: &[&[u8]] = &[
        include_bytes!("../../../tests/data/embedded-bitmap/color-i1-f17.ttf"),
        include_bytes!("../../../tests/data/embedded-bitmap/color-i3-f18.ttf"),
        include_bytes!("../../../tests/data/embedded-bitmap/color-i2-f19.ttf"),
        include_bytes!("../../../tests/data/embedded-bitmap/sbix-False.ttf"),
        include_bytes!("../../../tests/data/embedded-bitmap/sbix-True.ttf"),
    ];
    for input in fixtures {
        let (set, report) = embedded::read(input, Kind::Color, &Default::default()).unwrap();
        assert_eq!(set.fonts.len(), 1);
        assert!(report.has_loss());
        for n in 0..input.len() {
            assert!(embedded::read(&input[..n], Kind::Color, &Default::default()).is_err());
        }
        for n in 0..input.len() {
            let mut b = input.to_vec();
            b[n] ^= 255;
            let _ = embedded::read(&b, Kind::Color, &Default::default());
        }
    }
}

#[cfg(feature = "otb")]
#[test]
fn gray_composite_overlap_is_bitwise_or_not_max() {
    use fontpixel::{formats::embedded, model::Codepoint};
    let data = include_bytes!("../../../tests/data/embedded-bitmap/composite-gray-overlap.ttf");
    let (set, _) = embedded::read(data, Kind::OpenType, &Default::default()).unwrap();
    let font = set.single().unwrap();
    assert_eq!(
        font.glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap()
            .bitmap
            .as_bytes(),
        [85; 6]
    );
    assert_eq!(
        font.glyph_for_codepoint(Codepoint::Unicode(66))
            .unwrap()
            .bitmap
            .as_bytes(),
        [170; 6]
    );
    assert_eq!(
        font.glyph_for_codepoint(Codepoint::Unicode(67))
            .unwrap()
            .bitmap
            .as_bytes(),
        [255; 6]
    );
}

#[cfg(feature = "otb")]
#[test]
fn diagnostics_are_bounded_before_returning_assets() {
    use fontpixel::{
        Font, Glyph,
        formats::embedded,
        model::{Codepoint, Property, Vector},
    };
    let mut f = Font::default();
    f.metrics.pixel_size = Some(14);
    f.metrics.ascent = Some(11);
    f.metrics.descent = Some(3);
    let mut g = Glyph::new(glyph().bitmap);
    g.codepoints.push(Codepoint::Unicode(65));
    g.metrics.advance = Some(Vector::new(4, 0));
    f.add_glyph(g).unwrap();
    for i in 0..20 {
        f.properties
            .push(Property::new(format!("UNKNOWN_PROPERTY_{i}"), 1i64));
    }
    let mut options = embedded::EmbeddedWriteOptions::default();
    options.limits.sfnt.resources.max_total_bytes = 10000;
    let result = embedded::write(&f.into(), Kind::OpenType, &options);
    assert!(matches!(
        result,
        Err(fontpixel::Error::ResourceLimit {
            resource: "embedded diagnostic bytes",
            ..
        })
    ));
}
