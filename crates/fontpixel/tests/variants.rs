//! Regional planning, layout integrity and subset regression tests.
#![cfg(feature = "sfnt")]
use fontpixel::{
    Bitmap, Font, Glyph, Variants,
    model::{Codepoint, Vector},
    sfnt::{
        self,
        gsub::{Feature, Gsub, LanguageSystem, LayoutLimits, Script},
    },
    variants::{self, Options, ScriptVariant},
};
use std::collections::{BTreeMap, BTreeSet};
fn face(pattern: &str, advance: i64) -> Font {
    let mut f = Font::default();
    f.metrics.pixel_size = Some(14);
    f.metrics.ascent = Some(14);
    f.metrics.descent = Some(4);
    let mut g = Glyph::new(pattern.parse::<Bitmap>().unwrap());
    g.codepoints = vec![Codepoint::Unicode(65)];
    g.metrics.advance = Some(Vector::new(advance, 0));
    f.add_glyph(g).unwrap();
    f
}
fn sample() -> Variants {
    let mut v = Variants::new(face("#.\n##", 3));
    v.add_language("zh-Hant", face("##\n.#", 3)).unwrap();
    v.add_language("zh-HK", face("##\n.#", 3)).unwrap();
    v.add_language("ja", face("#.\n##", 4)).unwrap();
    v.scripts.push(ScriptVariant {
        scripts: vec![*b"latn", *b"grek", *b"cyrl"],
        font: face("#", 1),
    });
    v
}
#[test]
fn language_registry_and_regional_tags() {
    for (s, t) in [
        ("zh-Hant", *b"ZHT "),
        ("zh-HK", *b"ZHH "),
        ("zh-TW", *b"ZHT "),
        ("ja-JP", *b"JAN "),
        ("ko", *b"KOR "),
        ("en-GB", *b"ENG "),
        ("de", *b"DEU "),
        ("ZHT", *b"ZHT "),
    ] {
        assert_eq!(variants::language_tag(s).unwrap(), t);
    }
    for s in ["", "x--bad", "🤔", "not_a_tag"] {
        assert!(variants::language_tag(s).is_err());
    }
}
#[test]
fn dedup_uses_bitmap_and_all_metrics() {
    let p = sample().plan(&LayoutLimits::default()).unwrap();
    assert_eq!(p.glyphs.len(), 4);
    let lookup = &p.gsub.lookups;
    assert_eq!(lookup[0], lookup[1]);
    assert_ne!(lookup[0], lookup[2]);
    let mut v = sample();
    let id = v.languages[1]
        .font
        .glyph_id(Codepoint::Unicode(65))
        .unwrap();
    v.languages[1]
        .font
        .edit_glyph(id, |g| {
            g.metrics.vertical_origin = Some(Vector::new(1, 2));
            Ok(())
        })
        .unwrap();
    assert_eq!(v.plan(&Default::default()).unwrap().glyphs.len(), 5);
}
#[test]
fn alternative_only_characters_become_defaults() {
    let mut v = sample();
    let mut g = Glyph::new("#".parse::<Bitmap>().unwrap());
    g.codepoints = vec![Codepoint::Unicode(0x20000)];
    v.languages[0].font.add_glyph(g).unwrap();
    let p = v.plan(&Default::default()).unwrap();
    assert!(p.cmap.contains_key(&0x20000));
}
#[test]
fn script_defaults_and_explicit_empty_routes() {
    let p = sample().plan(&Default::default()).unwrap();
    let latn = &p.gsub.scripts[b"latn"];
    assert_eq!(latn.default.as_ref().unwrap().features.len(), 1);
    assert!(latn.languages.is_empty());
    assert!(
        p.gsub.scripts[b"hani"]
            .default
            .as_ref()
            .unwrap()
            .features
            .is_empty()
    );
}
#[test]
fn large_extension_offsets_roundtrip() {
    let g = Gsub {
        scripts: BTreeMap::from([(
            *b"DFLT",
            Script {
                default: Some(LanguageSystem {
                    required: None,
                    features: vec![0],
                }),
                ..Default::default()
            },
        )]),
        features: vec![Feature {
            tag: *b"locl",
            lookups: vec![0, 1],
        }],
        lookups: vec![
            (0..40000).map(|i| (i, i + 1)).collect(),
            (0..40000).map(|i| (i, i + 2)).collect(),
        ],
    };
    let b = g.encode(60000, &Default::default()).unwrap();
    assert!(b.len() > 65536);
    assert_eq!(Gsub::decode(&b, 60000, &Default::default()).unwrap(), g);
    let mut keep = BTreeSet::from([39999]);
    g.closure(&mut keep);
    assert!(keep.contains(&40001));
}
#[test]
fn malformed_layout_never_panics() {
    let p = sample().plan(&Default::default()).unwrap();
    let b = p.gsub.encode(5, &Default::default()).unwrap();
    for end in 0..b.len() {
        let _ = Gsub::decode(&b[..end], 5, &Default::default());
    }
    for i in 0..b.len() {
        for byte in [0, 255] {
            let mut bad = b.clone();
            bad[i] = byte;
            let _ = Gsub::decode(&bad, 5, &Default::default());
        }
    }
    let l = LayoutLimits {
        max_records: 2,
        ..Default::default()
    };
    assert!(Gsub::decode(&b, 5, &l).is_err());
}
#[test]
fn variants_and_subsets_keep_names_cmap_dependencies() {
    let v = sample();
    for shape in [
        fontpixel::outline::Shape::Square,
        fontpixel::outline::Shape::Dot,
    ] {
        let o = Options {
            shape,
            dot_size: if shape == fontpixel::outline::Shape::Dot {
                0.9
            } else {
                1.
            },
            embed_bitmaps: true,
            ..Default::default()
        };
        let e = variants::encode(&v, &o).unwrap();
        let f = sfnt::Sfnt::parse(&e.bytes, &Default::default()).unwrap();
        f.validate_checksums().unwrap();
        let s = sfnt::subset::subset(&e.bytes, &sfnt::subset::Options::text("A")).unwrap();
        let sub = sfnt::Sfnt::parse(&s.bytes, &Default::default()).unwrap();
        sub.validate_checksums().unwrap();
        assert_eq!(f.table(*b"name"), sub.table(*b"name"));
        assert!(sub.table(*b"GSUB").is_some());
        assert!(sub.table(*b"EBDT").is_some());
        assert_eq!(
            s.glyph_mapping.len(),
            5 + usize::from(shape == fontpixel::outline::Shape::Dot)
        );
        let empty = sfnt::subset::subset(&e.bytes, &sfnt::subset::Options::text("")).unwrap();
        assert_eq!(empty.glyph_mapping.len(), 1);
        assert!(
            sfnt::Sfnt::parse(&empty.bytes, &Default::default())
                .unwrap()
                .table(*b"GSUB")
                .is_none()
        );
    }
}
#[cfg(feature = "cff")]
#[test]
fn cff_subset_preserves_raw_programs() {
    let v = sample();
    let e = variants::encode(
        &v,
        &Options {
            flavor: variants::OutlineFlavor::Cff,
            shape: fontpixel::outline::Shape::Dot,
            dot_size: 0.9,
            ..Default::default()
        },
    )
    .unwrap();
    let s = sfnt::subset::subset(&e.bytes, &sfnt::subset::Options::text("A")).unwrap();
    let l = sfnt::cff::Limits::default();
    let f = sfnt::Sfnt::parse(&e.bytes, &l.sfnt).unwrap();
    let a = sfnt::cff::Cff::from_sfnt(&f, &l).unwrap();
    let f = sfnt::Sfnt::parse(&s.bytes, &l.sfnt).unwrap();
    let b = sfnt::cff::Cff::from_sfnt(&f, &l).unwrap();
    for (old, new) in s.glyph_mapping {
        assert_eq!(
            a.fontations().charstrings().get(usize::from(old)),
            b.fontations().charstrings().get(usize::from(new))
        );
        assert_eq!(a.glyph(old, &l).unwrap(), b.glyph(new, &l).unwrap());
    }
}
#[test]
fn unsupported_tables_fail_or_report_explicit_loss() {
    let e = variants::encode(&sample(), &Default::default()).unwrap();
    let f = sfnt::Sfnt::parse(&e.bytes, &Default::default()).unwrap();
    let mut b = sfnt::SfntBuilder::new(f.flavor());
    for t in f.tables() {
        b.insert(t.tag, t.data.to_vec()).unwrap();
    }
    b.insert(*b"TEST", vec![1, 2]).unwrap();
    let b = b.build(&Default::default()).unwrap();
    let mut o = sfnt::subset::Options::text("A");
    assert!(sfnt::subset::subset(&b, &o).is_err());
    o.drop_tables.insert(*b"TEST");
    assert!(sfnt::subset::subset(&b, &o).unwrap().report.has_loss());
}

