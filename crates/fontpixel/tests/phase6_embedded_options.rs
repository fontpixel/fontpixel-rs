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

mod metadata {
    use super::*;
    use fontpixel::formats::embedded::{
        EmbeddedAverageWidth as Average, EmbeddedCmapUnicodeFull as CmapFull,
        EmbeddedIndexLayout as Layout, EmbeddedMetadataOptions, EmbeddedPostNames as PostNames,
        EmbeddedReadOptions, EmbeddedUnicodeRanges as Ranges, EmbeddedWidthMax as WidthMax,
    };
    use fontpixel::sfnt::name::NameRecord;
    use std::sync::Arc;

    fn u32_at(bytes: &[u8], offset: usize) -> u32 {
        u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }
    fn i64_at(bytes: &[u8], offset: usize) -> i64 {
        i64::from_be_bytes(bytes[offset..offset + 8].try_into().unwrap())
    }
    /// Ten identical 3x2 glyphs (U+0041..U+004A), a narrow glyph, an empty
    /// space with a wide advance, and a supplementary-plane glyph.
    fn sample(copyright: Option<&str>) -> FontSet {
        let mut f = Font::default();
        f.names.family = Some("Sample".into());
        f.names.style = Some("Regular".into());
        f.names.copyright = copyright.map(str::to_owned);
        f.names.foundry = Some("Foundry".into());
        f.metrics.pixel_size = Some(8);
        f.metrics.ascent = Some(6);
        f.metrics.descent = Some(2);
        for cp in 0x41..0x4b {
            let mut g = Glyph::new("###\n#.#".parse::<Bitmap>().unwrap());
            g.codepoints.push(Codepoint::Unicode(cp));
            g.name = Some(format!("u{cp:04X}"));
            g.metrics.advance = Some(Vector::new(4, 0));
            f.add_glyph(g).unwrap();
        }
        f.add_glyph(glyph(0x4b, 3)).unwrap();
        let mut space = Glyph::new(Bitmap::<fontpixel::bitmap::Mono>::default());
        space.codepoints.push(Codepoint::Unicode(0x20));
        space.metrics.advance = Some(Vector::new(7, 0));
        f.add_glyph(space).unwrap();
        f.add_glyph(glyph(0x1f600, 9)).unwrap();
        FontSet {
            fonts: vec![f],
            ..Default::default()
        }
    }
    fn tong_like() -> EmbeddedMetadataOptions {
        let mut names = NameTable::default();
        for (id, text) in [
            (0, "Replacement copyright"),
            (1, "Sample"),
            (2, "Regular"),
            (3, "Sample Regular 1.0"),
            (4, "Sample Regular"),
            (5, "Version 1.0"),
            (6, "Sample-Regular"),
            (16, "Sample"),
            (17, "Regular"),
        ] {
            names.records.push(NameRecord::unicode(id, 0x409, text));
        }
        EmbeddedMetadataOptions {
            names: Some(Arc::new(names)),
            head_flags: Some(3),
            lowest_rec_ppem: Some(14),
            timestamps: Some([3_873_996_467, -1]),
            os2_version: Some(3),
            vendor_id: Some(*b"TONG"),
            panose: Some([2, 11, 5, 9, 0, 0, 0, 0, 0, 0]),
            unicode_ranges: Ranges::NonBmpOnly,
            break_char: Some(32),
            embedding_flags: Some(4),
            selection_flags: Some(0),
            average_width: Average::Floor,
            post_names: PostNames::Omit,
            fixed_pitch: Some(true),
            cmap_unicode_full: CmapFull::NonBmpOnly,
            width_max: WidthMax::Advance,
            index_layout: Layout::SharedMetricRuns { minimum_run: 8 },
        }
    }
    fn decoded(bytes: &[u8], kind: Kind) -> Vec<(Vec<Codepoint>, String, i64, Option<Vector>)> {
        let (set, _) = embedded::read(bytes, kind, &EmbeddedReadOptions::default()).unwrap();
        set.fonts[0]
            .glyphs()
            .map(|(_, g)| {
                (
                    g.codepoints.clone(),
                    format!("{:?}", g.bitmap),
                    g.metrics.bounds.x,
                    g.metrics.advance,
                )
            })
            .collect()
    }

