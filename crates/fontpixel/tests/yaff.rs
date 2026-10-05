//! YAFF syntax, exact model preservation, editing and resource regressions.
#![cfg(feature = "yaff")]
use fontpixel::bitmap::{Bounds, Gray8};
use fontpixel::formats::yaff::{self, YaffWriteOptions};
use fontpixel::model::*;
use fontpixel::{Bitmap, Font, FontSet, Glyph, ResourceLimits};
fn parse(s: &str) -> FontSet {
    yaff::read(s.as_bytes(), &ResourceLimits::default())
        .unwrap()
        .0
}
fn roundtrip(set: &FontSet) {
    let (bytes, report) = yaff::write(set, &YaffWriteOptions::default()).unwrap();
    assert!(!report.has_loss());
    let (back, report) = yaff::read(&bytes, &ResourceLimits::default())
        .unwrap_or_else(|e| panic!("{e}\n{}", String::from_utf8_lossy(&bytes)));
    assert!(!report.has_loss());
    assert_eq!(&back, set);
    let (again, _) = yaff::write(&back, &YaffWriteOptions::default()).unwrap();
    assert_eq!(again, bytes);
}
#[test]
fn standard_labels_metrics_and_multiline() {
    let f=parse("\u{feff}# hello\r\nyaff: 1.0\rfamily: Test\rnotice:\r    line one\r    \" line two \"\rleft-bearing: -2\rshift-up: -1\ru+0041:\r0x81, 0x40:\r\"A\":\r    .@.\r    @@@\r\r    right-bearing: 2\r    shift-up: 3\r\r'fi':\r    -\r").into_single().unwrap();
    assert_eq!(f.len(), 2);
    let g = f.glyph_for_codepoint(Codepoint::Unicode(65)).unwrap();
    assert_eq!(
        g.codepoints,
        vec![Codepoint::Unicode(65), Codepoint::Legacy(0x8140)]
    );
    assert_eq!(g.metrics.bounds, Bounds::new(3, 2, -2, 2));
    assert_eq!(g.metrics.advance, Some(Vector::new(3, 0)));
    assert!(f.glyph_for_sequence(&[102, 105]).is_some());
    assert_eq!(f.names.license.as_deref(), Some("line one\n line two "));
    roundtrip(&FontSet::from(f));
}
#[test]
fn exact_rich_model_and_editing() {
    let mut f = Font::default();
    f.names.family = Some("Family".into());
    f.names.style = Some("\"styled\"".into());
    f.names.localized.push(Property::new("日本語", "フォント"));
    f.metrics.point_size = Some(Rational::new(21, 3).unwrap());
    f.metrics.glyph_defaults.advance = Some(Vector::new(8, 2));
    f.charset.registry = Some("ISO10646".into());
    f.charset.encoding = Some("1".into());
    f.charset.gl = Some(true);
    f.charset.code_ranges = vec![(1, 9), (12, 99)];
    let id = f.add_glyph(Glyph::default()).unwrap();
    f.remove_glyph(id);
    let mut g = Glyph::new(".#\n##".parse::<Bitmap>().unwrap());
    g.name = Some("name\nwith newline".into());
    g.codepoints = vec![
        Codepoint::Unicode(65),
        Codepoint::Legacy(123),
        Codepoint::Unicode(0x110000),
    ];
    g.unicode_sequences = vec![vec![1, 2]];
    g.metrics.bounds.x = -3;
    g.metrics.advance = Some(Vector::new(9, 3));
    g.metrics.scalable_advance = Some(Vector::new(700, 0));
    g.metrics.vertical_origin = Some(Vector::new(-1, 2));
    g.properties.push(Property {
        name: "duplicate".into(),
        value: PropertyValue::List(vec![
            PropertyValue::Flag,
            PropertyValue::Bytes(vec![0, 255]),
            PropertyValue::Unsigned(u64::MAX),
            PropertyValue::Decimal("1.0".into()),
        ]),
        raw: Some(vec![255, 0]),
    });
    g.properties.push(Property::new("duplicate", 42));
    let id = f.add_glyph(g).unwrap();
    f.metrics.default_glyph = Some(id);
    f.add_glyph(Glyph::new(
        Bitmap::<Gray8>::from_raw(3, 1, &[0, 127, 255], Default::default()).unwrap(),
    ))
    .unwrap();
    let hole = f.add_glyph(Glyph::default()).unwrap();
    f.remove_glyph(hole);
    let mut set = FontSet::from(f);
    set.properties.push(Property::new("source", "arbitrary"));
    roundtrip(&set);
    let bytes = yaff::write(&set, &YaffWriteOptions::default()).unwrap().0;
    let s = String::from_utf8(bytes)
        .unwrap()
        .replace("family: Family", "family: Changed")
        .replace("left-bearing: -3", "left-bearing: -4")
        .replace("....@@", "@@..@@");
    let back = parse(&s);
    assert_eq!(back.fonts[0].names.family.as_deref(), Some("Changed"));
    let g = back.fonts[0].glyph(id).unwrap();
    assert_eq!(g.metrics.bounds.x, -4);
    assert_eq!(g.metrics.advance, Some(Vector::new(8, 3)));
}
#[test]
fn gray_levels_and_empty_dimensions() {
    for (levels, row, bytes) in [
        (4, ".12@", vec![0, 85, 170, 255]),
        (16, ".17E@", vec![0, 17, 119, 238, 255]),
        (256, "..017FFE@@", vec![0, 1, 127, 254, 255]),
    ] {
        let set = parse(&format!("levels: {levels}\nu+0041:\n    {row}\n"));
        assert_eq!(
            set.fonts[0].glyphs().next().unwrap().1.bitmap.as_bytes(),
            bytes
        );
        roundtrip(&set);
    }
    for (w, h) in [(0, 0), (0, 3), (5, 0)] {
        let set = FontSet::from(
            Font::from_glyphs([Glyph::new(
                Bitmap::<fontpixel::bitmap::Mono>::new(w, h).unwrap(),
            )])
            .unwrap(),
        );
        roundtrip(&set);
    }
    roundtrip(&FontSet::default());
    roundtrip(&FontSet {
        fonts: vec![Font::default(), Font::default()],
        ..FontSet::default()
    });
}
#[test]
fn malformed_and_bounded() {
    for s in [
        "u+0041:\n    @.\n    @\n",
        "u+0041:\n    x\n",
        "levels: 3\nu+0041:\n    @\n",
        "u+110000:\n    @\n",
        "u+D800:\n    @\n",
        "name:\n",
        "u+0041:\n    @\nname: late\n",
        "u+0041:\n    @\n    left-bearing: 9223372036854775807\n",
        "levels: 256\nu+0041:\n    1\n",
    ] {
        assert!(
            yaff::read(s.as_bytes(), &ResourceLimits::default()).is_err(),
            "{s}"
        );
    }
    assert!(yaff::read(&[255], &ResourceLimits::default()).is_err());
    let input = b"u+0041:\n    @@\n    @@\n";
    for limits in [
        ResourceLimits {
            max_input_bytes: 1,
            ..Default::default()
        },
        ResourceLimits {
            max_glyphs: 0,
            ..Default::default()
        },
        ResourceLimits {
            max_pixels_per_glyph: 3,
            ..Default::default()
        },
        ResourceLimits {
            max_table_bytes: 3,
            ..Default::default()
        },
        ResourceLimits {
            max_total_bytes: 20,
            ..Default::default()
        },
    ] {
        assert!(yaff::read(input, &limits).is_err());
    }
    let set = parse(std::str::from_utf8(input).unwrap());
    assert!(
        yaff::write(
            &set,
            &YaffWriteOptions {
                max_output_bytes: 10,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn registered_detection_gzip() {
    use fontpixel::formats::Confidence;
    use fontpixel::{ReadOptions, ResourcePath, WriteOptions};
    assert_eq!(yaff::detect(b"yaff: 1.0\n"), Confidence::Certain);
    assert_eq!(yaff::detect(b"u+0041:\n    @.\n"), Confidence::Possible);
    assert_eq!(yaff::detect(b"not a font"), Confidence::NoMatch);
    let set = parse("u+0041:\n    @.\n");
    let bytes = fontpixel::write(
        &set,
        "yaff",
        &WriteOptions::new(ResourcePath::new("out.yaff.gz").unwrap()),
    )
    .unwrap()
    .output
    .into_single()
    .unwrap();
    let back = fontpixel::read(&bytes, &ReadOptions::default()).unwrap();
    assert_eq!(back.fonts, set);
}
#[cfg(feature = "bdf")]
#[test]
fn bdf_exact_model_roundtrips() {
    for bytes in [
        include_bytes!("../../../tests/data/bdf/3x5-block.bdf").as_slice(),
        include_bytes!("../../../tests/data/bdf/spleen-5x8.bdf").as_slice(),
    ] {
        let font = fontpixel::formats::bdf::read(bytes, &ResourceLimits::default())
            .unwrap()
            .0;
        roundtrip(&FontSet::from(font));
    }
}
#[test]
fn punctuation_tags_zero_defaults_and_noncharacters() {
    let set = parse("u+0041:\n\"a: b\":\n    @\n\n    right-bearing: 0\n");
    roundtrip(&set);
    let mut f = Font::default();
    f.names.family = Some("\u{fdd0}\u{0000}".into());
    let g = Glyph {
        name: Some("\u{ffff}\u{0001}".into()),
        ..Glyph::default()
    };
    f.add_glyph(g).unwrap();
    roundtrip(&FontSet::from(f));
}
#[test]
fn multiline_record_budget_and_schema_validation() {
    let limits = ResourceLimits {
        max_table_bytes: 15,
        ..ResourceLimits::default()
    };
    assert!(yaff::read(b"notice:\n    1234567890\n    1234567890\n", &limits).is_err());
    let bytes = yaff::write(
        &FontSet::from(Font::default()),
        &YaffWriteOptions::default(),
    )
    .unwrap()
    .0;
    let text = String::from_utf8(bytes).unwrap();
    for text in [
        text.replace("\"version\":1", "\"version\":9"),
        text.replace("\"slots\":0", "\"slots\":18446744073709551615"),
        text.replace(
            "\"glyph_defaults\":",
            "\"default_glyph\":0,\"glyph_defaults\":",
        ),
    ] {
        assert!(yaff::read(text.as_bytes(), &ResourceLimits::default()).is_err());
    }
}
#[test]
fn glyph_resize_recomputes_visible_advance() {
    let set = parse("u+0041:\n    @\n\n    right-bearing: 2\n");
    let bytes = yaff::write(&set, &YaffWriteOptions::default()).unwrap().0;
    let text = String::from_utf8(bytes)
        .unwrap()
        .replace("    @\n", "    @@\n");
    let back = parse(&text);
    assert_eq!(
        back.fonts[0].glyphs().next().unwrap().1.metrics.advance,
        Some(Vector::new(4, 0))
    );
}
#[test]
fn committed_canonical_golden() {
    let input = include_bytes!("../../../tests/data/yaff/portable.yaff");
    let expected = include_bytes!("../../../tests/data/yaff/portable.golden.yaff");
    let set = yaff::read(input, &ResourceLimits::default()).unwrap().0;
    assert_eq!(
        yaff::write(&set, &YaffWriteOptions::default()).unwrap().0,
        expected
    );
    assert_eq!(
        yaff::read(expected, &ResourceLimits::default()).unwrap().0,
        set
    );
}
#[test]
fn duplicate_properties_and_empty_multiline_values() {
    let set = parse(
        "Family: first\nFAMILY: second\nnotice:\n    \"\"\n    \"\"\nu+0041:\n    @\n\n    right-bearing: 1\n    right-bearing: 2\n",
    );
    assert_eq!(set.fonts[0].names.family.as_deref(), Some("second"));
    assert_eq!(set.fonts[0].names.license.as_deref(), Some("\n"));
    assert_eq!(
        set.fonts[0]
            .properties
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case("family"))
            .count(),
        2
    );
    assert_eq!(set.fonts[0].glyphs().next().unwrap().1.properties.len(), 2);
    roundtrip(&set);
}

#[test]
fn unicode_whitespace_at_value_edges_survives() {
    // The reader trims all Unicode whitespace, so the writer must quote values
    // that start or end with any of it, not only ASCII space and tab.
    for name in [
        "Name\u{a0}",
        "\u{a0}Name",
        "Name\u{3000}",
        "\u{2003}Name\u{2003}",
    ] {
        let mut font = Font::from_glyphs([Glyph::new("#".parse::<Bitmap>().unwrap())]).unwrap();
        font.names.family = Some(name.into());
        let set = FontSet::from(font);
        roundtrip(&set);
        let (bytes, _) = yaff::write(&set, &YaffWriteOptions::default()).unwrap();
        let back = yaff::read(&bytes, &ResourceLimits::default()).unwrap().0;
        assert_eq!(back.fonts[0].names.family.as_deref(), Some(name));
    }
}