#[cfg(feature = "cff")]
#[test]
fn subset_cff_cid_hints_subroutines_and_font_matrices() {
    let limits = sfnt::cff::Limits::default();
    for data in [
        include_bytes!("../../../tests/data/cff/external-cid.cff").as_slice(),
        include_bytes!("../../../tests/data/cff/external-cid-format0.cff").as_slice(),
        include_bytes!("../../../tests/data/cff/external-operators.cff").as_slice(),
        include_bytes!("../../../tests/data/cff/external-fontmatrix.cff").as_slice(),
    ] {
        let original = sfnt::cff::Cff::parse(data, 1400, &limits).unwrap();
        let order = vec![0, (original.len() - 1) as u16];
        let bytes = original.subset(&order, &limits).unwrap();
        let sub = sfnt::cff::Cff::parse(&bytes, 1400, &limits).unwrap();
        assert_eq!(sub.len(), 2);
        for (new, &old) in order.iter().enumerate() {
            assert_eq!(original.identifier(old), sub.identifier(new as u16));
            assert_eq!(
                original.glyph(old, &limits).unwrap(),
                sub.glyph(new as u16, &limits).unwrap()
            );
            assert_eq!(
                original.fontations().charstrings().get(usize::from(old)),
                sub.fontations().charstrings().get(new)
            );
        }
    }
}
#[test]
fn source_limits_and_metric_overflow_are_errors() {
    let mut v = sample();
    let l = LayoutLimits {
        max_records: 1,
        ..Default::default()
    };
    assert!(v.plan(&l).is_err());
    v.base.metrics.line_gap = Some(i64::MAX);
    assert!(variants::encode(&v, &Default::default()).is_err());
}
#[test]
fn selected_glyph_closure_handles_chains_and_cycles() {
    let mut g = Gsub {
        scripts: BTreeMap::from([(
            *b"DFLT",
            Script {
                default: Some(LanguageSystem {
                    required: Some(0),
                    features: vec![],
                }),
                ..Default::default()
            },
        )]),
        features: vec![Feature {
            tag: *b"locl",
            lookups: vec![0, 1],
        }],
        lookups: vec![BTreeMap::from([(1, 2), (2, 3)]), BTreeMap::from([(3, 1)])],
    };
    let mut keep = BTreeSet::from([1]);
    g.closure(&mut keep);
    assert_eq!(keep, BTreeSet::from([1, 2, 3]));
    g.scripts.clear();
    let mut keep = BTreeSet::from([1]);
    g.closure(&mut keep);
    assert_eq!(keep, BTreeSet::from([1]));
}

