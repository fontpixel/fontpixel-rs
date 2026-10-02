//! Native rendering, failure budgets, image export and borrowed-blit regressions.
use fontpixel::bitmap::{Bitmap, BitmapLimits, Bounds, CompositeMode, Gray8};
use fontpixel::model::{Codepoint, Vector};
use fontpixel::render::{
    GlyphOrder, ImageOptions, MissingGlyph, RenderMode, RenderOptions, Rgba, WritingDirection,
    to_svg,
};
use fontpixel::{Error, Font, Glyph};

fn font() -> Font {
    let mut f = Font::default();
    f.metrics.bounds = Some(Bounds::new(4, 3, -1, -1));
    for (cp, rows, advance) in [(65, ".#.\n###\n#.#", 3), (66, "##.\n###\n##.", 2)] {
        let mut g = Glyph::new(rows.parse::<Bitmap>().unwrap());
        g.metrics.bounds.x = -1;
        g.metrics.bounds.y = -1;
        g.metrics.advance = Some(Vector { x: advance, y: 0 });
        g.metrics.vertical_advance = Some(Vector { x: 0, y: 3 });
        g.codepoints.push(Codepoint::Unicode(cp));
        f.add_glyph(g).unwrap();
    }
    f
}
#[test]
fn default_advance_cell_ink_and_gaps() {
    let f = font();
    let mut o = RenderOptions::default();
    assert_eq!(f.draw("AB", &o).unwrap().bitmap.width(), 7);
    o.mode = RenderMode::Cell;
    assert_eq!(f.draw("AB", &o).unwrap().bitmap.width(), 8);
    o.mode = RenderMode::Ink;
    assert_eq!(f.draw("AB", &o).unwrap().bitmap.width(), 6);
    o.mode = RenderMode::Cell;
    o.letter_spacing = 2;
    assert_eq!(f.draw("AB", &o).unwrap().bitmap.width(), 10);
    o.letter_spacing = 0;
    o.line_limit = 4;
    o.line_spacing = 2;
    let r = f.draw("AB", &o).unwrap();
    assert_eq!((r.bitmap.width(), r.bitmap.height()), (4, 8));
    assert_eq!(
        r.bitmap.to_string(),
        ".#..\n###.\n#.#.\n....\n....\n##..\n###.\n##.."
    );
    o.line_spacing = -1;
    assert_eq!(f.draw("AB", &o).unwrap().bitmap.height(), 5);
    o.use_current_glyph_spacing = true;
    o.mode = RenderMode::Advance;
    o.line_limit = 512;
    assert_eq!(f.draw("AB", &o).unwrap().bitmap.width(), 6);
}
#[test]
fn directions_and_missing_are_explicit() {
    for a in ["lr", "rl", "tb", "bt"] {
        for b in ["lr", "rl", "tb", "bt"] {
            assert!(format!("{a}{b}").parse::<WritingDirection>().is_ok());
        }
    }
    assert!("garbage".parse::<WritingDirection>().is_err());
    assert_eq!(
        WritingDirection::from_spec("garbage"),
        WritingDirection::default()
    );
    assert_eq!(
        WritingDirection::from_spec("lrbt-suffix"),
        "lrbt".parse().unwrap()
    );
    let f = font();
    let mut o = RenderOptions::default();
    let r = f.draw("A??B", &o).unwrap();
    assert!(r.report.has_loss());
    assert_eq!(r.report.entries.len(), 1); // repeated same missing code is deduplicated
    o.strict = true;
    assert!(matches!(f.draw("A?", &o), Err(Error::StrictLoss(_))));
    o.strict = false;
    o.missing = MissingGlyph::Error;
    assert!(f.draw("A?", &o).is_err());
    o.missing = MissingGlyph::Glyph(f.glyph_for_codepoint(Codepoint::Unicode(66)).unwrap());
    assert_eq!(
        f.draw("A?", &o).unwrap().bitmap,
        f.draw("AB", &o).unwrap().bitmap
    );
    assert_eq!(f.draw("", &o).unwrap().bitmap, Bitmap::default());
}
#[test]
fn clipping_is_real_ink_loss_and_bearings_do_not_overflow() {
    let mut f = font();
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.shift(-2, 0)?;
        Ok(())
    })
    .unwrap();
    let r = f.draw("A", &RenderOptions::default()).unwrap();
    assert!(
        r.report
            .entries
            .iter()
            .any(|e| e.code == "render.cell_clipping")
    );
    assert_eq!(r.bitmap.to_string(), "....\n#...\n#...");
    f.metrics.bounds = Some(Bounds::new(4, 3, i64::MAX, i64::MIN));
    let r = f.draw("A", &RenderOptions::default()).unwrap();
    assert!(!r.bitmap.has_ink());
}
#[test]
fn limits_apply_before_rendering_and_empty_layout_is_bounded() {
    let f = font();
    let mut o = RenderOptions::default();
    o.limits.max_glyphs = 2;
    assert!(matches!(
        f.draw("AAA", &o),
        Err(Error::ResourceLimit { .. })
    ));
    o = RenderOptions::default();
    o.limits.max_layout_bytes = 1;
    assert!(matches!(f.draw("A", &o), Err(Error::ResourceLimit { .. })));
    o = RenderOptions::default();
    o.limits.bitmap.max_pixels = 1;
    assert!(matches!(f.draw("A", &o), Err(Error::ResourceLimit { .. })));
    o = RenderOptions::default();
    o.letter_spacing = i64::MAX;
    assert!(f.draw("A", &o).is_err());
    o.line_limit = i64::MAX;
    assert!(f.draw("AA", &o).is_err());
    o.letter_spacing = i64::MIN;
    assert!(f.draw("AA", &o).is_err());
    let empty = Font::default();
    o = RenderOptions::default();
    o.limits.max_glyphs = 2;
    assert!(empty.draw("AAAA", &o).is_err());
    assert_eq!(
        empty.draw_all(GlyphOrder::File, &[], &o).unwrap().bitmap,
        Bitmap::default()
    );
}
#[test]
fn gray_conversion_is_explicit_and_reported() {
    let mut f = font();
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.bitmap = Bitmap::<Gray8>::from_raw(
            3,
            3,
            &[0, 127, 128, 255, 0, 254, 42, 200, 100],
            BitmapLimits::default(),
        )?
        .into();
        Ok(())
    })
    .unwrap();
    assert!(f.draw("A", &RenderOptions::default()).is_err());
    let o = RenderOptions {
        gray_threshold: Some(128),
        ..Default::default()
    };
    let r = f.draw("A", &o).unwrap();
    assert!(r.report.has_loss());
    assert_eq!(r.bitmap.to_string(), "..#.\n#.#.\n.#..");
    let o = RenderOptions { strict: true, ..o };
    assert!(matches!(f.draw("A", &o), Err(Error::StrictLoss(_))));
}
#[test]
fn unicode_to_legacy_uses_the_existing_exact_tables() {
    let mut f = font();
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.codepoints = vec![Codepoint::Legacy(0x80)];
        Ok(())
    })
    .unwrap();
    f.charset.name = Some("cp1252".into());
    let o = RenderOptions::default();
    assert_eq!(
        f.draw("€", &o).unwrap().bitmap,
        f.draw_codepoints([Some(Codepoint::Legacy(0x80))], &o)
            .unwrap()
            .bitmap
    );
    assert!(f.draw("人", &o).unwrap().report.has_loss());
    f.charset.name = Some("unknown-test".into());
    assert!(
        f.draw("B", &o)
            .unwrap()
            .report
            .entries
            .iter()
            .any(|e| e.code == "render.unknown_charset")
    );
}
#[test]
fn borrowed_composite_region_matches_scalar_clipping() {
    let source: Bitmap = "#..##.#..\n.#####.#.\n####..##.".parse().unwrap();
    for sx in -3..12 {
        for sy in -2..6 {
            for tx in -3..10 {
                for ty in -2..5 {
                    let mut actual = Bitmap::new(12, 7).unwrap();
                    let mut expected = actual.clone();
                    actual.composite_region(
                        &source,
                        Bounds::new(8, 4, sx, sy),
                        tx,
                        ty,
                        CompositeMode::Over,
                    );
                    for y in 0..4 {
                        for x in 0..8 {
                            let a = sx + x;
                            let b = sy + y;
                            let c = tx + x;
                            let d = ty + y;
                            if (0..9).contains(&a)
                                && (0..3).contains(&b)
                                && (0..12).contains(&c)
                                && (0..7).contains(&d)
                            {
                                expected
                                    .set_pixel(
                                        c as u32,
                                        d as u32,
                                        source.get_pixel(a as u32, b as u32).unwrap(),
                                    )
                                    .unwrap();
                            }
                        }
                    }
                    assert_eq!(actual, expected, "sx={sx} sy={sy} tx={tx} ty={ty}");
                }
            }
        }
    }
    let mut actual = Bitmap::new(2, 2).unwrap();
    actual.composite_region(
        &source,
        Bounds::new(u32::MAX, u32::MAX, i64::MIN, i64::MAX),
        i64::MAX,
        i64::MIN,
        CompositeMode::Over,
    );
    assert!(!actual.has_ink());
}
#[test]
fn svg_and_color_validation_are_bounded_and_deterministic() {
    assert_eq!("#f08".parse::<Rgba>().unwrap(), Rgba::new(255, 0, 136, 255));
    assert_eq!(
        "#f08c".parse::<Rgba>().unwrap(),
        Rgba::new(255, 0, 136, 204)
    );
    assert_eq!(
        "#aabbccdd".parse::<Rgba>().unwrap(),
        Rgba::new(170, 187, 204, 221)
    );
    for bad in ["red", "#ff", "#zz0000", "#１２３", "\"><script/>"] {
        assert!(bad.parse::<Rgba>().is_err());
    }
    let b: Bitmap = "#.#\n###".parse().unwrap();
    let svg = to_svg(
        &b,
        &ImageOptions {
            scale: 4,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(svg.contains("width=\"12\" height=\"8\""));
    assert!(svg.contains("M0 1h3v1h-3z"));
    assert!(
        to_svg(
            &b,
            &ImageOptions {
                scale: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
    let o = ImageOptions {
        limits: BitmapLimits {
            max_bytes: 10,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(to_svg(&b, &o), Err(Error::ResourceLimit { .. })));
    assert!(to_svg(&Bitmap::default(), &ImageOptions::default()).is_ok());
}
#[cfg(feature = "png")]
#[test]
fn png_roundtrips_bits_dimensions_alpha_and_limits() {
    let b: Bitmap = "#.#\n###".parse().unwrap();
    let o = ImageOptions {
        scale: 3,
        foreground: Rgba::new(2, 4, 6, 128),
        background: Rgba::TRANSPARENT,
        ..Default::default()
    };
    let bytes = fontpixel::render::to_png(&b, &o).unwrap();
    use std::io::Read;
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let mut offset = 8;
    let mut compressed = Vec::new();
    while offset < bytes.len() {
        let n = u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let kind = &bytes[offset + 4..offset + 8];
        let data = &bytes[offset + 8..offset + 8 + n];
        let crc = u32::from_be_bytes(bytes[offset + 8 + n..offset + 12 + n].try_into().unwrap());
        assert_eq!(crc, crc32fast::hash(&bytes[offset + 4..offset + 8 + n]));
        match kind {
            b"IHDR" => {
                assert_eq!(&data[..4], 9u32.to_be_bytes());
                assert_eq!(&data[4..8], 6u32.to_be_bytes());
                assert_eq!(data[8], 1);
                assert_eq!(data[9], 3);
            }
            b"PLTE" => assert_eq!(data, [0, 0, 0, 2, 4, 6]),
            b"tRNS" => assert_eq!(data, [0, 128]),
            b"IDAT" => compressed.extend_from_slice(data),
            _ => {}
        }
        offset += n + 12;
    }
    let mut decoded = Vec::new();
    flate2::read::ZlibDecoder::new(compressed.as_slice())
        .read_to_end(&mut decoded)
        .unwrap();
    let expected = b.enlarge(3, 3).unwrap();
    for (actual, expected) in decoded
        .as_chunks::<3>()
        .0
        .iter()
        .zip(expected.as_bytes().as_chunks::<2>().0.iter())
    {
        assert_eq!(actual[0], 0);
        assert_eq!(&actual[1..], expected);
    }
    assert!(fontpixel::render::to_png(&Bitmap::default(), &o).is_err());
    let o = ImageOptions {
        limits: BitmapLimits {
            max_bytes: 10,
            ..Default::default()
        },
        ..o
    };
    assert!(matches!(
        fontpixel::render::to_png(&b, &o),
        Err(Error::ResourceLimit { .. })
    ));
}
#[cfg(feature = "bdf")]
#[test]
fn invalid_bdf_source_never_renders_placeholder_pixels() {
    let src = "STARTFONT 2.1\nFONT broken\nSIZE 8 72 72\nFONTBOUNDINGBOX 4 1 0 0\nCHARS 1\nSTARTCHAR A\nENCODING 65\nDWIDTH 4 0\nBBX 4 1 0 0\nBITMAP\nZZ\nENDCHAR\nENDFONT\n";
    let mut f = fontpixel::read(src.as_bytes(), &Default::default())
        .unwrap()
        .fonts
        .into_single()
        .unwrap();
    let e = f.draw("A", &RenderOptions::default()).unwrap_err();
    assert!(e.to_string().contains("non-hex"));
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    f.edit_glyph(id, |g| g.set_bitmap("####".parse::<Bitmap>()?, 0, 0, false))
        .unwrap();
    assert_eq!(
        f.draw("A", &RenderOptions::default())
            .unwrap()
            .bitmap
            .to_string(),
        "####"
    );
}

#[test]
fn diagnostic_deduplication_scales_and_respects_its_own_budget() {
    let f = Font::default();
    let mut options = RenderOptions::default();
    let rendered = f
        .draw_codepoints((0..12_000).map(|n| Some(Codepoint::Unicode(n))), &options)
        .unwrap();
    assert_eq!(rendered.report.entries.len(), 12_000);
    assert_eq!(rendered.bitmap, Bitmap::default());
    options.limits.max_report_bytes = 1;
    assert!(matches!(
        f.draw("?", &options),
        Err(Error::ResourceLimit {
            resource: "render diagnostic bytes",
            ..
        })
    ));
    options.limits.max_report_bytes = 0;
    assert!(font().draw("A", &options).is_ok());
    let mut unknown = font();
    unknown.charset.name = Some("unknown-test".into());
    assert!(matches!(
        unknown.draw("A", &options),
        Err(Error::ResourceLimit { .. })
    ));
}

#[test]
fn file_order_aliases_ranges_and_zero_area_are_explicit() {
    let mut f = font();
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.codepoints = vec![
            Codepoint::Unicode(65),
            Codepoint::Unicode(67),
            Codepoint::Unicode(65),
        ];
        Ok(())
    })
    .unwrap();
    let options = RenderOptions {
        mode: RenderMode::Cell,
        ..Default::default()
    };
    assert_eq!(
        f.draw_all(GlyphOrder::File, &[], &options).unwrap().bitmap,
        f.draw("ACB", &options).unwrap().bitmap
    );
    assert_eq!(
        f.draw_all(GlyphOrder::ReverseFile, &[(66, 67)], &options)
            .unwrap()
            .bitmap,
        f.draw("BC", &options).unwrap().bitmap
    );
    for (w, h) in [(0, 5), (5, 0), (0, 0)] {
        let mut empty = Font::default();
        empty.metrics.bounds = Some(Bounds::new(w, h, 0, 0));
        let mut glyph = Glyph::new(Bitmap::<fontpixel::bitmap::Mono>::new(w, h).unwrap());
        glyph.codepoints.push(Codepoint::Unicode(65));
        empty.add_glyph(glyph).unwrap();
        for direction in ["lrtb", "rltb", "tbrl", "btlr"] {
            let options = RenderOptions {
                direction: direction.parse().unwrap(),
                ..options.clone()
            };
            assert!(empty.draw("AA", &options).is_ok());
        }
    }
}
