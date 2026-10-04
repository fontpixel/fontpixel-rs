//! Reconstructed 23-case W6.5 regression suite. CURRENTLY UNTESTED.
use fontpixel::Error;
use fontpixel::bitmap::{Bitmap, Bounds, Gray8};
use fontpixel::charset::merge_categories::*;
use fontpixel::merge::*;
use fontpixel::model::{
    Codepoint, Font, Glyph, GlyphBitmap, GlyphMetrics, Property, PropertyValue, Spacing, Vector,
};
fn glyph(cp: u32, width: i64) -> Glyph {
    let mut g = Glyph::new("#.#\n.##".parse::<Bitmap>().unwrap());
    g.codepoints.push(Codepoint::Unicode(cp));
    g.metrics.advance = Some(Vector::new(width, 0));
    g
}
fn font(glyphs: impl IntoIterator<Item = Glyph>) -> Font {
    let mut f = Font::from_glyphs(glyphs).unwrap();
    f.names.family = Some("Component".into());
    f.names.license = Some("OFL-1.1".into());
    f.names.copyright = Some("Copyright original contributors".into());
    f
}
fn part<'a>(f: &'a Font, take: &str) -> MergePart<'a> {
    MergePart::new(f, Selection::parse(take).unwrap())
}
fn options() -> MergeOptions {
    MergeOptions {
        family: "New Mix".into(),
        spacing: MergeSpacing::Prop,
        ..MergeOptions::default()
    }
}
fn merged(parts: &[MergePart<'_>]) -> MergeResult {
    merge(parts, &options()).unwrap()
}
fn lookup(f: &Font, cp: u32) -> &Glyph {
    f.glyph_for_codepoint(Codepoint::Unicode(cp)).unwrap()
}

#[test]
fn unicode_tables_every_boundary_and_partition() {
    for ranges in [CJK_RANGES, GEOMETRIC_RANGES, UNICODE_BLOCKS] {
        for pair in ranges.windows(2) {
            assert!(pair[0].end < pair[1].start);
        }
        for r in ranges {
            assert!(r.start <= r.end);
        }
    }
    for r in CJK_RANGES {
        assert!(is_cjk(r.start));
        assert!(is_cjk(r.end));
        for cp in [r.start - 1, r.end + 1] {
            assert_eq!(
                is_cjk(cp),
                CJK_RANGES.iter().any(|r| r.start <= cp && cp <= r.end)
            );
        }
    }
    for cp in 0..=0x10ffff {
        assert_eq!(is_cjk(cp) || is_western(cp), is_scalar(cp));
        assert!(!(is_cjk(cp) && is_western(cp)));
    }
    for (start, end) in [(0x2500, 0x259f), (0x2800, 0x28ff), (0xe0a0, 0xe0d4)] {
        assert!(is_geometric(start));
        assert!(is_geometric(end));
        assert!(!is_geometric(start - 1));
        assert!(!is_geometric(end + 1));
        assert!(is_western(start));
    }
    assert!(is_cjk(0xff00));
    assert!(is_cjk(0xffef));
    assert!(is_cjk(0x1f200));
    assert!(is_cjk(0x1f2ff));
    assert!(is_han(0x323b0));
    assert!(is_han(0x3347f));
    assert!(!is_han(0x33480));
    assert_eq!(east_asian_width(0xff21), Some(EastAsianWidth::Fullwidth));
    assert_eq!(east_asian_width(0xff66), Some(EastAsianWidth::Halfwidth));
    assert_eq!(east_asian_width(0x4e00), Some(EastAsianWidth::Wide));
    assert_eq!(east_asian_width(65), Some(EastAsianWidth::Narrow));
    assert_eq!(east_asian_width(0xe0a0), Some(EastAsianWidth::Ambiguous));
    assert_eq!(east_asian_width(0xd800), None);
    assert!(is_western(0x1f600));
    assert!(is_western(0x17000));
    assert_eq!(unicode_block(65).unwrap().name, "Basic Latin");
    assert_eq!(unicode_block(0x2fe0), None);
}
#[test]
fn selection_composes_and_exclusions_always_win() {
    let s = Selection::parse("western,cjk,-U+0041-005A,-geometric,U+2500").unwrap();
    assert!(s.contains(0x4e00));
    assert!(s.contains(97));
    assert!(!s.contains(65));
    assert!(!s.contains(0x2500));
    let s = Selection::parse("-cjk").unwrap();
    assert!(s.contains(65));
    assert!(!s.contains(0x4e00));
    let s = Selection::parse("U+D7FF-E000").unwrap();
    assert_eq!(s.scalar_count(), 2);
    assert_eq!(s.ranges().len(), 2);
    assert_eq!(Selection::parse("all,-all").unwrap().scalar_count(), 0);
    for bad in [
        "", "all,", "foo", "U+110000", "U+D800", "U+42-41", "U+1-2-3", "U+GG", "--all",
    ] {
        assert!(Selection::parse(bad).is_err(), "{bad}");
    }
    assert!(Selection::parse(&"all,".repeat(1025)).is_err());
}
#[test]
fn first_selected_present_wins_then_fallback() {
    let a = font([glyph(65, 3), glyph(66, 4)]);
    let b = font([glyph(65, 8), glyph(67, 9)]);
    let c = font([glyph(65, 12), glyph(66, 13)]);
    let r = merged(&[part(&a, "U+42"), part(&b, "U+41-43"), part(&c, "U+41-43")]);
    assert_eq!(lookup(&r.font, 65).metrics.advance.unwrap().x, 8);
    assert_eq!(lookup(&r.font, 66).metrics.advance.unwrap().x, 4);
    assert_eq!(lookup(&r.font, 67).metrics.advance.unwrap().x, 9);
    let r = merged(&[part(&a, "U+41"), part(&b, "U+42")]);
    assert!(r.font.glyph_for_codepoint(Codepoint::Unicode(66)).is_none());
    let r = merge(
        &[part(&a, "U+41"), part(&b, "U+42")],
        &MergeOptions {
            fallback: true,
            ..options()
        },
    )
    .unwrap();
    assert_eq!(lookup(&r.font, 66).metrics.advance.unwrap().x, 4);
    assert_eq!(r.report.parts[0].fallback_codepoints, 1);
    assert!(r.font.glyph_for_codepoint(Codepoint::Unicode(67)).is_none());
}
#[test]
fn full_selection_missing_is_compressed_and_grouped() {
    let f = font([glyph(65, 3), glyph(67, 3)]);
    let r = merged(&[part(&f, "U+0040-0044")]);
    let p = &r.report.parts[0];
    assert_eq!(p.selected_missing, 3);
    assert_eq!(
        p.missing_ranges,
        vec![
            CodepointRange { start: 64, end: 64 },
            CodepointRange { start: 66, end: 66 },
            CodepointRange { start: 68, end: 68 }
        ]
    );
    assert_eq!(p.missing_blocks[0].block, "Basic Latin");
    assert_eq!(p.missing_blocks[0].count, 3);
    let r = merged(&[part(&f, "all")]);
    assert_eq!(r.report.parts[0].selected_missing, 0x110000 - 0x800 - 2);
    assert!(r.report.parts[0].missing_ranges.len() < 10);
    assert!(r.report.parts[0].missing_blocks.len() < 400);
}
#[test]
fn baseline_offsets_defaults_and_overrides() {
    let mut a = font([glyph(65, 3)]);
    a.metrics.ascent = Some(8);
    a.metrics.descent = Some(2);
    let mut b = font([glyph(66, 4)]);
    b.metrics.ascent = Some(6);
    b.metrics.descent = Some(4);
    let mut p = part(&a, "all");
    p.dy = 3;
    let mut q = part(&b, "all");
    q.dy = -2;
    let r = merged(&[p.clone(), q.clone()]);
    assert_eq!(r.font.metrics.ascent, Some(11));
    assert_eq!(r.font.metrics.descent, Some(6));
    assert_eq!(lookup(&r.font, 65).metrics.bounds.y, 3);
    assert_eq!(lookup(&r.font, 66).metrics.bounds.y, -2);
    assert_eq!(lookup(&r.font, 65).bitmap, lookup(&a, 65).bitmap);
    let r = merge(
        &[p, q],
        &MergeOptions {
            ascent: Some(1),
            descent: Some(0),
            ..options()
        },
    )
    .unwrap();
    assert_eq!(r.font.metrics.ascent, Some(1));
    assert_eq!(lookup(&r.font, 65).metrics.bounds.y, 3);
    assert!(r.report.metrics.ascent_overridden);
    let mut g = glyph(67, 2);
    g.metrics.advance = None;
    g.metrics.vertical_advance = None;
    let mut f = font([g]);
    f.metrics.glyph_defaults = GlyphMetrics {
        advance: Some(Vector::new(7, 0)),
        vertical_advance: Some(Vector::new(0, -11)),
        ..GlyphMetrics::default()
    };
    let r = merged(&[part(&f, "all")]);
    assert_eq!(lookup(&r.font, 67).metrics.advance, Some(Vector::new(7, 0)));
    assert_eq!(
        lookup(&r.font, 67).metrics.vertical_advance,
        Some(Vector::new(0, -11))
    );
    assert_eq!(r.font.metrics.ascent, Some(2));
    assert!(
        merge(
            &[part(&a, "all")],
            &MergeOptions {
                ascent: Some(-1),
                ..options()
            }
        )
        .is_err()
    );
    let mut p = part(&a, "all");
    p.dy = i64::MAX;
    assert!(merge(&[p], &options()).is_err());
}
#[test]
fn prop_preserves_all_glyph_metrics_and_gray8_pixels() {
    let mut g =
        Glyph::new(Bitmap::<Gray8>::from_raw(3, 1, &[1, 128, 255], Default::default()).unwrap());
    g.codepoints.push(Codepoint::Unicode(65));
    g.metrics.bounds = Bounds::new(3, 1, -4, -2);
    g.metrics.advance = Some(Vector::new(9, 1));
    g.metrics.scalable_advance = Some(Vector::new(601, 7));
    let f = font([g.clone()]);
    let before = f.clone();
    let r = merged(&[part(&f, "all")]);
    assert_eq!(lookup(&r.font, 65), &g);
    assert_eq!(f, before);
    assert!(matches!(lookup(&r.font, 65).bitmap, GlyphBitmap::Gray8(_)));
}
#[test]
fn mono_equal_narrow_wide_and_allowed_mismatch() {
    let east = font([glyph(0x4e00, 12), glyph(0x4e01, 12), glyph(0x4e02, 14)]);
    let west = font([glyph(65, 6), glyph(66, 3)]);
    let o = MergeOptions {
        spacing: MergeSpacing::Mono,
        ..options()
    };
    let r = merge(&[part(&east, "cjk"), part(&west, "western")], &o).unwrap();
    assert_eq!(r.report.full_width, Some(12));
    assert_eq!(lookup(&r.font, 65), lookup(&west, 65));
    let b = lookup(&r.font, 66);
    assert_eq!(b.metrics.advance, Some(Vector::new(6, 0)));
    assert_eq!(b.metrics.bounds.x, 1);
    assert_eq!(b.bitmap, lookup(&west, 66).bitmap);
    assert_eq!(r.report.adjustments[0].right_padding, 2);
    let wide = font([glyph(65, 7)]);
    assert!(merge(&[part(&east, "cjk"), part(&wide, "western")], &o).is_err());
    let r = merge(
        &[part(&east, "cjk"), part(&wide, "western")],
        &MergeOptions {
            allow_misaligned: true,
            ..o
        },
    )
    .unwrap();
    assert_eq!(lookup(&r.font, 65), lookup(&wide, 65));
    assert_eq!(r.report.adjustments[0].kind, AdjustmentKind::Misaligned);
}
#[test]
fn mono_odd_width_absent_invalid_and_tied_modes() {
    let odd = font([glyph(0x4e00, 11)]);
    let west = font([glyph(65, 5)]);
    let o = MergeOptions {
        spacing: MergeSpacing::Mono,
        ..options()
    };
    assert!(merge(&[part(&odd, "cjk"), part(&west, "western")], &o).is_err());
    let r = merge(
        &[part(&odd, "cjk"), part(&west, "western")],
        &MergeOptions {
            allow_misaligned: true,
            ..o.clone()
        },
    )
    .unwrap();
    assert_eq!(r.report.adjustments[0].target_advance, None);
    assert!(merge(&[part(&west, "all")], &o).is_err());
    let tied = font([glyph(0x4e00, 10), glyph(0x4e01, 12)]);
    assert!(merge(&[part(&tied, "cjk")], &o).is_err());
    for advance in [
        None,
        Some(Vector::new(0, 0)),
        Some(Vector::new(-1, 0)),
        Some(Vector::new(10, 1)),
    ] {
        let mut g = glyph(0x4e00, 10);
        g.metrics.advance = advance;
        let f = font([g]);
        assert!(merge(&[part(&f, "cjk")], &o).is_err());
    }
    let east = font([glyph(0x4e00, 12)]);
    for advance in [None, Some(Vector::new(-1, 0)), Some(Vector::new(5, 1))] {
        let mut g = glyph(65, 5);
        g.metrics.advance = advance;
        let f = font([g]);
        assert!(merge(&[part(&east, "cjk"), part(&f, "western")], &o).is_err());
        assert!(
            merge(
                &[part(&east, "cjk"), part(&f, "western")],
                &MergeOptions {
                    allow_misaligned: true,
                    ..o.clone()
                }
            )
            .is_ok()
        );
    }
}
#[test]
fn auto_uses_all_explicit_spacing_classifications() {
    let mut east = font([glyph(0x4e00, 12)]);
    let mut west = font([glyph(65, 3)]);
    let o = MergeOptions {
        spacing: MergeSpacing::Auto,
        ..options()
    };
    assert_eq!(
        merge(&[part(&east, "cjk"), part(&west, "western")], &o)
            .unwrap()
            .report
            .effective_spacing,
        MergeSpacing::Prop
    );
    east.metrics.spacing = Some(Spacing::CharacterCell);
    west.metrics.spacing = Some(Spacing::Monospaced);
    assert_eq!(
        merge(&[part(&east, "cjk"), part(&west, "western")], &o)
            .unwrap()
            .report
            .effective_spacing,
        MergeSpacing::Mono
    );
    west.metrics.spacing = Some(Spacing::Proportional);
    assert_eq!(
        merge(&[part(&east, "cjk"), part(&west, "western")], &o)
            .unwrap()
            .report
            .effective_spacing,
        MergeSpacing::Prop
    );
}
#[test]
fn selected_aliases_and_sequences_preserve_shared_glyphs() {
    let mut g = glyph(65, 3);
    g.codepoints
        .extend([Codepoint::Unicode(66), Codepoint::Unicode(67)]);
    g.unicode_sequences = vec![vec![65, 66], vec![65, 67]];
    let f = font([g]);
    let r = merged(&[part(&f, "U+41-42")]);
    assert_eq!(r.font.len(), 1);
    assert_eq!(
        lookup(&r.font, 65).codepoints,
        vec![Codepoint::Unicode(65), Codepoint::Unicode(66)]
    );
    assert_eq!(lookup(&r.font, 65).unicode_sequences, vec![vec![65, 66]]);
    let mut g = glyph(65, 3);
    g.codepoints.push(Codepoint::Unicode(0x4e00));
    let f = font([g]);
    let r = merge(
        &[part(&f, "all")],
        &MergeOptions {
            spacing: MergeSpacing::Mono,
            allow_misaligned: true,
            ..options()
        },
    )
    .unwrap();
    assert_eq!(r.font.len(), 2);
    assert!(
        r.report
            .diagnostics
            .entries
            .iter()
            .any(|e| e.code == "merge.mapping.alias_split")
    );
}
#[test]
fn sequence_precedence_and_fallback_are_independent_of_scalar_aliases() {
    let mut a = glyph(65, 3);
    a.unicode_sequences = vec![vec![65, 66]];
    let mut b = glyph(65, 5);
    b.unicode_sequences = a.unicode_sequences.clone();
    let fa = font([a]);
    let fb = font([b]);
    let r = merged(&[part(&fa, "U+41"), part(&fb, "U+41-42")]);
    assert_eq!(
        r.font
            .glyph_for_sequence(&[65, 66])
            .unwrap()
            .metrics
            .advance,
        Some(Vector::new(5, 0))
    );
    let empty = font([]);
    let r = merge(
        &[part(&fa, "U+41"), part(&empty, "U+41-42")],
        &MergeOptions {
            fallback: true,
            ..options()
        },
    )
    .unwrap();
    assert!(r.font.glyph_for_sequence(&[65, 66]).is_some());
    assert_eq!(r.report.parts[0].fallback_sequences, 1);
}
#[test]
fn effective_last_duplicate_and_unsupported_mappings_are_explicit() {
    let mut g = glyph(65, 9);
    g.codepoints.extend([
        Codepoint::Legacy(65),
        Codepoint::Unicode(0xd800),
        Codepoint::Unicode(0x110000),
    ]);
    let f = font([glyph(65, 3), g, Glyph::default()]);
    let r = merged(&[part(&f, "all")]);
    assert_eq!(lookup(&r.font, 65).metrics.advance.unwrap().x, 9);
    assert_eq!(r.report.parts[0].omitted_legacy_mappings, 1);
    assert_eq!(r.report.parts[0].omitted_invalid_mappings, 2);
    assert_eq!(r.report.parts[0].omitted_unencoded_glyphs, 1);
    assert!(r.report.diagnostics.has_loss());
    assert!(matches!(
        merge(
            &[part(&f, "all")],
            &MergeOptions {
                strict: true,
                ..options()
            }
        ),
        Err(Error::StrictLoss(_))
    ));
}
#[test]
fn required_family_and_reserved_names_are_enforced() {
    let mut f = font([glyph(65, 3)]);
    f.names.copyright = Some(
        "Copyright Example; SIL Open Font License 1.1; Reserved Font Names Source, Plangothic"
            .into(),
    );
    f.names.license = None;
    for family in [
        "",
        " ",
        "bad\nname",
        " Source Mix ",
        "Source Mix",
        "myPLANGOTHICmix",
    ] {
        assert!(
            merge(
                &[part(&f, "all")],
                &MergeOptions {
                    family: family.into(),
                    ..options()
                }
            )
            .is_err(),
            "{family}"
        );
    }
    let r = merge(
        &[part(&f, "all")],
        &MergeOptions {
            strict: true,
            ..options()
        },
    )
    .unwrap();
    assert_eq!(
        r.report.licenses[0].modification,
        ModificationPermission::Allowed
    );
    assert_eq!(
        r.report.licenses[0].reserved_names,
        vec!["Source", "Plangothic"]
    );
    assert!(
        r.font
            .names
            .copyright
            .unwrap()
            .starts_with("Merged by fontpixel; parts: Component\n")
    );
    assert!(r.font.names.license.unwrap().contains("OFL-1.1"));
}
#[test]
fn licenses_preserve_order_unknown_prohibited_and_supplements() {
    let mut a = font([glyph(65, 3)]);
    a.names.license = None;
    a.names.copyright = None;
    a.names.family = Some("First".into());
    let mut b = font([glyph(66, 4)]);
    b.names.family = Some("Second".into());
    let mut pb = part(&b, "all");
    pb.license.modification = ModificationPermission::Prohibited;
    let r = merged(&[part(&a, "all"), pb.clone()]);
    assert_eq!(
        r.report.licenses[0].modification,
        ModificationPermission::Unknown
    );
    assert_eq!(
        r.report.licenses[1].modification,
        ModificationPermission::Prohibited
    );
    assert_eq!(
        r.report
            .diagnostics
            .entries
            .iter()
            .filter(|e| e.code.starts_with("merge.license."))
            .count(),
        2
    );
    assert!(matches!(
        merge(
            &[part(&a, "all"), pb],
            &MergeOptions {
                strict: true,
                ..options()
            }
        ),
        Err(Error::StrictLoss(_))
    ));
    let mut pb = part(&b, "all");
    pb.license.copyright = Some("Additional notice".into());
    let r = merged(&[pb]);
    assert!(
        r.font
            .names
            .copyright
            .unwrap()
            .contains("Copyright original contributors\nAdditional notice")
    );
    let mut tricky = font([glyph(65, 3)]);
    tricky.names.license = Some("This is NOT an MIT license grant".into());
    assert_eq!(
        merged(&[part(&tricky, "all")]).report.licenses[0].modification,
        ModificationPermission::Unknown
    );
}
#[test]
fn reports_are_valid_complete_json_and_text() {
    let mut f = font([glyph(65, 3)]);
    f.names.family = Some("quote\" slash\\ tab\t".into());
    let r = merged(&[part(&f, "U+0040-0042")]);
    let json: serde_json::Value = serde_json::from_str(&r.report.to_json()).unwrap();
    assert_eq!(json["family"], "New Mix");
    assert_eq!(json["parts"][0]["selected_missing"], 2);
    assert!(r.report.to_string().contains("Missing Basic Latin: 2"));
    assert!(
        r.report
            .to_string()
            .contains("Copyright original contributors")
    );
    #[cfg(feature = "serde")]
    assert_eq!(json, serde_json::to_value(&r.report).unwrap());
}
#[test]
fn bounds_and_budget_fail_before_materializing_unbounded_outputs() {
    let f = font([glyph(65, 3)]);
    let mut o = options();
    o.limits.max_total_bytes = 10;
    assert!(matches!(
        merge(&[part(&f, "all")], &o),
        Err(Error::ResourceLimit { .. })
    ));
    let mut o = options();
    o.limits.max_glyphs = 0;
    assert!(merge(&[part(&f, "all")], &o).is_err());
    let mut o = options();
    o.limits.max_resources = 0;
    assert!(merge(&[part(&f, "all")], &o).is_err());
    assert!(merge(&[], &options()).is_err());
    let mut g = glyph(65, 3);
    g.metrics.bounds.y = i64::MAX;
    assert!(Font::from_glyphs([g]).is_err());
    let mut f = font([glyph(65, 3)]);
    let mut value = PropertyValue::Flag;
    for _ in 0..130 {
        value = PropertyValue::List(vec![value]);
    }
    f.properties.push(Property::new("nested", value));
    assert!(matches!(
        merge(&[part(&f, "all")], &options()),
        Err(Error::ResourceLimit { .. })
    ));
}
#[test]
fn repeated_merges_are_deterministic() {
    let east = font([glyph(0x4e00, 12)]);
    let west = font([glyph(65, 4)]);
    let p = [part(&east, "cjk"), part(&west, "western")];
    let a = merged(&p);
    let b = merged(&p);
    assert_eq!(a.font, b.font);
    assert_eq!(a.report, b.report);
    assert_eq!(a.report.to_json(), b.report.to_json());
}
#[test]
fn geometric_source_and_width_policies_are_independent() {
    let east = font([glyph(0x4e00, 12), glyph(0x2500, 12)]);
    let west = font([glyph(0x2500, 4), glyph(65, 6)]);
    let o = MergeOptions {
        spacing: MergeSpacing::Mono,
        ..options()
    };
    let r = merge(&[part(&east, "cjk"), part(&west, "western")], &o).unwrap();
    assert_eq!(lookup(&r.font, 0x2500).metrics.advance.unwrap().x, 6);
    assert_eq!(r.report.geometric_width, GeometricWidth::Half);
    assert!(
        merge(
            &[
                part(&east, "cjk,geometric"),
                part(&west, "western,-geometric")
            ],
            &o
        )
        .is_err()
    );
    let r = merge(
        &[
            part(&east, "cjk,geometric"),
            part(&west, "western,-geometric"),
        ],
        &MergeOptions {
            geometric_width: GeometricWidth::Full,
            ..o
        },
    )
    .unwrap();
    assert_eq!(lookup(&r.font, 0x2500), lookup(&east, 0x2500));
    assert_eq!(r.report.parts[0].taken_codepoints, 2);
    assert_eq!(r.report.parts[1].taken_codepoints, 1);
}
#[test]
fn narrower_padding_updates_exact_scalable_bearings_and_image_offset() {
    let east = font([glyph(0x4e00, 12)]);
    let mut g = glyph(65, 3);
    g.metrics.scalable_advance = Some(Vector::new(300, 2));
    g.metrics.left_bearing = Some(-2);
    g.metrics.right_bearing = Some(1);
    g.metrics.image_offset = Some(Vector::new(-2, 7));
    g.metrics.abc = Some(fontpixel::model::AbcWidths { a: -2, b: 4, c: 1 });
    let west = font([g]);
    let o = MergeOptions {
        spacing: MergeSpacing::Mono,
        ..options()
    };
    let mut p = part(&west, "western");
    p.dy = 2;
    let r = merge(&[part(&east, "cjk"), p], &o).unwrap();
    let g = lookup(&r.font, 65);
    assert_eq!(g.metrics.scalable_advance, Some(Vector::new(600, 2)));
    assert_eq!(g.metrics.left_bearing, Some(-1));
    assert_eq!(g.metrics.right_bearing, Some(3));
    assert_eq!(g.metrics.image_offset, Some(Vector::new(-1, 5)));
    assert_eq!(g.metrics.abc.unwrap().a, -1);
    assert_eq!(g.metrics.abc.unwrap().c, 3);
    let mut g = glyph(65, 5);
    g.metrics.scalable_advance = Some(Vector::new(333, 0));
    let west = font([g]);
    let r = merge(&[part(&east, "cjk"), part(&west, "western")], &o).unwrap();
    assert_eq!(lookup(&r.font, 65).metrics.advance.unwrap().x, 6);
    assert_eq!(lookup(&r.font, 65).metrics.scalable_advance.unwrap().x, 400);
    assert!(
        r.report
            .diagnostics
            .entries
            .iter()
            .any(|e| e.code == "merge.metrics.scalable_rounding")
    );
    assert!(matches!(
        merge(
            &[part(&east, "cjk"), part(&west, "western")],
            &MergeOptions { strict: true, ..o }
        ),
        Err(Error::StrictLoss(_))
    ));
}
#[test]
fn first_global_metrics_default_references_and_differences_are_explicit() {
    let mut a = font([glyph(65, 3), glyph(66, 4)]);
    a.metrics.underline_position = Some(-2);
    a.metrics.underline_thickness = Some(1);
    a.metrics.line_gap = Some(3);
    a.metrics.default_character = Some(Codepoint::Unicode(66));
    a.metrics.default_glyph = a.glyph_id(Codepoint::Unicode(66));
    a.metrics.metrics_set = Some(2);
    let r = merged(&[part(&a, "U+42")]);
    assert_eq!(r.font.metrics.underline_position, Some(-2));
    assert_eq!(r.font.metrics.underline_thickness, Some(1));
    assert_eq!(r.font.metrics.metrics_set, Some(2));
    assert_eq!(r.font.metrics.line_gap, Some(3));
    assert_eq!(r.font.metrics.default_glyph.unwrap().index(), 0);
    assert_eq!(
        r.font.metrics.default_character,
        Some(Codepoint::Unicode(66))
    );
    assert!(
        r.report
            .diagnostics
            .entries
            .iter()
            .any(|e| e.code == "merge.metrics.defaults")
    );
    let r = merged(&[part(&a, "U+41")]);
    assert_eq!(r.font.metrics.default_glyph, None);
    assert_eq!(r.font.metrics.default_character, None);
}
#[test]
fn explicit_numeric_modification_and_no_derivative_identifiers_are_reported() {
    let mut f = font([glyph(65, 3)]);
    f.properties.set("MODIFICATION_ALLOWED", 0i64);
    assert_eq!(
        merged(&[part(&f, "all")]).report.licenses[0].modification,
        ModificationPermission::Prohibited
    );
    f.properties.set("MODIFICATION_ALLOWED", "FALSE");
    assert_eq!(
        merged(&[part(&f, "all")]).report.licenses[0].modification,
        ModificationPermission::Prohibited
    );
    f.properties.set("MODIFICATION_ALLOWED", "true");
    f.properties.set("ALLOW_MODIFICATION", "no");
    assert_eq!(
        merged(&[part(&f, "all")]).report.licenses[0].modification,
        ModificationPermission::Prohibited
    );
    f.properties.remove("ALLOW_MODIFICATION");
    f.properties.remove("MODIFICATION_ALLOWED");
    f.names.license = Some("CC-BY-ND-4.0".into());
    assert_eq!(
        merged(&[part(&f, "all")]).report.licenses[0].modification,
        ModificationPermission::Prohibited
    );
    f.names.license = Some("MIT".into());
    f.names.copyright = Some("Copyright Example\nCC-BY-ND-4.0".into());
    assert_eq!(
        merged(&[part(&f, "all")]).report.licenses[0].modification,
        ModificationPermission::Prohibited
    );
    let mut p = part(&f, "all");
    p.license.reserved_names = vec!["  New  ".into()];
    assert!(merge(&[p], &options()).is_err());
}
#[test]
fn scalable_rounding_and_zero_advance_combining_marks_still_center() {
    let east = font([glyph(0x4e00, 10)]);
    let mut g = glyph(65, 4);
    g.metrics.scalable_advance = Some(Vector::new(333, 0));
    let mut mark = glyph(0x301, 0);
    mark.metrics.scalable_advance = Some(Vector::new(0, 0));
    let west = font([g, mark]);
    let o = MergeOptions {
        spacing: MergeSpacing::Mono,
        ..options()
    };
    let r = merge(&[part(&east, "cjk"), part(&west, "western")], &o).unwrap();
    assert_eq!(lookup(&r.font, 65).metrics.advance.unwrap().x, 5);
    assert_eq!(lookup(&r.font, 65).metrics.scalable_advance.unwrap().x, 416);
    assert_eq!(lookup(&r.font, 0x301).metrics.advance.unwrap().x, 5);
    assert_eq!(
        lookup(&r.font, 0x301).metrics.scalable_advance.unwrap().x,
        0
    );
    assert_eq!(lookup(&r.font, 0x301).metrics.bounds.x, 2);
    assert!(
        r.report
            .diagnostics
            .entries
            .iter()
            .any(|e| e.code == "merge.metrics.scalable_zero_advance")
    );
    assert!(r.report.diagnostics.has_loss());
}
#[test]
fn mixed_pixel_sizes_keep_first_nominal_grid_and_all_actual_pixels() {
    let mut east = font([glyph(0x4e00, 12)]);
    east.metrics.pixel_size = Some(12);
    east.metrics.ppem_x = Some(12);
    east.metrics.ppem_y = Some(12);
    east.metrics.ascent = Some(10);
    east.metrics.descent = Some(2);
    let mut west = font([glyph(65, 6)]);
    west.metrics.pixel_size = Some(13);
    west.metrics.ppem_x = Some(13);
    west.metrics.ppem_y = Some(13);
    west.metrics.ascent = Some(11);
    west.metrics.descent = Some(2);
    let mut p = part(&west, "western");
    p.dy = -1;
    let r = merge(
        &[part(&east, "cjk"), p],
        &MergeOptions {
            ascent: Some(12),
            descent: Some(4),
            ..options()
        },
    )
    .unwrap();
    assert_eq!(r.font.metrics.pixel_size, Some(12));
    assert_eq!(r.font.metrics.ppem_y, Some(12));
    assert_eq!(r.font.metrics.ascent, Some(12));
    assert_eq!(r.font.metrics.descent, Some(4));
    assert_eq!(lookup(&r.font, 65).bitmap, lookup(&west, 65).bitmap);
    assert_eq!(lookup(&r.font, 65).metrics.bounds.y, -1);
    assert!(
        r.report
            .diagnostics
            .entries
            .iter()
            .any(|e| e.code == "merge.metrics.pixel_size" && e.location.font_index == Some(1))
    );
    assert!(
        r.report
            .diagnostics
            .entries
            .iter()
            .any(|e| e.code == "merge.metrics.ppem_y" && e.location.font_index == Some(1))
    );
}
