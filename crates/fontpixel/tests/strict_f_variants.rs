//! Unit F: explicit outline-export table controls (`variants::MetadataOptions`)
//! and exact name tables.
#![cfg(feature = "sfnt")]
#[path = "support/variants_fixture.rs"]
mod fixture;
use fontpixel::{
    Error, Result, Variants,
    model::Spacing,
    outline::Shape,
    report::Impact,
    sfnt::{
        Sfnt,
        cmap::{self, FullRecords},
        glyf::{self, GlyfTable},
        name::{NameRecord, NameTable},
        os2::Os2Table,
        post::PostTable,
    },
    variants::{self, DotGlyph, GaspPolicy, GlyphOrder, MetadataOptions, Options},
};

/// One face without alternates: BMP glyphs out of name order relative to
/// code points (`u20000` sorts first by name), a monospaced spacing claim and
/// a source copyright.
fn single() -> Variants {
    let mut face = fixture::face(&[
        (0x42, "###\n#.#\n###", 4),
        (0x41, "#.\n##", 3),
        (0x20000, "#.#\n.#.\n#.#", 4),
        (0x4e00, "#####\n.....\n#####", 6),
    ]);
    face.names.copyright = Some("Source notice".into());
    face.metrics.spacing = Some(Spacing::Monospaced);
    Variants::new(face)
}
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([bytes[at], bytes[at + 1]])
}
fn sfnt(bytes: &[u8]) -> Sfnt<'_> {
    Sfnt::parse(bytes, &Default::default()).unwrap()
}
fn glyphs(bytes: &[u8]) -> (Vec<glyf::Glyph>, cmap::Mapping) {
    let font = sfnt(bytes);
    let n = u16_at(font.table(*b"maxp").unwrap(), 4);
    let format = u16_at(font.table(*b"head").unwrap(), 50) as i16;
    let table = GlyfTable::decode(
        font.table(*b"glyf").unwrap(),
        font.table(*b"loca").unwrap(),
        n,
        format,
        &Default::default(),
    )
    .unwrap();
    let mapping = cmap::decode(font.table(*b"cmap").unwrap(), n, &Default::default()).unwrap();
    (table.glyphs, mapping)
}
fn ttf(metadata: MetadataOptions, shape: Shape) -> Options {
    Options {
        shape,
        dot_size: if shape == Shape::Dot { 0.9 } else { 1. },
        flavor: variants::OutlineFlavor::TrueType,
        gray_threshold: Some(128),
        metadata,
        ..Default::default()
    }
}

#[test]
fn default_and_explicit_default_controls_are_byte_and_report_identical() {
    for (v, shape) in [
        (single(), Shape::Square),
        (single(), Shape::Dot),
        (Variants::new(fixture::single()), Shape::Dot),
    ] {
        let base = variants::encode(&v, &ttf(MetadataOptions::default(), shape)).unwrap();
        let explicit = MetadataOptions {
            gasp: GaspPolicy::Write,
            cmap_full_records: FullRecords::Always,
            glyph_order: GlyphOrder::Planned,
            dot_glyph: DotGlyph::Last,
            ..Default::default()
        };
        let same = variants::encode(&v, &ttf(explicit, shape)).unwrap();
        assert_eq!(same.bytes, base.bytes);
        assert_eq!(same.report, base.report);
        assert!(
            base.report
                .entries
                .iter()
                .all(|e| !e.code.starts_with("variants.metadata"))
        );
        let font = sfnt(&base.bytes);
        assert!(font.table(*b"gasp").is_some());
        assert_eq!(u16_at(font.table(*b"maxp").unwrap(), 14), 1);
        assert_eq!(u16_at(font.table(*b"cmap").unwrap(), 2), 4);
    }
}

