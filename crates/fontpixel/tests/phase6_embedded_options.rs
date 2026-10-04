//! Explicit common-metric policies retain native bitmap and naming semantics.
#![cfg(any(feature = "otb", feature = "bdat"))]

use fontpixel::formats::embedded::{
    self, EmbeddedReferenceStrike as Reference, EmbeddedRounding as Rounding, EmbeddedWriteOptions,
};
use fontpixel::model::{Codepoint, Spacing, Vector};
use fontpixel::sfnt::{
    Sfnt,
    bitmap::Kind,
    cmap,
    metrics::{self, Axis},
    name::NameTable,
    os2::Os2Table,
    post::PostTable,
};
use fontpixel::{Bitmap, Font, FontSet, Glyph, Input, ReadOptions, ResourcePath, WriteOptions};

fn glyph(cp: u32, advance: i64) -> Glyph {
    let mut glyph = Glyph::new("#".parse::<Bitmap>().unwrap());
    glyph.codepoints.push(Codepoint::Unicode(cp));
    glyph.name = Some(format!("u{cp:04X}"));
    glyph.metrics.advance = Some(Vector::new(advance, 0));
    glyph
}
fn font(ppem_x: u32, ppem_y: u32, family: &str, codes: &[(u32, i64)]) -> Font {
    let mut font = Font::default();
    font.names.family = Some(family.into());
    font.names.style = Some("Regular".into());
    font.metrics.pixel_size = Some(ppem_y);
    font.metrics.ppem_x = Some(ppem_x);
    font.metrics.ppem_y = Some(ppem_y);
    font.metrics.ascent = Some(i64::from(ppem_y));
    font.metrics.descent = Some(0);
    for &(cp, advance) in codes {
        font.add_glyph(glyph(cp, advance)).unwrap();
    }
    font
}
fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

