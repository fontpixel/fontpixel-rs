//! Inline singleton labels retain native sfnt bytes, identities and diagnostics.
#![cfg(feature = "otb")]

use fontpixel::formats::embedded::{self, EmbeddedWriteOptions};
use fontpixel::model::{Codepoint, Property, Vector};
use fontpixel::report::{Impact, Level, Location, ReportEntry};
use fontpixel::sfnt::{Sfnt, bitmap::Kind, cmap, post::PostTable};
use fontpixel::{Bitmap, Font, FontSet, Glyph, Report, ResourcePath, WriteOptions};
use std::collections::BTreeMap;

fn glyph(name: Option<&str>, codes: &[u32], pixels: &str, advance: i64) -> Glyph {
    let mut g = Glyph::new(pixels.parse::<Bitmap>().unwrap());
    g.name = name.map(str::to_owned);
    g.codepoints = codes.iter().copied().map(Codepoint::Unicode).collect();
    g.metrics.advance = Some(Vector::new(advance, 0));
    g
}
fn fixture(repeated: bool, ppem: u32) -> Font {
    let mut f = Font::default();
    f.metrics.pixel_size = Some(ppem);
    f.metrics.ascent = Some(i64::from(ppem) - 3);
    f.metrics.descent = Some(3);
    f.properties.push(Property::new("font.source", 99i64));
    let mut a = glyph(
        Some("A"),
        if repeated { &[65, 65] } else { &[65] },
        "#.#",
        4,
    );
    a.metrics.bounds.x = -1;
    a.metrics.scalable_advance = Some(Vector::new(500, 0));
    let mut b = glyph(
        Some("B"),
        if repeated {
            &[0x10000, 66, 66]
        } else {
            &[66, 0x10000]
        },
        ".#.",
        4,
    );
    b.properties.push(Property::new("glyph.source", 7i64));
    for g in [
        a,
        b,
        glyph(Some(".notdef"), &[], "...", 3),
        glyph(Some("bad/name"), &[], "#", 2),
        glyph(None, &[], "#.", 3),
    ] {
        f.add_glyph(g).unwrap();
    }
    f
}
fn expected_report() -> Report {
    let mut entries = Vec::new();
    for fi in 0..2 {
        for (code, gi, message) in [
            (
                "source_property",
                None,
                "Source property \"font.source\" is not represented",
            ),
            (
                "glyph_metric",
                Some(0),
                "Independent scalable_advance is not preserved",
            ),
            (
                "glyph_property",
                Some(1),
                "Glyph property \"glyph.source\" is not represented",
            ),
        ] {
            entries.push(ReportEntry {
                level: Level::Warning,
                impact: Impact::Loss,
                code: format!("embedded.{code}"),
                location: Location {
                    font_index: Some(fi),
                    glyph_index: gi,
                    ..Default::default()
                },
                message: message.into(),
            });
        }
        if fi == 0 {
            entries.push(ReportEntry {
                level: Level::Warning,
                impact: Impact::Loss,
                code: "embedded.glyph_name".into(),
                location: Location {
                    font_index: Some(0),
                    glyph_index: Some(3),
                    ..Default::default()
                },
                message: "Glyph name cannot be encoded in post format 2".into(),
            });
        }
    }
    Report { entries }
}

#[test]
fn singleton_and_deduped_labels_produce_identical_native_bytes_and_reports() {
    for kind in [Kind::OpenType, Kind::Apple] {
        let canonical = FontSet {
            fonts: vec![fixture(false, 14), fixture(false, 12)],
            ..Default::default()
        };
        let expected = embedded::write(&canonical, kind, &EmbeddedWriteOptions::default()).unwrap();
        assert_eq!(expected.1, expected_report());
        let sfnt = Sfnt::parse(&expected.0, &Default::default()).unwrap();
        sfnt.validate_checksums().unwrap();
        assert_eq!(
            cmap::decode(sfnt.table(*b"cmap").unwrap(), 5, &Default::default()).unwrap(),
            BTreeMap::from([(65, 1), (66, 2), (0x10000, 2)])
        );
        assert_eq!(
            PostTable::decode(sfnt.table(*b"post").unwrap(), 5, &Default::default())
                .unwrap()
                .names
                .unwrap(),
            [".notdef", "A", "B", "glyph3", "glyph4"]
        );
        let (data_tag, _) = kind.tags();
        let mut golden_data = vec![0, 2, 0, 0];
        for _ in 0..2 {
            golden_data.extend([
                1, 3, 0, 1, 3, 0, // explicit .notdef, retained at gid 0
                1, 3, 255, 1, 4, 160, // singleton A
                1, 3, 0, 1, 4, 64, // B and supplementary alias
                1, 1, 0, 1, 2, 128, // unencoded named glyph
                1, 2, 0, 1, 3, 128, // unencoded slot glyph
            ]);
        }
        assert_eq!(sfnt.table(data_tag).unwrap(), golden_data);
        for (first, second) in [(true, true), (false, true), (true, false)] {
            let set = FontSet {
                fonts: vec![fixture(first, 14), fixture(second, 12)],
                ..Default::default()
            };
            let actual = embedded::write(&set, kind, &EmbeddedWriteOptions::default()).unwrap();
            assert_eq!(actual, expected, "{kind:?}, repeat={first}/{second}");
            if kind == Kind::OpenType {
                let mut options = WriteOptions::new(ResourcePath::new("labels.otb").unwrap());
                let written = fontpixel::write(&set, "otb", &options).unwrap();
                assert_eq!(written.report, expected.1);
                assert_eq!(written.output.into_single().unwrap(), expected.0);
                options.strict = true;
                let fontpixel::Error::StrictLoss(report) =
                    fontpixel::write(&set, "otb", &options).unwrap_err()
                else {
                    panic!("expected strict loss")
                };
                assert_eq!(report, expected.1);
            }
        }
    }
}