    #[test]
    fn explicit_default_metadata_is_byte_and_report_identical() {
        let set = sample(Some("Notice"));
        for kind in [Kind::OpenType, Kind::Apple] {
            let baseline = embedded::write(&set, kind, &Default::default()).unwrap();
            let explicit = EmbeddedWriteOptions {
                metadata: EmbeddedMetadataOptions::default(),
                ..Default::default()
            };
            assert_eq!(embedded::write(&set, kind, &explicit).unwrap(), baseline);
            assert!(baseline.1.entries.iter().all(|e| {
                !e.code.starts_with("embedded.metadata")
                    && e.code != "embedded.post_names"
                    && e.code != "embedded.index_layout"
            }));
            let sfnt = Sfnt::parse(&baseline.0, &Default::default()).unwrap();
            let head = sfnt.table(*b"head").unwrap();
            assert_eq!((u16_at(head, 16), u16_at(head, 46)), (11, 8));
            assert_eq!((i64_at(head, 20), i64_at(head, 28)), (0, 0));
            let os2 = Os2Table::decode(sfnt.table(*b"OS/2").unwrap(), &Default::default()).unwrap();
            assert_eq!((os2.version, &os2.base.vendor_id), (4, b"NONE"));
            assert_eq!(u32_at(sfnt.table(*b"post").unwrap(), 0), 0x20000);
        }
    }

    #[test]
    fn every_metadata_control_reaches_its_table_and_is_audited() {
        let set = sample(Some("Source notice"));
        let baseline = embedded::write(&set, Kind::OpenType, &Default::default()).unwrap();
        let options = EmbeddedWriteOptions {
            metadata: tong_like(),
            ..Default::default()
        };
        for kind in [Kind::OpenType, Kind::Apple] {
            let (bytes, report) = embedded::write(&set, kind, &options).unwrap();
            let sfnt = Sfnt::parse(&bytes, &Default::default()).unwrap();
            sfnt.validate_checksums().unwrap();
            let head = sfnt.table(*b"head").unwrap();
            assert_eq!((u16_at(head, 16), u16_at(head, 46)), (3, 14));
            assert_eq!((i64_at(head, 20), i64_at(head, 28)), (3_873_996_467, -1));
            let os2 = Os2Table::decode(sfnt.table(*b"OS/2").unwrap(), &Default::default()).unwrap();
            assert_eq!(os2.version, 3);
            assert_eq!(&os2.base.vendor_id, b"TONG");
            assert_eq!(os2.base.panose, [2, 11, 5, 9, 0, 0, 0, 0, 0, 0]);
            assert_eq!(os2.base.unicode_ranges, [0, 1 << 25, 0, 0]);
            assert_eq!(os2.extension.as_ref().unwrap().break_char, 32);
            assert_eq!((os2.base.embedding_flags, os2.base.selection_flags), (4, 0));
            // Nonzero design advances: ten of 400, then 300, 700 and 900.
            // Their mean 5900/13 = 453.8 floors to 453 (native nearest 454).
            assert_eq!(os2.base.average_width, 453);
            let native = Sfnt::parse(&baseline.0, &Default::default()).unwrap();
            let native =
                Os2Table::decode(native.table(*b"OS/2").unwrap(), &Default::default()).unwrap();
            assert_eq!(native.base.average_width, 454);
            let post_bytes = sfnt.table(*b"post").unwrap();
            assert_eq!(u32_at(post_bytes, 0), 0x30000);
            let post = PostTable::decode(post_bytes, 14, &Default::default()).unwrap();
            assert!(post.names.is_none());
            assert_eq!(post.header.is_fixed_pitch, 1);
            let names =
                NameTable::decode(sfnt.table(*b"name").unwrap(), &Default::default()).unwrap();
            let ids: Vec<_> = names.records.iter().map(|r| r.name_id).collect();
            assert_eq!(ids, [0, 1, 2, 3, 4, 5, 6, 16, 17]);
            let (_, location) = kind.tags();
            // widthMax is the largest advance (9), not the largest bitmap
            // width (3).
            assert_eq!(sfnt.table(location).unwrap()[8 + 16 + 2], 9);
            let codes: Vec<_> = report.entries.iter().map(|e| e.code.as_str()).collect();
            for expected in [
                "embedded.metadata_configuration",
                "embedded.post_names",
                "embedded.index_layout",
            ] {
                assert!(codes.contains(&expected), "{expected}: {codes:?}");
            }
            // Source copyright (ID 0) and foundry (ID 8) are dropped: two losses.
            assert_eq!(
                codes
                    .iter()
                    .filter(|c| **c == "embedded.metadata_name")
                    .count(),
                2
            );
            // Every native diagnostic of the default write remains.
            for entry in &baseline.1.entries {
                assert!(report.entries.contains(entry), "{entry:?}");
            }
            // Pixels, mappings and horizontal metrics are unchanged.
            assert_eq!(decoded(&bytes, kind), decoded(&baseline.0, Kind::OpenType));
        }
    }