#[test]
fn explicit_metric_reference_and_rounding_leave_names_and_bitmap_tables_unchanged() {
    let mut fonts = Vec::new();
    for (index, (ppem, ascent)) in [(24, 19), (32, 26), (64, 52), (8, 6), (12, 9), (16, 13)]
        .into_iter()
        .enumerate()
    {
        let mut f = font(
            ppem,
            ppem,
            if index == 0 {
                "Naming source"
            } else {
                "Metric source"
            },
            &[(65, i64::from(ascent))],
        );
        f.metrics.ascent = Some(i64::from(ascent));
        f.metrics.descent = Some(i64::from(ppem - ascent));
        f.metrics.line_gap = Some(if index == 2 { 52 } else { 1 });
        f.metrics.underline_position = Some(if index == 2 { -52 } else { -1 });
        f.metrics.underline_thickness = Some(if index == 2 { 52 } else { 1 });
        f.metrics.spacing = Some(if index == 2 {
            Spacing::Proportional
        } else {
            Spacing::Monospaced
        });
        let id = f.glyphs().next().unwrap().0;
        f.edit_glyph(id, |g| {
            g.metrics.bounds.x = -4;
            Ok(())
        })
        .unwrap();
        fonts.push(f);
    }
    let set = FontSet {
        fonts,
        ..Default::default()
    };
    for kind in [Kind::OpenType, Kind::Apple] {
        let baseline = embedded::write(&set, kind, &Default::default()).unwrap();
        let explicit_defaults = EmbeddedWriteOptions {
            reference_strike: Reference::Index(0),
            ..Default::default()
        };
        assert_eq!(
            embedded::write(&set, kind, &explicit_defaults).unwrap(),
            baseline
        );
        let original = Sfnt::parse(&baseline.0, &Default::default()).unwrap();
        assert_eq!(u16_at(original.table(*b"head").unwrap(), 18), 2400);
        let options = EmbeddedWriteOptions {
            units_per_em: Some(1000),
            reference_strike: Reference::LargestPpem,
            rounding: Rounding::NearestTiesEven,
            ..Default::default()
        };
        let (bytes, report) = embedded::write(&set, kind, &options).unwrap();
        let sfnt = Sfnt::parse(&bytes, &Default::default()).unwrap();
        sfnt.validate_checksums().unwrap();
        assert_eq!(u16_at(sfnt.table(*b"head").unwrap(), 18), 1000);
        let (data, location) = kind.tags();
        assert_eq!(sfnt.table(data), original.table(data));
        assert_eq!(sfnt.table(location), original.table(location));
        assert_eq!(sfnt.table(*b"name"), original.table(*b"name"));
        let names = NameTable::decode(sfnt.table(*b"name").unwrap(), &Default::default()).unwrap();
        assert!(
            names
                .records
                .iter()
                .any(|r| r.name_id == 1 && r.text().unwrap().as_deref() == Some("Naming source"))
        );
        let (header, metrics) = metrics::decode(
            Axis::Horizontal,
            sfnt.table(*b"hhea").unwrap(),
            sfnt.table(*b"hmtx").unwrap(),
            2,
            &Default::default(),
        )
        .unwrap();
        assert_eq!(
            (header.ascender, header.descender, header.line_gap),
            (812, -188, 812)
        );
        assert_eq!((metrics[1].advance, metrics[1].bearing), (812, -62));
        let os2 = Os2Table::decode(sfnt.table(*b"OS/2").unwrap(), &Default::default()).unwrap();
        assert_eq!(os2.base.typo_metrics, [812, -188, 812]);
        assert_eq!(os2.base.windows_metrics, [812, 188]);
        let post =
            PostTable::decode(sfnt.table(*b"post").unwrap(), 2, &Default::default()).unwrap();
        assert_eq!(
            (
                post.header.underline_position,
                post.header.underline_thickness,
                post.header.is_fixed_pitch
            ),
            (-812, 812, 0)
        );
        assert!(
            report
                .entries
                .iter()
                .any(|e| e.code == "embedded.design_rounding" && e.location.glyph_index == Some(1))
        );
        let away = EmbeddedWriteOptions {
            rounding: Rounding::NearestTiesAwayFromZero,
            ..options
        };
        let (away, _) = embedded::write(&set, kind, &away).unwrap();
        let sfnt = Sfnt::parse(&away, &Default::default()).unwrap();
        let (header, metrics) = metrics::decode(
            Axis::Horizontal,
            sfnt.table(*b"hhea").unwrap(),
            sfnt.table(*b"hmtx").unwrap(),
            2,
            &Default::default(),
        )
        .unwrap();
        assert_eq!(
            (header.ascender, metrics[1].advance, metrics[1].bearing),
            (813, 813, -63)
        );
    }
}