#[test]
fn singleton_notdef_name_keeps_unicode_precedence_over_notdef_identity() {
    let mut canonical = Font::default();
    canonical.metrics.pixel_size = Some(14);
    canonical
        .add_glyph(glyph(Some(".notdef"), &[65], "#", 2))
        .unwrap();
    canonical
        .add_glyph(glyph(Some(".notdef"), &[], ".", 2))
        .unwrap();
    let mut repeated = canonical.clone();
    let id = repeated.glyphs().next().unwrap().0;
    repeated
        .edit_glyph(id, |g| {
            g.codepoints.push(Codepoint::Unicode(65));
            Ok(())
        })
        .unwrap();
    let expected = embedded::write(&canonical.into(), Kind::OpenType, &Default::default()).unwrap();
    let actual = embedded::write(&repeated.into(), Kind::OpenType, &Default::default()).unwrap();
    assert_eq!(actual, expected);
    assert!(actual.1.entries.is_empty());
    let sfnt = Sfnt::parse(&actual.0, &Default::default()).unwrap();
    assert_eq!(
        cmap::decode(sfnt.table(*b"cmap").unwrap(), 2, &Default::default()).unwrap(),
        BTreeMap::from([(65, 1)])
    );
    assert_eq!(
        sfnt.table(*b"EBDT").unwrap(),
        [0, 2, 0, 0, 1, 1, 0, 1, 2, 0, 1, 1, 0, 1, 2, 128]
    );
}

#[test]
fn duplicate_alias_errors_keep_identity_and_error_precedence() {
    for (first, second) in [
        (vec![65], vec![65, 65]),
        (vec![65, 65], vec![65]),
        (vec![65], vec![65]),
    ] {
        let mut f = Font::default();
        f.metrics.pixel_size = Some(14);
        f.add_glyph(glyph(Some("A"), &first, "#", 2)).unwrap();
        f.add_glyph(glyph(Some("invalid/name"), &second, "#", 2))
            .unwrap();
        let mut options = EmbeddedWriteOptions::default();
        options.limits.sfnt.max_cmap_mappings = 0;
        let error = embedded::write(&f.into(), Kind::OpenType, &options).unwrap_err();
        assert_eq!(
            error.to_string(),
            "embedded bitmap: ambiguous duplicate glyph labels within strike"
        );
    }
    for repeated in [false, true] {
        let mut f = Font::default();
        f.metrics.pixel_size = Some(14);
        f.add_glyph(glyph(
            Some("A"),
            if repeated { &[65, 65] } else { &[65] },
            "#",
            2,
        ))
        .unwrap();
        f.add_glyph(glyph(Some("B"), &[65, 66], "#", 2)).unwrap();
        let error = embedded::write(&f.into(), Kind::OpenType, &Default::default()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "embedded bitmap: conflicting Unicode aliases across glyphs/strikes"
        );
    }
}

fn word_boundary_fixture(repeated: bool) -> FontSet {
    let mut first = Font::default();
    first.metrics.pixel_size = Some(14);
    first.metrics.ascent = Some(11);
    first.metrics.descent = Some(3);
    for index in 0..129 {
        let cp = 0x2000 + index;
        let codes = if repeated { vec![cp, cp] } else { vec![cp] };
        first
            .add_glyph(glyph(Some(&format!("g{index}")), &codes, "#", 3))
            .unwrap();
    }
    let mut second = Font::default();
    second.metrics.pixel_size = Some(12);
    second.metrics.ascent = Some(9);
    second.metrics.descent = Some(3);
    // Encounter a high existing ID before low IDs, crossing several bit words.
    // Its different metrics must not replace the first strike's design metrics.
    for index in [128, 63, 64, 0] {
        let cp = 0x2000 + index;
        let codes = if repeated { vec![cp] } else { vec![cp, cp] };
        second
            .add_glyph(glyph(Some(&format!("g{index}")), &codes, "#", 6))
            .unwrap();
    }
    second
        .add_glyph(glyph(Some("newglyph"), &[0x3000], "#", 6))
        .unwrap();
    // The reserved .notdef is previously unseen in this strike, although the
    // first strike already emitted a synthesized image for ID zero.
    second
        .add_glyph(glyph(Some(".notdef"), &[], "#", 7))
        .unwrap();
    FontSet {
        fonts: vec![first, second],
        ..Default::default()
    }
}