    #[test]
    fn coverage_exact_and_non_bmp_unicode_ranges() {
        let write = |set: &FontSet, unicode_ranges| {
            let options = EmbeddedWriteOptions {
                metadata: EmbeddedMetadataOptions {
                    unicode_ranges,
                    ..Default::default()
                },
                ..Default::default()
            };
            let bytes = embedded::write(set, Kind::OpenType, &options).unwrap().0;
            let sfnt = Sfnt::parse(&bytes, &Default::default()).unwrap();
            Os2Table::decode(sfnt.table(*b"OS/2").unwrap(), &Default::default())
                .unwrap()
                .base
                .unicode_ranges
        };
        let set = sample(None);
        assert_ne!(write(&set, Ranges::Coverage), [0; 4]);
        assert_eq!(write(&set, Ranges::Exact([1, 2, 3, 4])), [1, 2, 3, 4]);
        assert_eq!(write(&set, Ranges::NonBmpOnly), [0, 1 << 25, 0, 0]);
        let bmp = FontSet {
            fonts: vec![font(8, 8, "Bmp", &[(65, 4), (66, 4)])],
            ..Default::default()
        };
        assert_eq!(write(&bmp, Ranges::NonBmpOnly), [0; 4]);
    }

    #[test]
    fn bmp_only_cmap_can_omit_the_redundant_unicode_full_record() {
        let records = |set: &FontSet, cmap_unicode_full| {
            let options = EmbeddedWriteOptions {
                metadata: EmbeddedMetadataOptions {
                    cmap_unicode_full,
                    ..Default::default()
                },
                ..Default::default()
            };
            let bytes = embedded::write(set, Kind::OpenType, &options).unwrap().0;
            let sfnt = Sfnt::parse(&bytes, &Default::default()).unwrap();
            let table = sfnt.table(*b"cmap").unwrap();
            let keys: Vec<_> = (0..usize::from(u16_at(table, 2)))
                .map(|i| (u16_at(table, 4 + 8 * i), u16_at(table, 6 + 8 * i)))
                .collect();
            (keys, cmap::decode(table, 64, &Default::default()).unwrap())
        };
        let bmp = FontSet {
            fonts: vec![font(8, 8, "Bmp", &[(65, 4), (0x4e00, 8)])],
            ..Default::default()
        };
        let (native, mapping) = records(&bmp, CmapFull::Always);
        assert_eq!(native, [(0, 3), (0, 4), (3, 1), (3, 10)]);
        let (omitted, same) = records(&bmp, CmapFull::NonBmpOnly);
        assert_eq!(omitted, [(0, 3), (3, 1), (3, 10)]);
        assert_eq!(same, mapping);
        let set = sample(None);
        assert_eq!(
            records(&set, CmapFull::NonBmpOnly),
            records(&set, CmapFull::Always)
        );
    }