#[test]
fn vertical_metrics_survive_variant_export_and_subset() {
    let mut f = face("##\n.#", 4);
    let id = f.glyph_id(Codepoint::Unicode(65)).unwrap();
    f.edit_glyph(id, |g| {
        g.metrics.vertical_advance = Some(Vector::new(0, -14));
        g.metrics.vertical_origin = Some(Vector::new(2, 12));
        Ok(())
    })
    .unwrap();
    let e = variants::encode(&Variants::new(f), &Default::default()).unwrap();
    let l = sfnt::Limits::default();
    let f = sfnt::Sfnt::parse(&e.bytes, &l).unwrap();
    let (_, m) = sfnt::metrics::decode(
        sfnt::metrics::Axis::Vertical,
        f.table(*b"vhea").unwrap(),
        f.table(*b"vmtx").unwrap(),
        2,
        &l,
    )
    .unwrap();
    assert_eq!(
        m[1],
        sfnt::metrics::LongMetric {
            advance: 1400,
            bearing: 1000
        }
    );
    let sub = sfnt::subset::subset(&e.bytes, &sfnt::subset::Options::text("A")).unwrap();
    let f = sfnt::Sfnt::parse(&sub.bytes, &l).unwrap();
    assert!(f.table(*b"vmtx").is_some());
}