#[test]
fn every_table_control_reaches_its_field_and_is_audited() {
    let v = single();
    let metadata = MetadataOptions {
        timestamps: Some([3_660_681_600, -7]),
        embedding_flags: Some(4),
        selection_flags: Some(0),
        break_char: Some(32),
        code_page_ranges: Some([1 << 29, 1 << 31]),
        fixed_pitch: Some(false),
        max_zones: Some(2),
        gasp: GaspPolicy::Omit,
        cmap_full_records: FullRecords::WhenNeeded,
        ..Default::default()
    };
    let base = variants::encode(&v, &ttf(MetadataOptions::default(), Shape::Square)).unwrap();
    let out = variants::encode(&v, &ttf(metadata, Shape::Square)).unwrap();
    let font = sfnt(&out.bytes);
    let head = font.table(*b"head").unwrap();
    assert_eq!(
        i64::from_be_bytes(head[20..28].try_into().unwrap()),
        3_660_681_600
    );
    assert_eq!(i64::from_be_bytes(head[28..36].try_into().unwrap()), -7);
    let os2 = Os2Table::decode(font.table(*b"OS/2").unwrap(), &Default::default()).unwrap();
    let native = Os2Table::decode(
        sfnt(&base.bytes).table(*b"OS/2").unwrap(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(native.base.selection_flags, 0x40);
    assert_ne!(native.code_page_ranges, Some([1 << 29, 1 << 31]));
    assert_eq!(
        (
            os2.base.embedding_flags,
            os2.base.selection_flags,
            os2.extension.as_ref().unwrap().break_char,
            os2.code_page_ranges
        ),
        (4, 0, 32, Some([1 << 29, 1 << 31]))
    );
    let n = u16_at(font.table(*b"maxp").unwrap(), 4);
    let post = PostTable::decode(font.table(*b"post").unwrap(), n, &Default::default()).unwrap();
    assert_eq!(post.header.is_fixed_pitch, 0);
    assert_eq!(u16_at(font.table(*b"maxp").unwrap(), 14), 2);
    assert!(font.table(*b"gasp").is_none());
    // The map has a supplementary scalar: (0,3), (3,1) and (3,10) only.
    let cmap_table = font.table(*b"cmap").unwrap();
    assert_eq!(u16_at(cmap_table, 2), 3);
    assert_eq!(glyphs(&out.bytes).1, glyphs(&base.bytes).1);
    // Outlines and advances are untouched.
    assert_eq!(glyphs(&out.bytes).0, glyphs(&base.bytes).0);
    assert_eq!(font.table(*b"hmtx"), sfnt(&base.bytes).table(*b"hmtx"));
    let audit = out
        .report
        .entries
        .iter()
        .find(|e| e.code == "variants.metadata_configuration")
        .unwrap();
    assert_eq!(audit.impact, Impact::None);
}

#[test]
fn codepoint_order_and_dot_after_notdef_renumber_without_changing_outlines() {
    let v = single();
    let base = variants::encode(&v, &ttf(MetadataOptions::default(), Shape::Dot)).unwrap();
    let out = variants::encode(
        &v,
        &ttf(
            MetadataOptions {
                glyph_order: GlyphOrder::Codepoint,
                dot_glyph: DotGlyph::AfterNotdef,
                ..Default::default()
            },
            Shape::Dot,
        ),
    )
    .unwrap();
    let (old, old_map) = glyphs(&base.bytes);
    let (new, new_map) = glyphs(&out.bytes);
    assert_eq!(old.len(), new.len());
    // Native: name order puts u20000 first and the dot last.
    assert_eq!(old_map[&0x20000], 1);
    assert!(matches!(&old[old.len() - 1], glyf::Glyph::Simple(_)));
    // Explicit: dot at 1, then code point order.
    assert!(matches!(&new[1], glyf::Glyph::Simple(_)));
    assert_eq!(
        new_map
            .iter()
            .map(|(&cp, &id)| (cp, id))
            .collect::<Vec<_>>(),
        [(0x41, 2), (0x42, 3), (0x4e00, 4), (0x20000, 5)]
    );
    // Each code point's composite has the same placements, now naming glyph 1.
    for (cp, &id) in &new_map {
        let (glyf::Glyph::Composite(a), glyf::Glyph::Composite(b)) =
            (&old[usize::from(old_map[cp])], &new[usize::from(id)])
        else {
            panic!("dot glyphs are composites");
        };
        assert_eq!(a.components.len(), b.components.len());
        for (x, y) in a.components.iter().zip(&b.components) {
            assert_eq!((x.glyph_id, y.glyph_id), (old.len() as u16 - 1, 1));
            assert_eq!((x.anchor, x.transform), (y.anchor, y.transform));
        }
    }
    // hmtx follows: the dot's advance moved from last to 1.
    let hm = |bytes: &[u8]| {
        let font = sfnt(bytes);
        let n = u16_at(font.table(*b"maxp").unwrap(), 4);
        fontpixel::sfnt::metrics::decode(
            fontpixel::sfnt::metrics::Axis::Horizontal,
            font.table(*b"hhea").unwrap(),
            font.table(*b"hmtx").unwrap(),
            n,
            &Default::default(),
        )
        .unwrap()
        .1
    };
    let (a, b) = (hm(&base.bytes), hm(&out.bytes));
    assert_eq!(a[a.len() - 1], b[1]);
    for (cp, &id) in &new_map {
        assert_eq!(a[usize::from(old_map[cp])], b[usize::from(id)]);
    }
}

#[test]
fn renumbering_rejects_regional_alternates_and_invalid_controls() {
    let regional = fixture::variants();
    for metadata in [
        MetadataOptions {
            glyph_order: GlyphOrder::Codepoint,
            ..Default::default()
        },
        MetadataOptions {
            dot_glyph: DotGlyph::AfterNotdef,
            ..Default::default()
        },
    ] {
        let error = variants::encode(&regional, &ttf(metadata, Shape::Dot)).unwrap_err();
        assert!(error.to_string().contains("regional alternates"), "{error}");
    }
    let v = single();
    for (metadata, shape) in [
        (
            MetadataOptions {
                dot_glyph: DotGlyph::AfterNotdef,
                ..Default::default()
            },
            Shape::Square,
        ),
        (
            MetadataOptions {
                embedding_flags: Some(0x10),
                ..Default::default()
            },
            Shape::Square,
        ),
        (
            MetadataOptions {
                selection_flags: Some(0x100),
                ..Default::default()
            },
            Shape::Square,
        ),
        (
            MetadataOptions {
                code_page_ranges: Some([0x0000_8000, 0]),
                ..Default::default()
            },
            Shape::Square,
        ),
    ] {
        assert!(matches!(
            variants::encode(&v, &ttf(metadata, shape)),
            Err(Error::InvalidInput(_))
        ));
    }
    let cff = Options {
        flavor: variants::OutlineFlavor::Cff,
        metadata: MetadataOptions {
            max_zones: Some(2),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        variants::encode(&v, &cff),
        Err(Error::InvalidInput(_))
    ));
}

fn exact_names(copyright: &str) -> NameTable {
    let mut table = NameTable::default();
    for (id, text) in [(0, copyright), (1, "Ledger"), (2, "Regular")] {
        table.records.push(NameRecord::unicode(id, 0x409, text));
    }
    table
}

#[test]
fn exact_name_table_reports_dropped_source_notices_as_loss() {
    let v = single();
    let kept = variants::encode(
        &v,
        &Options {
            names: Some(exact_names("Source notice")),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        !kept
            .report
            .entries
            .iter()
            .any(|e| e.code == "variants.metadata_name")
    );
    let dropped = variants::encode(
        &v,
        &Options {
            names: Some(exact_names("Other notice")),
            ..Default::default()
        },
    )
    .unwrap();
    let loss = dropped
        .report
        .entries
        .iter()
        .find(|e| e.code == "variants.metadata_name")
        .unwrap();
    assert_eq!(loss.impact, Impact::Loss);
    // An explicit copyright override already replaces the source notice.
    let overridden = variants::encode(
        &v,
        &Options {
            names: Some(exact_names("Other notice")),
            copyright: Some("Other notice".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        !overridden
            .report
            .entries
            .iter()
            .any(|e| e.code == "variants.metadata_name")
    );
    let names = NameTable::decode(
        sfnt(&dropped.bytes).table(*b"name").unwrap(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(names.records.len(), 3);
}

/// Smallest accepted ledger total; one byte less is a resource error.
fn minimal(v: &Variants, o: &Options) -> usize {
    let run = |total: usize| -> Result<Vec<u8>> {
        let mut o = o.clone();
        o.limits.sfnt.resources.max_total_bytes = total;
        Ok(variants::encode(v, &o)?.bytes)
    };
    let expected = run(256 << 20).unwrap();
    let (mut lo, mut hi) = (0usize, 256 << 20);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if run(mid).is_ok() {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    assert_eq!(run(hi).unwrap(), expected);
    assert!(matches!(run(hi - 1), Err(Error::ResourceLimit { .. })));
    hi
}

#[test]
fn renumbering_scratch_and_exact_names_are_charged_to_the_export_ledger() {
    let v = single();
    let base = minimal(&v, &ttf(MetadataOptions::default(), Shape::Dot));
    let renumbered = minimal(
        &v,
        &ttf(
            MetadataOptions {
                glyph_order: GlyphOrder::Codepoint,
                dot_glyph: DotGlyph::AfterNotdef,
                ..Default::default()
            },
            Shape::Dot,
        ),
    );
    assert!(renumbered > base, "{renumbered} {base}");
    let mut table = exact_names("Source notice");
    let small = minimal(
        &v,
        &Options {
            names: Some(table.clone()),
            ..Default::default()
        },
    );
    table.records[0].value.extend(std::iter::repeat_n(0, 4096));
    let large = minimal(
        &v,
        &Options {
            names: Some(table),
            ..Default::default()
        },
    );
    assert!(large >= small + 4096, "{large} {small}");
}