#[test]
fn largest_uses_actual_ppem_last_ties_and_descending_missing_glyph_fallback() {
    let mut set = FontSet {
        fonts: vec![
            font(10, 10, "First", &[(65, 1), (66, 1), (67, 1), (68, 1)]),
            font(20, 20, "Second", &[(65, 4), (66, 8), (67, 8)]),
            font(30, 20, "Third", &[(65, 9), (66, 9)]),
            font(15, 15, "Fourth", &[(65, 5), (68, 6)]),
            font(30, 30, "Largest", &[(65, 12)]),
        ],
        ..Default::default()
    };
    // Misleading source/display sizes must not override the actual physical ppem.
    for (index, font) in set.fonts.iter_mut().enumerate() {
        font.metrics.pixel_size = Some(200 - index as u32);
    }
    let mut options = EmbeddedWriteOptions {
        units_per_em: Some(600),
        reference_strike: Reference::LargestPpem,
        ..Default::default()
    };
    let baseline = embedded::write(&set, Kind::OpenType, &Default::default()).unwrap();
    let (bytes, _) = embedded::write(&set, Kind::OpenType, &options).unwrap();
    let sfnt = Sfnt::parse(&bytes, &Default::default()).unwrap();
    let original = Sfnt::parse(&baseline.0, &Default::default()).unwrap();
    for tag in [*b"EBDT", *b"EBLC", *b"cmap", *b"name"] {
        assert_eq!(sfnt.table(tag), original.table(tag));
    }
    let mapping = cmap::decode(sfnt.table(*b"cmap").unwrap(), 5, &Default::default()).unwrap();
    let (_, metrics) = metrics::decode(
        Axis::Horizontal,
        sfnt.table(*b"hhea").unwrap(),
        sfnt.table(*b"hmtx").unwrap(),
        5,
        &Default::default(),
    )
    .unwrap();
    for (cp, advance) in [(65, 240), (66, 180), (67, 240), (68, 240)] {
        assert_eq!(metrics[usize::from(mapping[&cp])].advance, advance);
    }
    options.reference_strike = Reference::Index(1);
    let (bytes, _) = embedded::write(&set, Kind::OpenType, &options).unwrap();
    let sfnt = Sfnt::parse(&bytes, &Default::default()).unwrap();
    let (_, metrics) = metrics::decode(
        Axis::Horizontal,
        sfnt.table(*b"hhea").unwrap(),
        sfnt.table(*b"hmtx").unwrap(),
        5,
        &Default::default(),
    )
    .unwrap();
    for (cp, advance) in [(65, 120), (66, 240), (67, 240), (68, 60)] {
        assert_eq!(metrics[usize::from(mapping[&cp])].advance, advance);
    }
    // With the largest removed, equal y-ppem selects the later index (x=30).
    set.fonts.pop();
    options.reference_strike = Reference::LargestPpem;
    let (bytes, _) = embedded::write(&set, Kind::OpenType, &options).unwrap();
    let sfnt = Sfnt::parse(&bytes, &Default::default()).unwrap();
    let (_, metrics) = metrics::decode(
        Axis::Horizontal,
        sfnt.table(*b"hhea").unwrap(),
        sfnt.table(*b"hmtx").unwrap(),
        5,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(metrics[usize::from(mapping[&65])].advance, 180);
    options.units_per_em = None;
    let (bytes, _) = embedded::write(&set, Kind::OpenType, &options).unwrap();
    assert_eq!(
        u16_at(
            Sfnt::parse(&bytes, &Default::default())
                .unwrap()
                .table(*b"head")
                .unwrap(),
            18
        ),
        2000
    );
}

#[test]
fn metric_options_validate_before_expensive_work_and_keep_limits_active() {
    let set = FontSet::from(font(1, 1, "Bounds", &[(65, 1)]));
    for upem in [16, 16384] {
        let options = EmbeddedWriteOptions {
            units_per_em: Some(upem),
            ..Default::default()
        };
        let (bytes, _) = embedded::write(&set, Kind::OpenType, &options).unwrap();
        assert_eq!(
            u16_at(
                Sfnt::parse(&bytes, &Default::default())
                    .unwrap()
                    .table(*b"head")
                    .unwrap(),
                18
            ),
            upem
        );
    }
    for upem in [0, 15, 16385, u16::MAX] {
        let mut options = EmbeddedWriteOptions {
            units_per_em: Some(upem),
            ..Default::default()
        };
        options.limits.sfnt.resources.max_total_bytes = 0;
        assert_eq!(
            embedded::write(&set, Kind::OpenType, &options)
                .unwrap_err()
                .to_string(),
            "embedded bitmap: units_per_em must be in 16..=16384"
        );
    }
    for index in [1, usize::MAX] {
        let mut options = EmbeddedWriteOptions {
            reference_strike: Reference::Index(index),
            ..Default::default()
        };
        options.limits.sfnt.resources.max_total_bytes = 0;
        assert_eq!(
            embedded::write(&set, Kind::OpenType, &options)
                .unwrap_err()
                .to_string(),
            "embedded bitmap: reference strike index is outside FontSet"
        );
    }
    for case in 0..4 {
        let mut options = EmbeddedWriteOptions {
            units_per_em: Some(1000),
            reference_strike: Reference::LargestPpem,
            rounding: Rounding::NearestTiesEven,
            ..Default::default()
        };
        match case {
            0 => options.limits.sfnt.resources.max_total_bytes = 0,
            1 => options.limits.sfnt.resources.max_glyphs = 0,
            2 => options.limits.max_pixel_work = 0,
            _ => options.limits.sfnt.resources.max_table_bytes = 0,
        }
        assert!(embedded::write(&set, Kind::OpenType, &options).is_err());
    }
    let mut overflow = set.clone();
    for value in [i64::MIN, i64::MAX] {
        overflow.fonts[0].metrics.line_gap = Some(value);
        let options = EmbeddedWriteOptions {
            units_per_em: Some(16384),
            rounding: Rounding::NearestTiesEven,
            ..Default::default()
        };
        assert_eq!(
            embedded::write(&overflow, Kind::OpenType, &options)
                .unwrap_err()
                .to_string(),
            "embedded bitmap: design metric does not fit int16"
        );
    }
    assert!(embedded::write(&FontSet::default(), Kind::OpenType, &Default::default()).is_err());
    let mut duplicate = set.clone();
    duplicate.fonts.push(duplicate.fonts[0].clone());
    assert_eq!(
        embedded::write(&duplicate, Kind::OpenType, &Default::default())
            .unwrap_err()
            .to_string(),
        "embedded bitmap: duplicate physical strike size"
    );
    let mut zero = set.clone();
    zero.fonts[0].metrics.ppem_y = Some(0);
    assert!(embedded::write(&zero, Kind::OpenType, &Default::default()).is_err());
}

#[test]
fn registry_options_match_typed_writes_preserve_readers_and_strict_reports() {
    let set = FontSet {
        fonts: vec![
            font(16, 16, "First", &[(65, 13)]),
            font(64, 64, "Second", &[(65, 52)]),
        ],
        ..Default::default()
    };
    let options = EmbeddedWriteOptions {
        units_per_em: Some(1000),
        reference_strike: Reference::LargestPpem,
        rounding: Rounding::NearestTiesEven,
        ..Default::default()
    };
    let mut registry = fontpixel::registry();
    #[cfg(feature = "color")]
    let color_before = registry
        .write(
            &set,
            "color",
            &WriteOptions::new(ResourcePath::new("out.otf").unwrap()),
        )
        .unwrap();
    registry.set_embedded_write_options(options.clone());
    for (format, kind, enabled) in [
        ("otb", Kind::OpenType, cfg!(feature = "otb")),
        ("bdat", Kind::Apple, cfg!(feature = "bdat")),
    ] {
        if !enabled {
            continue;
        }
        let expected = embedded::write(&set, kind, &options).unwrap();
        let mut write = WriteOptions::new(ResourcePath::new(format!("out.{format}")).unwrap());
        let actual = registry.write(&set, format, &write).unwrap();
        assert_eq!(actual.report, expected.1);
        assert_eq!(actual.output.into_single().unwrap(), expected.0);
        let loaded = registry
            .read(
                &Input::new(&expected.0),
                &ReadOptions {
                    format: Some(format.into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(loaded.fonts.fonts.len(), 2);
        write.strict = true;
        let fontpixel::Error::StrictLoss(report) =
            registry.write(&set, format, &write).unwrap_err()
        else {
            panic!("expected strict loss")
        };
        assert_eq!(report, expected.1);
    }
    #[cfg(feature = "color")]
    {
        let color_after = registry
            .write(
                &set,
                "color",
                &WriteOptions::new(ResourcePath::new("out.otf").unwrap()),
            )
            .unwrap();
        assert_eq!(color_before.report, color_after.report);
        assert_eq!(
            color_before.output.into_single().unwrap(),
            color_after.output.into_single().unwrap()
        );
        for custom in [
            EmbeddedWriteOptions {
                units_per_em: Some(1000),
                ..Default::default()
            },
            EmbeddedWriteOptions {
                reference_strike: Reference::Index(0),
                ..Default::default()
            },
            EmbeddedWriteOptions {
                rounding: Rounding::NearestTiesEven,
                ..Default::default()
            },
        ] {
            assert_eq!(
                embedded::write(&set, Kind::Color, &custom)
                    .unwrap_err()
                    .to_string(),
                "embedded bitmap: custom embedded metric controls require otb or bdat"
            );
        }
    }
}