#[test]
fn subset_preserves_explicit_truetype_bbox_bytes() {
    let e = variants::encode(&Variants::new(face("##\n.#", 4)), &Default::default()).unwrap();
    let l = sfnt::Limits::default();
    let f = sfnt::Sfnt::parse(&e.bytes, &l).unwrap();
    let mut data = f.table(*b"glyf").unwrap().to_vec();
    let loca = f.table(*b"loca").unwrap();
    let start = usize::from(u16::from_be_bytes([loca[2], loca[3]])) * 2;
    data[start + 2..start + 4].copy_from_slice(&(-17i16).to_be_bytes());
    let mut b = sfnt::SfntBuilder::new(f.flavor());
    for t in f.tables() {
        b.insert(
            t.tag,
            if t.tag == *b"glyf" {
                data.clone()
            } else {
                t.data.to_vec()
            },
        )
        .unwrap();
    }
    let source = b.build(&l).unwrap();
    let sub = sfnt::subset::subset(&source, &sfnt::subset::Options::text("A")).unwrap();
    let font = sfnt::Sfnt::parse(&sub.bytes, &l).unwrap();
    assert_eq!(font.table(*b"glyf").unwrap(), data);
}

#[test]
fn subset_prunes_scaling_for_removed_sparse_strikes() {
    use sfnt::bitmap as sbit;
    let mut f = face("#", 4);
    let mut glyph = f
        .glyph_for_codepoint(Codepoint::Unicode(65))
        .unwrap()
        .clone();
    glyph.codepoints = vec![Codepoint::Unicode(66)];
    f.add_glyph(glyph).unwrap();
    let e = variants::encode(
        &Variants::new(f),
        &Options {
            embed_bitmaps: true,
            ..Default::default()
        },
    )
    .unwrap();
    let l = sfnt::Limits::default();
    let f = sfnt::Sfnt::parse(&e.bytes, &l).unwrap();
    let mut strikes = sbit::decode(
        f.table(*b"EBLC").unwrap(),
        f.table(*b"EBDT").unwrap(),
        sbit::Kind::OpenType,
        3,
        &Default::default(),
    )
    .unwrap();
    let mut second = strikes[0].clone();
    second.ppem_x = 28;
    second.ppem_y = 28;
    second.glyphs.retain(|&id, _| id == 2);
    strikes[0].glyphs.retain(|&id, _| id != 2);
    strikes.push(second);
    let bitmap = sbit::encode(&strikes, sbit::Kind::OpenType, 3, &Default::default()).unwrap();
    let scale = sbit::Scale {
        horizontal: Default::default(),
        vertical: Default::default(),
        target: [56, 56],
        source: [28, 28],
    };
    let mut b = sfnt::SfntBuilder::new(f.flavor());
    for t in f.tables() {
        if ![*b"EBDT", *b"EBLC"].contains(&t.tag) {
            b.insert(t.tag, t.data.to_vec()).unwrap();
        }
    }
    b.insert(*b"EBDT", bitmap.data).unwrap();
    b.insert(*b"EBLC", bitmap.location).unwrap();
    b.insert(
        *b"EBSC",
        sbit::encode_scaling(&[scale], &Default::default()).unwrap(),
    )
    .unwrap();
    let source = b.build(&l).unwrap();
    let s = sfnt::subset::subset(&source, &sfnt::subset::Options::text("A")).unwrap();
    let f = sfnt::Sfnt::parse(&s.bytes, &l).unwrap();
    assert!(f.table(*b"EBSC").is_none());
    let strikes = sbit::decode(
        f.table(*b"EBLC").unwrap(),
        f.table(*b"EBDT").unwrap(),
        sbit::Kind::OpenType,
        2,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(strikes.len(), 1);
}

#[test]
fn writer_repairs_missing_dflt_default_without_changing_language_routing() {
    let mut g = sample().plan(&Default::default()).unwrap().gsub;
    g.scripts.get_mut(b"DFLT").unwrap().default = None;
    let b = g.encode(5, &Default::default()).unwrap();
    let result = Gsub::decode(&b, 5, &Default::default()).unwrap();
    assert_eq!(
        result.scripts[b"DFLT"].default,
        Some(LanguageSystem::default())
    );
    assert_eq!(
        result.scripts[b"DFLT"].languages,
        g.scripts[b"DFLT"].languages
    );
}

#[test]
fn maximum_context_matches_actual_layout_presence() {
    for (v, context) in [(Variants::new(face("#", 4)), 0), (sample(), 1)] {
        let e = variants::encode(&v, &Default::default()).unwrap();
        let f = sfnt::Sfnt::parse(&e.bytes, &Default::default()).unwrap();
        let os =
            sfnt::os2::Os2Table::decode(f.table(*b"OS/2").unwrap(), &Default::default()).unwrap();
        assert_eq!(os.extension.unwrap().max_context, context);
    }
}