#[test]
fn sparse_multistrike_ids_cross_bit_words_without_changing_first_metrics_or_caps() {
    use fontpixel::sfnt::metrics::{self, Axis, LongMetric};
    let canonical = word_boundary_fixture(false);
    let redundant = word_boundary_fixture(true);
    let mut options = EmbeddedWriteOptions::default();
    // 135 source glyphs, plus the first strike's synthesized .notdef image.
    options.limits.sfnt.resources.max_glyphs = 136;
    for kind in [Kind::OpenType, Kind::Apple] {
        let expected = embedded::write(&canonical, kind, &options).unwrap();
        assert_eq!(
            embedded::write(&redundant, kind, &options).unwrap(),
            expected
        );
        let sfnt = Sfnt::parse(&expected.0, &Default::default()).unwrap();
        sfnt.validate_checksums().unwrap();
        let mapping =
            cmap::decode(sfnt.table(*b"cmap").unwrap(), 131, &Default::default()).unwrap();
        for index in 0..129 {
            assert_eq!(mapping[&(0x2000 + index)], (index + 1) as u16);
        }
        assert_eq!(mapping[&0x3000], 130);
        let (_, horizontal) = metrics::decode(
            Axis::Horizontal,
            sfnt.table(*b"hhea").unwrap(),
            sfnt.table(*b"hmtx").unwrap(),
            131,
            &Default::default(),
        )
        .unwrap();
        assert_eq!(horizontal[0], LongMetric::default());
        assert!(horizontal[1..130].iter().all(|m| *m
            == LongMetric {
                advance: 300,
                bearing: 0
            }));
        assert_eq!(
            horizontal[130],
            LongMetric {
                advance: 700,
                bearing: 0
            }
        );
        assert_eq!(expected.1.entries, [ReportEntry {
            level: Level::Warning, impact: Impact::Loss, code: "embedded.design_rounding".into(),
            location: Location { glyph_index: Some(130), ..Default::default() },
            message: "Global design metrics rounded from strike pixels; embedded pixel metrics remain exact".into(),
        }]);
    }
    options.limits.sfnt.resources.max_glyphs = 135;
    assert!(matches!(
        embedded::write(&canonical, Kind::OpenType, &options),
        Err(fontpixel::Error::ResourceLimit {
            resource: "embedded bitmap glyphs",
            limit: 135
        })
    ));
    options.limits.sfnt.resources.max_glyphs = 134;
    assert_eq!(
        embedded::write(&canonical, Kind::OpenType, &options)
            .unwrap_err()
            .to_string(),
        "embedded bitmap: bitmap model/glyph budget exceeded"
    );
    let mut duplicate = word_boundary_fixture(false);
    duplicate.fonts[1]
        .add_glyph(glyph(Some("duplicate"), &[0x2080], "#", 6))
        .unwrap();
    assert_eq!(
        embedded::write(&duplicate, Kind::OpenType, &Default::default())
            .unwrap_err()
            .to_string(),
        "embedded bitmap: ambiguous duplicate glyph labels within strike"
    );
    let mut duplicate_notdef = word_boundary_fixture(false);
    duplicate_notdef.fonts[1]
        .add_glyph(glyph(Some(".notdef"), &[], "#", 7))
        .unwrap();
    assert_eq!(
        embedded::write(&duplicate_notdef, Kind::OpenType, &Default::default())
            .unwrap_err()
            .to_string(),
        "embedded bitmap: ambiguous duplicate glyph labels within strike"
    );
}

#[cfg(feature = "color")]
#[test]
fn color_shared_singleton_name_diagnostic_is_unchanged() {
    let mut baseline = FontSet {
        fonts: vec![fixture(false, 14), fixture(false, 12)],
        ..Default::default()
    };
    let id = baseline.fonts[1].glyphs().next().unwrap().0;
    baseline.fonts[1]
        .edit_glyph(id, |g| {
            g.name = Some("OtherA".into());
            Ok(())
        })
        .unwrap();
    let expected = embedded::write(&baseline, Kind::Color, &Default::default()).unwrap();
    let name_loss = expected
        .1
        .entries
        .iter()
        .find(|entry| {
            entry.code == "embedded.strike_glyph_name" && entry.location.glyph_index == Some(0)
        })
        .unwrap();
    assert_eq!(name_loss.location.font_index, Some(1));
    assert_eq!(
        name_loss.message,
        "Shared post name comes from the first strike containing this glyph"
    );
    for fi in 0..2 {
        let id = baseline.fonts[fi].glyphs().next().unwrap().0;
        baseline.fonts[fi]
            .edit_glyph(id, |g| {
                g.codepoints.push(Codepoint::Unicode(65));
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(
        embedded::write(&baseline, Kind::Color, &Default::default()).unwrap(),
        expected
    );
}