    #[test]
    fn shared_metric_runs_match_the_export_rs_index_structure() {
        let set = sample(None);
        for kind in [Kind::OpenType, Kind::Apple] {
            let canonical = embedded::write(&set, kind, &Default::default()).unwrap();
            for (minimum_run, runs) in [(8, 1), (11, 0), (1, 3)] {
                let options = EmbeddedWriteOptions {
                    metadata: EmbeddedMetadataOptions {
                        index_layout: Layout::SharedMetricRuns { minimum_run },
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let (bytes, report) = embedded::write(&set, kind, &options).unwrap();
                let sfnt = Sfnt::parse(&bytes, &Default::default()).unwrap();
                let (data_tag, location_tag) = kind.tags();
                let location = sfnt.table(location_tag).unwrap();
                let data = sfnt.table(data_tag).unwrap();
                let array = u32_at(location, 8) as usize;
                let subtables = u32_at(location, 16) as usize;
                let formats: Vec<_> = (0..subtables)
                    .map(|i| {
                        let at = array + u32_at(location, array + i * 8 + 4) as usize;
                        (u16_at(location, at), u16_at(location, at + 2))
                    })
                    .collect();
                assert_eq!(
                    formats.iter().filter(|f| **f == (2, 5)).count(),
                    runs,
                    "{minimum_run}: {formats:?}"
                );
                assert!(formats.iter().all(|f| *f == (2, 5) || *f == (3, 2)));
                assert_eq!(location[8 + 47], 1);
                if minimum_run == 8 {
                    // GID order: .notdef, U+0020, U+0041..U+004A (run), U+004B, U+1F600.
                    assert_eq!(formats, [(3, 2), (2, 5), (3, 2)]);
                    let at = array + u32_at(location, array + 12) as usize;
                    // One 6-pixel image per glyph, then BigGlyphMetrics with
                    // zero vertical fields.
                    assert_eq!(u32_at(location, at + 8), 1);
                    assert_eq!(&location[at + 12..at + 20], &[2, 3, 0, 2, 4, 0, 0, 0]);
                    // Bit-aligned rows ### and #.# pack into 1111_01xx.
                    let image = u32_at(location, at + 4) as usize;
                    assert_eq!(data[image], 0b1111_0100);
                }
                assert_eq!(
                    report
                        .entries
                        .iter()
                        .filter(|e| e.code == "embedded.index_layout")
                        .count(),
                    usize::from(runs > 0)
                );
                assert_eq!(decoded(&bytes, kind), decoded(&canonical.0, kind));
            }
        }
    }

    #[test]
    fn shared_metric_runs_reject_vertical_metrics_and_zero_runs() {
        let mut set = sample(None);
        let options = EmbeddedWriteOptions {
            metadata: EmbeddedMetadataOptions {
                index_layout: Layout::SharedMetricRuns { minimum_run: 0 },
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(embedded::write(&set, Kind::OpenType, &options).is_err());
        let id = set.fonts[0].glyphs().next().unwrap().0;
        set.fonts[0]
            .edit_glyph(id, |g| {
                g.metrics.vertical_advance = Some(Vector::new(0, -8));
                Ok(())
            })
            .unwrap();
        let options = EmbeddedWriteOptions {
            metadata: EmbeddedMetadataOptions {
                index_layout: Layout::SharedMetricRuns { minimum_run: 8 },
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(
            embedded::write(&set, Kind::OpenType, &options)
                .unwrap_err()
                .to_string()
                .contains("shared-metric run layout")
        );
    }

    #[test]
    fn invalid_metadata_is_rejected_before_writing() {
        let set = sample(None);
        let names = |records, language_tags| {
            Some(Arc::new(NameTable {
                records,
                language_tags,
            }))
        };
        let bad = [
            EmbeddedMetadataOptions {
                head_flags: Some(0x8000),
                ..Default::default()
            },
            EmbeddedMetadataOptions {
                lowest_rec_ppem: Some(0),
                ..Default::default()
            },
            EmbeddedMetadataOptions {
                os2_version: Some(2),
                ..Default::default()
            },
            EmbeddedMetadataOptions {
                os2_version: Some(5),
                ..Default::default()
            },
            EmbeddedMetadataOptions {
                vendor_id: Some(*b"T\0NG"),
                ..Default::default()
            },
            EmbeddedMetadataOptions {
                unicode_ranges: Ranges::Exact([0, 0, 0, 1 << 27]),
                ..Default::default()
            },
            EmbeddedMetadataOptions {
                embedding_flags: Some(1),
                ..Default::default()
            },
            EmbeddedMetadataOptions {
                embedding_flags: Some(0x6),
                ..Default::default()
            },
            EmbeddedMetadataOptions {
                selection_flags: Some(0x400),
                ..Default::default()
            },
            EmbeddedMetadataOptions {
                os2_version: Some(3),
                selection_flags: Some(0x80),
                ..Default::default()
            },
            EmbeddedMetadataOptions {
                names: names(vec![NameRecord::unicode(6, 0x409, "Bad Name")], vec![]),
                ..Default::default()
            },
            EmbeddedMetadataOptions {
                names: names(
                    vec![NameRecord::unicode(1, 0x8000, "Family")],
                    vec!["en_US".into()],
                ),
                ..Default::default()
            },
            EmbeddedMetadataOptions {
                names: names(
                    vec![
                        NameRecord::unicode(1, 0x409, "A"),
                        NameRecord::unicode(1, 0x409, "B"),
                    ],
                    vec![],
                ),
                ..Default::default()
            },
        ];
        for metadata in bad {
            let options = EmbeddedWriteOptions {
                metadata,
                ..Default::default()
            };
            for kind in [Kind::OpenType, Kind::Apple] {
                assert!(
                    embedded::write(&set, kind, &options).is_err(),
                    "{options:?}"
                );
            }
        }
        #[cfg(feature = "color")]
        assert_eq!(
            embedded::write(
                &set,
                Kind::Color,
                &EmbeddedWriteOptions {
                    metadata: EmbeddedMetadataOptions {
                        break_char: Some(32),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            )
            .unwrap_err()
            .to_string(),
            "embedded bitmap: custom embedded metadata requires otb or bdat"
        );
    }

    fn minimum_limit(set: &FontSet, metadata: &EmbeddedMetadataOptions) -> usize {
        let fits = |limit: usize| {
            let mut options = EmbeddedWriteOptions {
                metadata: metadata.clone(),
                ..Default::default()
            };
            options.limits.sfnt.resources.max_total_bytes = limit;
            embedded::write(set, Kind::OpenType, &options).is_ok()
        };
        let (mut low, mut high) = (0, 64 << 20);
        assert!(fits(high));
        while low < high {
            let mid = low + (high - low) / 2;
            if fits(mid) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        assert!(!fits(low - 1));
        low
    }

    #[test]
    fn explicit_name_table_capacity_is_charged_to_the_writer_budget() {
        let set = sample(None);
        let small = tong_like();
        let with_spare = |spare: usize| {
            let mut table = (**small.names.as_ref().unwrap()).clone();
            table.records[0].value.reserve_exact(spare);
            let extra = table.records[0].value.capacity() - table.records[0].value.len();
            let options = EmbeddedMetadataOptions {
                names: Some(Arc::new(table)),
                ..small.clone()
            };
            let bytes = embedded::write(
                &set,
                Kind::OpenType,
                &EmbeddedWriteOptions {
                    metadata: options.clone(),
                    ..Default::default()
                },
            )
            .unwrap()
            .0;
            (minimum_limit(&set, &options), extra, bytes)
        };
        let base = minimum_limit(&set, &small);
        let (one, extra_one, bytes_one) = with_spare(1 << 20);
        let (two, extra_two, bytes_two) = with_spare(2 << 20);
        // Identical output; once the table dominates the budget, every
        // retained spare byte is charged exactly once.
        assert_eq!(bytes_one, bytes_two);
        assert!(one > base && one >= base + extra_one - 4096, "{base} {one}");
        assert_eq!(two - one, extra_two - extra_one);
        let without = minimum_limit(&set, &EmbeddedMetadataOptions::default());
        assert!(base >= without, "{base} {without}");
    }

    #[test]
    fn strict_registry_writes_reject_dropped_notices_and_glyph_names() {
        let set = sample(Some("Source notice"));
        for (metadata, code) in [
            (
                EmbeddedMetadataOptions {
                    post_names: PostNames::Omit,
                    ..Default::default()
                },
                "embedded.post_names",
            ),
            (
                EmbeddedMetadataOptions {
                    names: tong_like().names,
                    ..Default::default()
                },
                "embedded.metadata_name",
            ),
        ] {
            let mut registry = fontpixel::registry();
            registry.set_embedded_write_options(EmbeddedWriteOptions {
                metadata,
                ..Default::default()
            });
            let mut write = WriteOptions::new(ResourcePath::new("out.otb").unwrap());
            let written = registry.write(&set, "otb", &write).unwrap();
            assert!(written.report.entries.iter().any(|e| e.code == code));
            write.strict = true;
            let fontpixel::Error::StrictLoss(report) =
                registry.write(&set, "otb", &write).unwrap_err()
            else {
                panic!("expected strict loss")
            };
            assert!(report.entries.iter().any(|e| e.code == code));
        }
    }
}
