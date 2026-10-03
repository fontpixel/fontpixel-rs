//! Native SVG fonts, images, malformed inputs and resource limits.
#![cfg(feature = "svg")]
use fontpixel::{
    Bitmap, Font, FontSet, Glyph, Input,
    bitmap::Bounds,
    formats::svg::{self, ImageOptions, ReadOptions, WriteOptions},
    model::{Codepoint, Vector},
    outline::Shape,
};
fn font() -> Font {
    let mut f = Font::default();
    f.metrics.pixel_size = Some(8);
    f.metrics.ascent = Some(6);
    f.metrics.descent = Some(2);
    f.metrics.bounds = Some(Bounds::new(6, 8, -2, -2));
    f.metrics.line_gap = Some(2);
    f.metrics.underline_position = Some(-1);
    f.metrics.underline_thickness = Some(1);
    f.names.family = Some("A<&\"'家族".into());
    f.names.license = Some("line 1\nline 2\tlicensed & allowed".into());
    for (i, text) in [
        ".....\n.###.\n.#.#.\n.###.\n.....",
        ".....\n.....",
        ".#.\n#.#\n.#.",
        "#",
    ]
    .iter()
    .enumerate()
    {
        let mut g = Glyph::new(text.parse::<Bitmap>().unwrap());
        g.name = Some(format!("g{i}<&"));
        g.metrics.bounds.x = -2;
        g.metrics.bounds.y = -1;
        g.metrics.advance = Some(Vector::new(i as i64 + 3, 0));
        if i == 0 {
            g.codepoints = vec![Codepoint::Unicode(65), Codepoint::Unicode(0x10000)];
            g.unicode_sequences = vec![vec![102, 105], vec![66]];
        }
        if i == 1 {
            g.codepoints.push(Codepoint::Unicode(32));
        }
        if i == 2 {
            g.metrics.vertical_advance = Some(Vector::new(1, -8));
            g.metrics.vertical_origin = Some(Vector::new(2, 6));
        }
        let id = f.add_glyph(g).unwrap();
        if i == 0 {
            f.metrics.default_glyph = Some(id);
        }
    }
    f
}
fn text(body: &str) -> Vec<u8> {
    format!("<svg xmlns='http://www.w3.org/2000/svg'><defs><font horiz-adv-x='500'><font-face units-per-em='800' ascent='600' descent='200'/>{body}</font></defs></svg>").into_bytes()
}
fn read(body: &str) -> fontpixel::Result<(FontSet, fontpixel::Report)> {
    svg::read(&text(body), &ReadOptions::default())
}
fn equal(a: &Font, b: &Font) {
    assert_eq!(a.len(), b.len());
    assert_eq!(a.names, b.names);
    for ((_, a), (_, b)) in a.glyphs().zip(b.glyphs()) {
        assert_eq!(a, b);
    }
}
#[test]
fn native_roundtrip_all_shapes() {
    for (shape, size) in [
        (Shape::Square, 1.),
        (Shape::SquareSeparate, 0.75),
        (Shape::Dot, 0.9),
    ] {
        let f = font();
        let o = WriteOptions {
            shape,
            dot_size: size,
            ..Default::default()
        };
        let (data, r) = svg::write(&FontSet::from(f.clone()), &o).unwrap();
        assert!(!r.has_loss(), "{r}");
        assert_eq!(data, svg::write(&FontSet::from(f.clone()), &o).unwrap().0);
        let (set, r) = svg::read(&data, &Default::default()).unwrap();
        assert!(!r.has_loss(), "{r}");
        equal(&f, &set.fonts[0]);
        assert_eq!(f.metrics, set.fonts[0].metrics);
    }
}
#[test]
fn collection_and_select() {
    let f = font();
    let mut set = FontSet::from(f.clone());
    set.fonts.push(f);
    let d = svg::write(&set, &Default::default()).unwrap().0;
    assert_eq!(svg::read(&d, &Default::default()).unwrap().0.fonts.len(), 2);
    assert_eq!(
        svg::read(
            &d,
            &ReadOptions {
                font_index: Some(1),
                ..Default::default()
            }
        )
        .unwrap()
        .0
        .fonts
        .len(),
        1
    );
    assert!(
        svg::read(
            &d,
            &ReadOptions {
                font_index: Some(2),
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn generic_grid_and_all_commands() {
    let (set, r) = read("<glyph unicode='A' d='M0 0h300v300H0z M100 100V200H200V100z'/>").unwrap();
    assert!(r.has_loss());
    let g = set.fonts[0]
        .glyph_for_codepoint(Codepoint::Unicode(65))
        .unwrap();
    assert_eq!(
        g.bitmap,
        Glyph::new("###\n#.#\n###".parse::<Bitmap>().unwrap()).bitmap
    );
    assert_eq!(g.metrics.advance, Some(Vector::new(5, 0)));
}
#[test]
fn generic_relative_multiple_open_subpaths_close() {
    let (set, _) =
        read("<glyph d='m-100-100 100 0 0 100 -100 0 m200 0 100 0 0 100 -100 0'/>").unwrap();
    assert_eq!(
        set.fonts[0].glyphs().next().unwrap().1.bitmap,
        Glyph::new("..#\n#..".parse::<Bitmap>().unwrap()).bitmap
    );
}
#[test]
fn curves_arcs_and_smooth_commands() {
    for path in [
        "M0 0C0 100 100 100 100 0S200-100 200 0Z",
        "M0 0Q100 200 200 0T400 0Z",
        "M0 100A100 100 30 1 0 200 100a100 100 30 1 0-200 0Z",
    ] {
        let data = text(&format!("<glyph d='{path}'/>"));
        let (s, _) = svg::read(
            &data,
            &ReadOptions {
                ppem: Some(8),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(s.fonts[0].len(), 1);
    }
}
#[test]
fn generic_child_paths() {
    let (set, _) = read("<glyph unicode='A'><path d='M0 0h100v100H0Z'/></glyph>").unwrap();
    assert_eq!(set.fonts[0].len(), 1);
}
#[test]
fn explicit_scale_rounding_and_origin() {
    let d=b"<svg><font horiz-origin-x='100' horiz-origin-y='100' horiz-adv-x='450'><font-face units-per-em='800'/><glyph d='M0 0h300v300H0Z'/></font></svg>";
    let (s, r) = svg::read(
        d,
        &ReadOptions {
            ppem: Some(8),
            ..Default::default()
        },
    )
    .unwrap();
    let g = s.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(g.metrics.bounds, Bounds::new(3, 3, -1, -1));
    assert_eq!(g.metrics.advance, Some(Vector::new(5, 0)));
    assert!(r.entries.iter().any(|e| e.code == "svg.raster-projection"));
}
#[test]
fn xml_entities_and_namespace_prefixes() {
    let data=b"<?xml version='1.0'?><s:svg xmlns:s='http://www.w3.org/2000/svg'><s:font><s:font-face units-per-em='800'/><s:glyph unicode='&amp;' glyph-name='&quot;&lt;&gt;&apos;' d='M0 0h100v100H0z'/></s:font></s:svg>";
    let (s, _) = svg::read(data, &Default::default()).unwrap();
    assert!(
        s.fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(38))
            .is_some()
    );
}
#[test]
fn detection_distinguishes_font_image_and_foreign_xml() {
    let r = fontpixel::registry();
    let d = text("<glyph d='M0 0h100v100H0Z'/>");
    assert_eq!(
        r.detect(&Input::new(&d), &Default::default())
            .unwrap()
            .format
            .id,
        "svg"
    );
    assert!(svg::read(b"<svg><path d='M0 0'/></svg>", &Default::default()).is_err());
    assert!(svg::read(b"<svg xmlns='foreign'><font/></svg>", &Default::default()).is_err());
}
#[test]
fn native_metadata_does_not_hide_clipped_path() {
    let d = svg::write(&FontSet::from(font()), &Default::default())
        .unwrap()
        .0;
    let d = String::from_utf8(d)
        .unwrap()
        .replace("fp:bounds=\"5 5 -2 -1\"", "fp:bounds=\"1 1 -2 -1\"");
    assert!(svg::read(d.as_bytes(), &Default::default()).is_err());
}
#[test]
fn aliases_must_match() {
    let d = svg::write(&FontSet::from(font()), &Default::default())
        .unwrap()
        .0;
    let d = String::from_utf8(d).unwrap().replacen(
        "unicode=\"&#x41;\"",
        "horiz-origin-x=\"5\" unicode=\"&#x41;\"",
        1,
    );
    assert!(svg::read(d.as_bytes(), &Default::default()).is_err());
}
#[test]
fn native_vertical_advance_without_origin() {
    let mut f = font();
    let id = f.glyphs().last().unwrap().0;
    f.edit_glyph(id, |g| {
        g.metrics.vertical_advance = Some(Vector::new(0, -8));
        Ok(())
    })
    .unwrap();
    let d = svg::write(&FontSet::from(f.clone()), &Default::default())
        .unwrap()
        .0;
    equal(&f, &svg::read(&d, &Default::default()).unwrap().0.fonts[0]);
}
#[test]
fn missing_glyph_and_blank_preserved() {
    let (set,_)=svg::read(&text("<missing-glyph horiz-adv-x='0'/><glyph unicode=' ' horiz-adv-x='300'/><glyph unicode='A' d='M0 0h100v100H0Z'/>"),&Default::default()).unwrap();
    let f = &set.fonts[0];
    assert_eq!(f.len(), 3);
    assert!(f.metrics.default_glyph.is_some());
    assert_eq!(
        f.glyph_for_codepoint(Codepoint::Unicode(32))
            .unwrap()
            .bitmap
            .width(),
        0
    );
}
#[test]
fn malformed_and_unsafe_xml() {
    for bad in [
        "<!DOCTYPE svg><svg/>",
        "<!DOCTYPE svg [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><svg>&x;</svg>",
        "<svg><font></svg>",
        "<svg a='1' a='2'/>",
        "<svg>&unknown;</svg>",
        "<svg>&#0;</svg>",
        "<svg/><svg/>",
        "<?xml-stylesheet href='http://example.com'?><svg/>",
    ] {
        assert!(
            svg::read(bad.as_bytes(), &Default::default()).is_err(),
            "{bad}"
        );
    }
}
#[test]
fn rejects_unsupported_graphics() {
    for bad in [
        "<glyph transform='scale(2)' d='M0 0'/>",
        "<glyph fill-rule='evenodd' d='M0 0'/>",
        "<glyph style='fill:none'/>",
        "<glyph arabic-form='initial'/>",
        "<glyph><rect width='1'/></glyph>",
        "<glyph><path d='M0 0' transform='scale(2)'/></glyph>",
        "<script>alert(1)</script>",
        "<glyph d='M0 0'><path d='M0 0'/></glyph>",
        "<glyph><use href='#x'/></glyph>",
    ] {
        assert!(read(bad).is_err(), "{bad}");
    }
}
#[test]
fn malformed_numbers_paths() {
    for path in [
        "MNaN 0",
        "M1e999 0",
        "M0 0L",
        "M0 0A1 1 0 7 0 1 1",
        "M0 0Q1 1",
        "M0 0X1 2",
        "M0 0L1000000001 0",
    ] {
        assert!(read(&format!("<glyph d='{path}'/>")).is_err(), "{path}");
    }
}
#[test]
fn exhaustive_grid_checks_late_glyph() {
    assert!(read("<glyph d='M0 0h100v100H0Z'/><glyph d='M0 0h100.5v100H0Z'/>").is_err());
}
#[test]
fn budgets_are_effective() {
    let d = svg::write(&FontSet::from(font()), &Default::default())
        .unwrap()
        .0;
    let mut cases = Vec::new();
    let mut o = ReadOptions::default();
    o.limits.max_fonts = 0;
    cases.push(o);
    let mut o = ReadOptions::default();
    o.limits.max_nodes = 1;
    cases.push(o);
    let mut o = ReadOptions::default();
    o.limits.max_depth = 1;
    cases.push(o);
    let mut o = ReadOptions::default();
    o.limits.max_attributes = 1;
    cases.push(o);
    let mut o = ReadOptions::default();
    o.limits.resources.max_glyphs = 0;
    cases.push(o);
    let mut o = ReadOptions::default();
    o.limits.resources.max_table_bytes = 3;
    cases.push(o);
    let mut o = ReadOptions::default();
    o.limits.max_total_pixels = 1;
    cases.push(o);
    let mut o = ReadOptions::default();
    o.limits.max_total_edges = 1;
    cases.push(o);
    let mut o = ReadOptions::default();
    o.limits.resources.max_input_bytes = 1;
    cases.push(o);
    for o in cases {
        assert!(svg::read(&d, &o).is_err(), "{o:?}");
    }
}
#[test]
fn output_budget_invalid_sizes_and_xml() {
    let mut o = WriteOptions {
        max_output_bytes: 1,
        ..Default::default()
    };
    assert!(svg::write(&FontSet::from(font()), &o).is_err());
    o = WriteOptions::default();
    o.dot_size = f64::NAN;
    assert!(svg::write(&FontSet::from(font()), &o).is_err());
    o = WriteOptions::default();
    o.units_per_pixel = 0;
    assert!(svg::write(&FontSet::from(font()), &o).is_err());
    let mut f = font();
    f.names.family = Some("bad\0".into());
    assert!(svg::write(&FontSet::from(f), &Default::default()).is_err());
}
#[test]
fn loss_and_strict_are_atomic() {
    let mut f = font();
    let id = f.glyphs().last().unwrap().0;
    f.edit_glyph(id, |g| {
        g.codepoints.push(Codepoint::Unicode(0));
        Ok(())
    })
    .unwrap();
    let (d, r) = svg::write(&FontSet::from(f.clone()), &Default::default()).unwrap();
    assert!(r.has_loss());
    assert!(!String::from_utf8(d).unwrap().contains("&#x0;"));
    let mut o = fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("test.svg").unwrap());
    o.strict = true;
    assert!(matches!(
        fontpixel::write(&FontSet::from(f), "svg", &o),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}
#[test]
fn gray_requires_threshold() {
    let mut f = Font::default();
    f.metrics.pixel_size = Some(8);
    let gray = ".#".parse::<Bitmap>().unwrap().to_gray8().unwrap();
    f.add_glyph(Glyph::new(gray)).unwrap();
    assert!(svg::write(&FontSet::from(f.clone()), &Default::default()).is_err());
    let (_, r) = svg::write(
        &FontSet::from(f),
        &WriteOptions {
            gray_threshold: Some(128),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(r.has_loss());
}
#[test]
fn image_shapes_and_single_glyph() {
    let g = font().glyphs().next().unwrap().1.clone();
    for shape in [Shape::Square, Shape::SquareSeparate, Shape::Dot] {
        let o = ImageOptions {
            shape,
            ..Default::default()
        };
        let a = svg::glyph_image(&g, &o, None).unwrap();
        assert!(a.contains("scale(1 -1)"));
        assert!(roxmltree::Document::parse(&a).is_ok());
        if shape == Shape::Dot {
            assert!(a.contains("a0.5"));
        }
    }
}
#[test]
fn image_empty_and_limits() {
    assert!(svg::image(&Bitmap::default(), &Default::default()).is_ok());
    let mut o = ImageOptions::default();
    o.image.scale = 0;
    assert!(svg::image(&Bitmap::default(), &o).is_err());
    o = ImageOptions::default();
    o.image.limits.max_bytes = 1;
    assert!(svg::image(&Bitmap::default(), &o).is_err());
}
#[test]
fn custom_registry_options_persist_independently() {
    let mut r = fontpixel::registry();
    r.set_svg_read_options(ReadOptions {
        ppem: Some(8),
        ..Default::default()
    });
    r.set_svg_write_options(WriteOptions {
        shape: Shape::Dot,
        dot_size: 0.8,
        ..Default::default()
    });
    let d = text("<glyph d='M0 0h100.5v100H0Z'/>");
    assert!(r.read(&Input::new(&d), &Default::default()).is_ok());
    let o = fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("a.svg").unwrap());
    let d = r
        .write(&FontSet::from(font()), "svg", &o)
        .unwrap()
        .output
        .into_single()
        .unwrap();
    assert!(String::from_utf8(d).unwrap().contains("a40 40"));
}
proptest::proptest! {#[test] fn random_bitmap_font_roundtrip(data in proptest::collection::vec(proptest::prelude::any::<u8>(),0..256), w in 1u32..17,h in 0u32..17,x in -8i32..8,y in -8i32..8){let mut b:Bitmap=Bitmap::new(w,h).unwrap();for row in 0..h{for col in 0..w{if data.get(((row*w+col) as usize)%data.len().max(1)).is_some_and(|v|v&1!=0){b.set_pixel(col,row,1).unwrap();}}}let mut g=Glyph::new(b);g.metrics.bounds.x=i64::from(x);g.metrics.bounds.y=i64::from(y);g.metrics.advance=Some(Vector::new(9,0));let mut f=Font::default();f.metrics.pixel_size=Some(16);f.add_glyph(g).unwrap();for shape in [Shape::Square,Shape::Dot]{let d=svg::write(&FontSet::from(f.clone()),&WriteOptions{shape,..Default::default()}).unwrap().0;equal(&f,&svg::read(&d,&Default::default()).unwrap().0.fonts[0]);}}}

#[test]
fn repeated_glyph_geometry_is_streamed_under_model_budget() {
    let path = "M0 0h100v100H0Z".repeat(20);
    let body = format!("<glyph d='{path}'/>").repeat(100);
    let mut o = ReadOptions::default();
    o.limits.resources.max_total_bytes = 128 * 1024;
    let (s, _) = svg::read(&text(&body), &o).unwrap();
    assert_eq!(s.fonts[0].len(), 100);
}
#[test]
fn writer_counts_alias_records_and_generated_commands() {
    let f = FontSet::from(font());
    let mut o = WriteOptions::default();
    o.limits.resources.max_glyphs = 4;
    assert!(svg::write(&f, &o).is_err());
    o = WriteOptions::default();
    o.limits.max_total_edges = 1;
    assert!(svg::write(&f, &o).is_err());
}

#[test]
fn xml_budgets_preflight_attributes_namespaces_and_depth() {
    let mut o = ReadOptions::default();
    o.limits.max_attributes = 4;
    for attrs in [
        (0..5).map(|i| format!(" a{i}='v'")).collect::<String>(),
        (0..5)
            .map(|i| format!(" xmlns:p{i}='urn:{i}'"))
            .collect::<String>(),
    ] {
        assert!(matches!(
            svg::read(format!("<svg{attrs}/>").as_bytes(), &o),
            Err(fontpixel::Error::ResourceLimit {
                resource: "SVG XML attributes",
                ..
            })
        ));
    }
    o = ReadOptions::default();
    o.limits.max_depth = 4;
    let s = format!("<svg>{}{}</svg>", "<g>".repeat(10000), "</g>".repeat(10000));
    assert!(matches!(
        svg::read(s.as_bytes(), &o),
        Err(fontpixel::Error::ResourceLimit {
            resource: "SVG XML depth",
            ..
        })
    ));
}
#[test]
fn huge_arc_expansion_is_rejected_before_normalization() {
    for d in [
        "M0 0A1e-300 1e9 0 0 0 1 1",
        "M0 0A1e9 1e-300 0 0 1 100 0",
        "M1e9 0m1e9 0",
    ] {
        assert!(read(&format!("<glyph d='{d}'/>")).is_err());
    }
}

#[test]
fn effective_defaults_zero_advances_and_negative_bearings() {
    let mut f = Font::default();
    f.metrics.pixel_size = Some(8);
    f.metrics.glyph_defaults.advance = Some(Vector::new(0, 1));
    f.metrics.glyph_defaults.vertical_advance = Some(Vector::new(2, -9));
    f.metrics.glyph_defaults.vertical_origin = Some(Vector::new(-3, 7));
    let mut g = Glyph::new("#".parse::<Bitmap>().unwrap());
    g.metrics.bounds = Bounds::new(1, 1, -4, -2);
    f.add_glyph(g).unwrap();
    let (d, r) = svg::write(&FontSet::from(f), &Default::default()).unwrap();
    assert!(r.has_loss());
    let (set, _) = svg::read(&d, &Default::default()).unwrap();
    let g = set.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(g.metrics.advance, Some(Vector::new(0, 1)));
    assert_eq!(g.metrics.vertical_advance, Some(Vector::new(2, -9)));
    assert_eq!(g.metrics.vertical_origin, Some(Vector::new(-3, 7)));
    assert_eq!(g.metrics.bounds, Bounds::new(1, 1, -4, -2));
}
#[test]
fn generic_metadata_loss_and_selected_font_locations() {
    let d=b"<svg><font><font-face units-per-em='800' font-weight='bold'/><glyph d='M0 0h100v100H0Z'/><hkern u1='A' u2='V' k='100'/></font><font><font-face units-per-em='800' font-weight='bold'/><glyph d='M0 0h100v100H0Z'/></font></svg>";
    let (_, r) = svg::read(
        d,
        &ReadOptions {
            font_index: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(r.has_loss());
    assert!(r.entries.iter().all(|e| e.location.font_index == Some(0)));
}
#[test]
fn common_registry_budgets_are_enforced() {
    let d = svg::write(&FontSet::from(font()), &Default::default())
        .unwrap()
        .0;
    let mut o = fontpixel::ReadOptions {
        format: Some("svg".into()),
        ..Default::default()
    };
    o.limits.max_glyphs = 0;
    assert!(matches!(
        fontpixel::read(&d, &o),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}
#[test]
fn deterministic_truncations_and_byte_mutations_do_not_panic() {
    let d = text("<glyph unicode='A' d='M0 0Q100 300 200 0T400 0Z'/>");
    let o = ReadOptions {
        ppem: Some(8),
        ..Default::default()
    };
    for end in 0..d.len() {
        let _ = svg::read(&d[..end], &o);
    }
    for i in 0..d.len() {
        for byte in [0, 39, 60, 62, 255] {
            let mut bytes = d.clone();
            bytes[i] = byte;
            let _ = svg::read(&bytes, &o);
        }
    }
}

#[cfg(not(feature = "cjk-charsets"))]
#[test]
fn legacy_conversion_requires_its_charset_feature() {
    let mut f = Font::default();
    f.metrics.pixel_size = Some(8);
    f.charset.name = Some("gb2312".into());
    let mut g = Glyph::new("#".parse::<Bitmap>().unwrap());
    g.codepoints.push(Codepoint::Legacy(0x2121));
    f.add_glyph(g).unwrap();
    assert!(
        svg::write(&FontSet::from(f), &Default::default())
            .unwrap_err()
            .to_string()
            .contains("cjk-charsets")
    );
}

#[test]
fn independent_child_paths_union_even_with_opposite_winding() {
    let o = ReadOptions {
        ppem: Some(8),
        ..Default::default()
    };
    let (set, _) = svg::read(
        &text("<glyph><path d='M0 0h300v300H0Z'/><path d='M0 0v300h300V0Z'/></glyph>"),
        &o,
    )
    .unwrap();
    let g = set.fonts[0].glyphs().next().unwrap().1;
    assert_eq!(
        g.bitmap,
        Glyph::new("###\n###\n###".parse::<Bitmap>().unwrap()).bitmap
    );
    // Subpaths of a single path still use the nonzero winding rule.
    let (set, _) = svg::read(&text("<glyph d='M0 0h300v300H0Z M0 0v300h300V0Z'/>"), &o).unwrap();
    assert_eq!(set.fonts[0].glyphs().next().unwrap().1.bitmap.width(), 0);
    let mut limited = o;
    limited.limits.max_total_pixels = 20;
    assert!(matches!(
        svg::read(
            &text("<glyph><path d='M0 0h300v300H0Z'/><path d='M0 0v300h300V0Z'/></glyph>"),
            &limited
        ),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}

#[test]
fn alias_loss_locations_reference_retained_glyph() {
    let d = text(
        "<glyph xmlns:fp='https://fontpixel.org/ns/svg/1' fp:id='0' unicode='A' d='M0 0h100v100H0Z' extra='x'/><glyph xmlns:fp='https://fontpixel.org/ns/svg/1' fp:id='0' unicode='B' d='M0 0h100v100H0Z' extra='x'/>",
    );
    let (set, r) = svg::read(&d, &Default::default()).unwrap();
    assert_eq!(set.fonts[0].len(), 1);
    assert!(
        r.entries
            .iter()
            .filter_map(|e| e.location.glyph_index)
            .all(|i| i == 0)
    );
}

#[test]
fn arbitrary_style_names_are_not_invalid_svg_descriptors() {
    for style in [
        "Regular",
        "Bold Italic",
        "自定义",
        "italic",
        "normal,oblique",
    ] {
        let mut f = font();
        f.names.style = Some(style.into());
        let (d, r) = svg::write(&FontSet::from(f.clone()), &Default::default()).unwrap();
        assert!(!r.has_loss());
        let s = std::str::from_utf8(&d).unwrap();
        let doc = roxmltree::Document::parse(s).unwrap();
        let face = doc
            .descendants()
            .find(|n| n.tag_name().name() == "font-face")
            .unwrap();
        if let Some(value) = face.attribute("font-style") {
            assert!(
                value == "all"
                    || value
                        .split(',')
                        .all(|s| matches!(s.trim(), "normal" | "italic" | "oblique")),
                "{value}"
            );
        }
        equal(&f, &svg::read(&d, &Default::default()).unwrap().0.fonts[0]);
    }
}

#[test]
fn unicode_mapping_model_budget_is_preallocation() {
    let d = text(&format!(
        "<glyph unicode='{}' d='M0 0h100v100H0Z'/>",
        "A".repeat(1024)
    ));
    let mut o = ReadOptions::default();
    o.limits.resources.max_total_bytes = 4096;
    assert!(matches!(
        svg::read(&d, &o),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}

#[test]
fn many_aliases_are_grouped_once_before_indexing() {
    let mut body = String::new();
    for i in 0..8192 {
        body.push_str(&format!("<glyph xmlns:fp='https://fontpixel.org/ns/svg/1' fp:id='0' unicode='&#x{:x};' d='M0 0h100v100H0Z'/>", 0x10000 + i));
    }
    let (set, _) = read(&body).unwrap();
    let f = &set.fonts[0];
    assert_eq!(f.len(), 1);
    assert_eq!(f.glyphs().next().unwrap().1.codepoints.len(), 8192);
    assert_eq!(
        f.glyph_id(Codepoint::Unicode(0x10000)),
        f.glyph_id(Codepoint::Unicode(0x11fff))
    );
}

#[test]
fn active_animation_is_rejected_and_unknown_native_fields_reported() {
    for content in [
        "<animate attributeName='d'/>",
        "<animateMotion/>",
        "<animateTransform/>",
        "<set attributeName='horiz-adv-x' to='200'/>",
    ] {
        assert!(read(&format!("{content}<glyph d='M0 0h100v100H0Z'/>")).is_err());
    }
    let (_, r) = read(
        "<glyph xmlns:fp='https://fontpixel.org/ns/svg/1' fp:unknown='x' d='M0 0h100v100H0Z'/>",
    )
    .unwrap();
    assert!(r.entries.iter().any(|e| e.code == "svg.glyph-attributes"));
}

#[test]
fn common_registry_model_budget_reaches_explicit_svg_reader() {
    let d = text(&format!(
        "<glyph unicode='{}' d='M0 0h100v100H0Z'/>",
        "A".repeat(1024)
    ));
    let mut o = fontpixel::ReadOptions {
        format: Some("svg".into()),
        ..Default::default()
    };
    o.limits.max_total_bytes = 4096;
    assert!(matches!(
        fontpixel::read(&d, &o),
        Err(fontpixel::Error::ResourceLimit { .. })
    ));
}

#[test]
fn negative_horizontal_advances_cannot_emit_invalid_svg() {
    for defaults in [false, true] {
        let mut f = font();
        if defaults {
            f.metrics.glyph_defaults.advance = Some(Vector::new(-1, 0));
        } else {
            let id = f.glyphs().next().unwrap().0;
            f.edit_glyph(id, |g| {
                g.metrics.advance = Some(Vector::new(-1, 0));
                Ok(())
            })
            .unwrap();
        }
        assert!(svg::write(&FontSet::from(f), &Default::default()).is_err());
    }
}

#[test]
fn child_paths_keep_documented_exact_center_boundary_rule() {
    let (set, _) = svg::read(
        &text("<glyph><path d='M0 0Q100 400 400 0Z'/><path d='M100 0L300 300L400 0Z'/></glyph>"),
        &ReadOptions {
            ppem: Some(8),
            ..Default::default()
        },
    )
    .unwrap();
    // The right diagonal goes exactly through (350,150); the half-open native
    // crossing rule excludes that center, unlike an antialias alpha >= 128 tie.
    assert_eq!(
        set.fonts[0].glyphs().next().unwrap().1.bitmap,
        Glyph::new(".##.\n####".parse::<Bitmap>().unwrap()).bitmap
    );
}
