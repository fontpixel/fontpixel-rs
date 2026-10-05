//! Subset admission and validation regressions through the public library API.
#![cfg(feature = "sfnt")]

use fontpixel::{
    Bitmap, Error, Font, Glyph, Variants,
    model::{Codepoint, Vector},
    outline::Shape,
    sfnt::{self, Sfnt, SfntBuilder, subset},
    variants,
};
use std::collections::BTreeMap;

const GOLDEN: &[u8] = include_bytes!("../../../tests/data/compat/mini-square-1.0-0.ttf");

fn constant_cmap(groups: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut data = Vec::new();
    for n in [0u16, 1, 3, 10] {
        data.extend(n.to_be_bytes());
    }
    data.extend(12u32.to_be_bytes());
    data.extend(13u16.to_be_bytes());
    data.extend(0u16.to_be_bytes());
    for n in [16 + 12 * groups.len() as u32, 0, groups.len() as u32] {
        data.extend(n.to_be_bytes());
    }
    for &(start, end, glyph) in groups {
        for n in [start, end, glyph] {
            data.extend(n.to_be_bytes());
        }
    }
    data
}

fn source_with_cmap(cmap: Vec<u8>) -> Vec<u8> {
    let limits = sfnt::Limits::default();
    let source = Sfnt::parse(GOLDEN, &limits).unwrap();
    let mut builder = SfntBuilder::new(source.flavor());
    for table in source.tables().iter().filter(|table| table.tag != *b"cmap") {
        builder.insert(table.tag, table.data.to_vec()).unwrap();
    }
    builder.insert(*b"cmap", cmap).unwrap();
    builder.build(&limits).unwrap()
}

#[test]
fn small_subset_does_not_allocate_the_complete_source_cmap() {
    let raw_cmap = constant_cmap(&[(0x20, 0x2000, 1)]);
    let source = source_with_cmap(raw_cmap.clone());
    let original = source.clone();
    let mut options = subset::Options::text("A");
    options.limits.sfnt.resources.max_total_bytes = 64 * 1024;
    // Materializing every mapping cannot fit this allowance. Selecting one
    // scalar must still validate the entire compressed source map.
    assert!(matches!(
        sfnt::cmap::decode(&raw_cmap, 4, &options.limits.sfnt),
        Err(Error::ResourceLimit { .. })
    ));
    let result = subset::subset(&source, &options).unwrap();
    assert_eq!(result.glyph_mapping, BTreeMap::from([(0, 0), (1, 1)]));
    assert_eq!(source, original);
    let font = Sfnt::parse(&result.bytes, &options.limits.sfnt).unwrap();
    font.validate_checksums().unwrap();
    assert_eq!(
        sfnt::cmap::decode(font.table(*b"cmap").unwrap(), 2, &options.limits.sfnt).unwrap(),
        BTreeMap::from([(65, 1)])
    );
}

#[test]
fn subset_rejects_an_invalid_mapping_outside_the_selection() {
    let source = source_with_cmap(constant_cmap(&[(65, 65, 1), (66, 66, 4)]));
    assert!(matches!(
        subset::subset(&source, &subset::Options::text("A")),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn subset_keeps_the_full_source_cmap_work_limit() {
    let source = source_with_cmap(constant_cmap(&[(0x20, 0x2000, 1)]));
    let mut options = subset::Options::text("A");
    options.limits.sfnt.max_cmap_mappings = 100;
    assert!(matches!(
        subset::subset(&source, &options),
        Err(Error::ResourceLimit {
            resource: "sfnt cmap mappings",
            limit: 100
        })
    ));
}

// ---------------------------------------------------------------------------
// One shared working ledger: exact boundaries and stage coverage.
// ---------------------------------------------------------------------------

const LEDGER: &str = "sfnt subset working bytes";

fn allowance(options: &subset::Options, bytes: usize) -> subset::Options {
    let mut options = options.clone();
    options.limits.sfnt.resources.max_total_bytes = bytes;
    options
}

/// Smallest working allowance that succeeds. Every smaller allowance must be
/// rejected as a resource limit, never as another error or partial output.
fn minimal_allowance(source: &[u8], options: &subset::Options) -> usize {
    let (mut low, mut high) = (0usize, 64 << 20);
    subset::subset(source, &allowance(options, high)).unwrap();
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        match subset::subset(source, &allowance(options, middle)) {
            Ok(_) => high = middle,
            Err(Error::ResourceLimit { .. }) => low = middle,
            Err(other) => panic!("allowance {middle}: {other:?}"),
        }
    }
    high
}

/// The exact peak reproduces the unconstrained result; one byte less is
/// rejected by the shared subset ledger itself.
fn exact_boundary(source: &[u8], options: &subset::Options) -> usize {
    let peak = minimal_allowance(source, options);
    let full = subset::subset(source, options).unwrap();
    let exact = subset::subset(source, &allowance(options, peak)).unwrap();
    assert_eq!(exact.bytes, full.bytes);
    assert_eq!(exact.glyph_mapping, full.glyph_mapping);
    assert_eq!(exact.report, full.report);
    match subset::subset(source, &allowance(options, peak - 1)) {
        Err(Error::ResourceLimit { resource, limit }) => {
            assert_eq!((resource, limit), (LEDGER, peak - 1));
        }
        other => panic!("one byte short of {peak}: {other:?}"),
    }
    peak
}

fn face(glyphs: &[(u32, &str)]) -> Font {
    let mut font = Font::default();
    font.metrics.pixel_size = Some(14);
    font.metrics.ascent = Some(14);
    font.metrics.descent = Some(4);
    for &(cp, pattern) in glyphs {
        let mut glyph = Glyph::new(pattern.parse::<Bitmap>().unwrap());
        glyph.codepoints = vec![Codepoint::Unicode(cp)];
        glyph.metrics.advance = Some(Vector::new(3, 0));
        font.add_glyph(glyph).unwrap();
    }
    font
}

const LATIN: [(u32, &str); 3] = [(65, "#.\n##"), (66, "##\n#."), (67, "#\n#")];

/// Regional language variants make the exporter emit GSUB.
fn exported(options: variants::Options) -> Vec<u8> {
    let mut v = Variants::new(face(&LATIN));
    v.add_language("zh-Hant", face(&[(65, "##\n.#")])).unwrap();
    v.add_language("ja", face(&[(66, ".#\n##")])).unwrap();
    variants::encode(&v, &options).unwrap().bytes
}

fn dots() -> variants::Options {
    variants::Options {
        shape: Shape::Dot,
        dot_size: 0.9,
        ..Default::default()
    }
}

fn rebuild(source: &[u8], replace: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let limits = sfnt::Limits::default();
    let font = Sfnt::parse(source, &limits).unwrap();
    let mut builder = SfntBuilder::new(font.flavor());
    for table in font.tables() {
        if !replace.iter().any(|(tag, _)| *tag == table.tag) {
            builder.insert(table.tag, table.data.to_vec()).unwrap();
        }
    }
    for (tag, data) in replace {
        builder.insert(*tag, data.clone()).unwrap();
    }
    builder.build(&limits).unwrap()
}

fn table(font: &[u8], tag: [u8; 4]) -> Vec<u8> {
    Sfnt::parse(font, &Default::default())
        .unwrap()
        .table(tag)
        .unwrap()
        .to_vec()
}

fn glyph_count(font: &[u8]) -> u16 {
    let maxp = table(font, *b"maxp");
    u16::from_be_bytes([maxp[4], maxp[5]])
}

fn nominal(font: &[u8]) -> BTreeMap<u32, u16> {
    sfnt::cmap::decode(
        &table(font, *b"cmap"),
        glyph_count(font),
        &Default::default(),
    )
    .unwrap()
}

/// Raw glyf records (including their two-byte alignment padding).
fn records(font: &[u8]) -> Vec<Vec<u8>> {
    let long = table(font, *b"head")[51] == 1;
    let loca = table(font, *b"loca");
    let glyf = table(font, *b"glyf");
    let offset = |i: usize| {
        if long {
            u32::from_be_bytes(loca[4 * i..4 * i + 4].try_into().unwrap()) as usize
        } else {
            usize::from(u16::from_be_bytes([loca[2 * i], loca[2 * i + 1]])) * 2
        }
    };
    (0..usize::from(glyph_count(font)))
        .map(|i| glyf[offset(i)..offset(i + 1)].to_vec())
        .collect()
}

/// Re-encode a TrueType source's glyf model with long loca offsets.
fn with_outlines(
    source: &[u8],
    edit: impl FnOnce(&mut sfnt::glyf::GlyfTable, &BTreeMap<u16, u16>),
) -> Vec<u8> {
    use sfnt::glyf::{GlyfTable, LocaFormat};
    let limits = sfnt::glyf::Limits::default();
    let font = Sfnt::parse(source, &limits.sfnt).unwrap();
    let mut model = GlyfTable::from_sfnt(&font, &limits).unwrap();
    let used = subset::subset(source, &subset::Options::text("A"))
        .unwrap()
        .glyph_mapping;
    edit(&mut model, &used);
    let encoded = model.encode(LocaFormat::Long, &limits).unwrap();
    let mut head = table(source, *b"head");
    head[50..52].copy_from_slice(&1i16.to_be_bytes());
    rebuild(
        source,
        &[
            (*b"glyf", encoded.glyf),
            (*b"loca", encoded.loca),
            (*b"head", head),
        ],
    )
}

#[test]
fn exact_allowance_and_one_byte_short_for_every_outline_kind() {
    let mut sources = vec![
        GOLDEN.to_vec(),
        exported(variants::Options {
            embed_bitmaps: true,
            ..Default::default()
        }),
        exported(dots()),
    ];
    if cfg!(feature = "cff") {
        sources.push(exported(variants::Options {
            flavor: variants::OutlineFlavor::Cff,
            ..dots()
        }));
    }
    for source in &sources {
        for text in ["A", "ABC", ""] {
            exact_boundary(source, &subset::Options::text(text));
        }
    }
}

#[test]
fn copied_global_tables_coexist_with_the_output_but_input_is_not_recharged() {
    const SIZE: usize = 256 * 1024;
    let options = subset::Options::text("A");
    let base = minimal_allowance(GOLDEN, &options);
    let meta = (0..SIZE).map(|i| i as u8).collect::<Vec<_>>();
    let source = rebuild(GOLDEN, &[(*b"meta", meta.clone())]);
    let peak = exact_boundary(&source, &options);
    // The ledger copy of the unchanged table and its bytes in the final sfnt
    // are live together; the borrowed source table is not charged again.
    assert!(peak - base >= 2 * SIZE, "{base} -> {peak}");
    assert!(peak - base < 2 * SIZE + 4096, "{base} -> {peak}");
    let output = subset::subset(&source, &options).unwrap();
    assert_eq!(table(&output.bytes, *b"meta"), meta);
}

/// A copy of the TrueType export whose unselected glyph C is a simple glyph
/// with `points` points.
fn with_unselected_simple_glyph(points: usize) -> (Vec<u8>, usize) {
    let source = exported(Default::default());
    let target = usize::from(nominal(&source)[&67]);
    let font = with_outlines(&source, |model, used| {
        assert!(!used.contains_key(&(target as u16)));
        let contour = (0..points)
            .map(|i| ((i % 128) as f64, (i / 128) as f64, true))
            .collect();
        model.glyphs[target] = sfnt::glyf::Glyph::simple(vec![contour]);
    });
    (font, target)
}

#[test]
fn unselected_simple_glyphs_are_validated_and_their_temporaries_charged() {
    const POINTS: usize = 60_000;
    let options = subset::Options::text("A");
    let (small, _) = with_unselected_simple_glyph(4);
    let (large, target) = with_unselected_simple_glyph(POINTS);
    let low = exact_boundary(&small, &options);
    let high = exact_boundary(&large, &options);
    // Fontations' points (8 bytes) and flags plus the decoded flag bytes are
    // live together, on top of the selection state, while C is validated.
    assert!(high >= 10 * POINTS, "{low} -> {high}");
    assert!(high - low >= 9 * POINTS, "{low} -> {high}");
    // C's outline is validated and discarded, never retained or emitted.
    let a = subset::subset(&small, &options).unwrap();
    let b = subset::subset(&large, &options).unwrap();
    assert_eq!(a.glyph_mapping, b.glyph_mapping);
    assert_eq!(table(&a.bytes, *b"glyf"), table(&b.bytes, *b"glyf"));
    // Validation of C is not bypassed: a reserved flag bit still fails.
    let mut glyf = table(&large, *b"glyf");
    let loca = table(&large, *b"loca");
    let start = u32::from_be_bytes(loca[4 * target..4 * target + 4].try_into().unwrap()) as usize;
    // Header (10), one contour end point (2), empty program length (2).
    glyf[start + 14] |= 0x80;
    let corrupt = rebuild(&large, &[(*b"glyf", glyf)]);
    match subset::subset(&corrupt, &options) {
        Err(Error::InvalidInput(message)) => {
            assert!(message.contains("reserved simple glyph flag"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

fn component_len(flags: u16) -> usize {
    4 + if flags & 1 != 0 { 4 } else { 2 }
        + if flags & 8 != 0 {
            2
        } else if flags & 64 != 0 {
            4
        } else if flags & 128 != 0 {
            8
        } else {
            0
        }
}

#[test]
fn composite_closure_renumbers_children_and_preserves_programs() {
    use sfnt::glyf::Glyph as Outline;
    let mut composites = BTreeMap::new();
    let source = with_outlines(&exported(dots()), |model, _| {
        // Hint programs are opaque bytes that must survive unchanged.
        for (gid, glyph) in model.glyphs.iter_mut().enumerate() {
            if let Outline::Composite(c) = glyph {
                c.instructions = vec![0xb0, gid as u8];
                composites.insert(gid as u16, c.components.clone());
            }
        }
    });
    let options = subset::Options::text("A");
    exact_boundary(&source, &options);
    let result = subset::subset(&source, &options).unwrap();
    let mapping = &result.glyph_mapping;
    let (before, after) = (records(&source), records(&result.bytes));
    let mut checked = 0;
    for (&old, &new) in mapping {
        let mut record = after[usize::from(new)].clone();
        if let Some(components) = composites.get(&old) {
            let mut at = 10;
            for component in components {
                assert!(mapping.contains_key(&component.glyph_id));
                let flags = u16::from_be_bytes([record[at], record[at + 1]]);
                let child = u16::from_be_bytes([record[at + 2], record[at + 3]]);
                assert_eq!(child, mapping[&component.glyph_id]);
                record[at + 2..at + 4].copy_from_slice(&component.glyph_id.to_be_bytes());
                at += component_len(flags);
            }
            checked += 1;
        }
        assert_eq!(record, before[usize::from(old)], "glyph {old}");
    }
    assert!(checked > 0);
    // An unselected composite cycle is still a graph error.
    let b = nominal(&source)[&66];
    assert!(!mapping.contains_key(&b) && composites.contains_key(&b));
    let mut glyf = table(&source, *b"glyf");
    let loca = table(&source, *b"loca");
    let start = u32::from_be_bytes(loca[4 * usize::from(b)..][..4].try_into().unwrap()) as usize;
    glyf[start + 12..start + 14].copy_from_slice(&b.to_be_bytes());
    let cyclic = rebuild(&source, &[(*b"glyf", glyf)]);
    match subset::subset(&cyclic, &options) {
        Err(Error::InvalidInput(message)) => {
            assert!(message.contains("cyclic composite glyph"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn strikes_post_names_and_vertical_metrics_follow_the_mapping() {
    use sfnt::{bitmap, metrics, post::PostTable};
    let mut base = face(&LATIN);
    for cp in [65, 66, 67] {
        let id = base.glyph_id(Codepoint::Unicode(cp)).unwrap();
        base.edit_glyph(id, |g| {
            // vmtx stores centered origins: x is half the advance.
            g.metrics.advance = Some(Vector::new(4, 0));
            g.metrics.vertical_advance = Some(Vector::new(0, -14));
            g.metrics.vertical_origin = Some(Vector::new(2, 12 - cp as i64 % 3));
            Ok(())
        })
        .unwrap();
    }
    let font = variants::encode(
        &Variants::new(base),
        &variants::Options {
            embed_bitmaps: true,
            ..Default::default()
        },
    )
    .unwrap()
    .bytes;
    let count = glyph_count(&font);
    let names = (0..count)
        .map(|i| match i {
            0 => ".notdef".to_string(),
            _ => format!("custom{i}"),
        })
        .collect::<Vec<_>>();
    let post = PostTable {
        header: Default::default(),
        names: Some(names.clone()),
    }
    .encode(count, &Default::default())
    .unwrap();
    let source = rebuild(&font, &[(*b"post", post)]);
    let options = subset::Options::text("B");
    exact_boundary(&source, &options);
    let result = subset::subset(&source, &options).unwrap();
    let n = result.glyph_mapping.len() as u16;
    let out = &result.bytes;
    let limits = sfnt::Limits::default();
    let post = PostTable::decode(&table(out, *b"post"), n, &limits).unwrap();
    let vertical = |font: &[u8], count| {
        metrics::decode(
            metrics::Axis::Vertical,
            &table(font, *b"vhea"),
            &table(font, *b"vmtx"),
            count,
            &limits,
        )
        .unwrap()
        .1
    };
    let strikes = |font: &[u8], count| {
        bitmap::decode(
            &table(font, *b"EBLC"),
            &table(font, *b"EBDT"),
            bitmap::Kind::OpenType,
            count,
            &Default::default(),
        )
        .unwrap()
    };
    let (vsource, voutput) = (vertical(&source, count), vertical(out, n));
    let (ssource, soutput) = (strikes(&source, count), strikes(out, n));
    assert_eq!(ssource.len(), soutput.len());
    for (&old, &new) in &result.glyph_mapping {
        let (old, new) = (usize::from(old), usize::from(new));
        assert_eq!(post.names.as_ref().unwrap()[new], names[old]);
        assert_eq!(voutput[new], vsource[old]);
        for (s, t) in ssource.iter().zip(&soutput) {
            assert_eq!(s.glyphs.get(&(old as u16)), t.glyphs.get(&(new as u16)));
        }
    }
}

#[test]
fn vorg_ltsh_hdmx_and_scaling_follow_the_mapping() {
    use sfnt::bitmap;
    let font = exported(variants::Options {
        embed_bitmaps: true,
        ..Default::default()
    });
    let count = glyph_count(&font);
    let glyphs = 0..count;
    let mut vorg = vec![0, 1, 0, 0, 0, 12];
    vorg.extend(count.to_be_bytes());
    for g in glyphs.clone() {
        vorg.extend(g.to_be_bytes());
        vorg.extend((100 + g).to_be_bytes());
    }
    let mut ltsh = vec![0, 0];
    ltsh.extend(count.to_be_bytes());
    ltsh.extend(glyphs.clone().map(|g| g as u8 + 1));
    let stride = (usize::from(count) + 5) & !3;
    let mut hdmx = vec![0, 0, 0, 1];
    hdmx.extend((stride as u32).to_be_bytes());
    let mut record = vec![0u8; stride];
    record[0] = 14;
    record[1] = 99;
    for g in glyphs {
        record[2 + usize::from(g)] = 3 + g as u8;
    }
    hdmx.extend(&record);
    let strike = bitmap::decode(
        &table(&font, *b"EBLC"),
        &table(&font, *b"EBDT"),
        bitmap::Kind::OpenType,
        count,
        &Default::default(),
    )
    .unwrap()
    .remove(0);
    let ebsc = bitmap::encode_scaling(
        &[bitmap::Scale {
            horizontal: Default::default(),
            vertical: Default::default(),
            target: [28, 28],
            source: [strike.ppem_x, strike.ppem_y],
        }],
        &Default::default(),
    )
    .unwrap();
    let source = rebuild(
        &font,
        &[
            (*b"VORG", vorg),
            (*b"LTSH", ltsh),
            (*b"hdmx", hdmx),
            (*b"EBSC", ebsc.clone()),
        ],
    );
    let options = subset::Options::text("A");
    exact_boundary(&source, &options);
    let result = subset::subset(&source, &options).unwrap();
    let (mapping, out) = (&result.glyph_mapping, &result.bytes);
    let n = mapping.len() as u16;
    let mut vorg = vec![0, 1, 0, 0, 0, 12];
    vorg.extend(n.to_be_bytes());
    let mut ltsh = vec![0, 0];
    ltsh.extend(n.to_be_bytes());
    let size = (usize::from(n) + 5) & !3;
    let mut hdmx = vec![0, 0, 0, 1];
    hdmx.extend((size as u32).to_be_bytes());
    let mut record = vec![0u8; size];
    record[0] = 14;
    for (&old, &new) in mapping {
        vorg.extend(new.to_be_bytes());
        vorg.extend((100 + old).to_be_bytes());
        ltsh.push(old as u8 + 1);
        record[2 + usize::from(new)] = 3 + old as u8;
    }
    record[1] = *record[2..].iter().max().unwrap();
    hdmx.extend(record);
    assert_eq!(table(out, *b"VORG"), vorg);
    assert_eq!(table(out, *b"LTSH"), ltsh);
    assert_eq!(table(out, *b"hdmx"), hdmx);
    assert_eq!(table(out, *b"EBSC"), ebsc);
}

#[test]
fn gsub_closure_and_dflt_repair_survive_the_exact_allowance() {
    use sfnt::gsub::{Feature, Gsub, LanguageSystem, Script};
    let font = exported(Default::default());
    let count = glyph_count(&font);
    let cmap = nominal(&font);
    let (a, b, c) = (cmap[&65], cmap[&66], cmap[&67]);
    let gsub = Gsub {
        scripts: BTreeMap::from([(
            *b"DFLU",
            Script {
                default: None,
                languages: BTreeMap::from([(
                    *b"ZHS ",
                    LanguageSystem {
                        required: None,
                        features: vec![0],
                    },
                )]),
            },
        )]),
        features: vec![Feature {
            tag: *b"salt",
            lookups: vec![0],
        }],
        lookups: vec![BTreeMap::from([(a, c)]), BTreeMap::from([(b, a)])],
    };
    // The encoder always writes a DFLT default; retag a script without one.
    let mut data = gsub.encode(count, &Default::default()).unwrap();
    assert_eq!(&data[12..16], b"DFLU");
    data[12..16].copy_from_slice(b"DFLT");
    let source = rebuild(&font, &[(*b"GSUB", data)]);
    let options = subset::Options::text("A");
    exact_boundary(&source, &options);
    let result = subset::subset(&source, &options).unwrap();
    let mapping = &result.glyph_mapping;
    // Only active lookups close the selection; the unused one is pruned.
    assert!(mapping.contains_key(&c) && !mapping.contains_key(&b));
    assert!(
        result
            .report
            .entries
            .iter()
            .any(|e| e.code == "subset.dflt_langsys_repaired")
    );
    let out = Gsub::decode(
        &table(&result.bytes, *b"GSUB"),
        mapping.len() as u16,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        out.lookups,
        vec![BTreeMap::from([(mapping[&a], mapping[&c])])]
    );
    assert_eq!(
        out.scripts[b"DFLT"].default,
        Some(LanguageSystem::default())
    );
}

/// Add one more encoding record to a cmap, pointing at a new subtable.
fn with_cmap_record(cmap: &[u8], platform: u16, encoding: u16, subtable: &[u8]) -> Vec<u8> {
    let n = usize::from(u16::from_be_bytes([cmap[2], cmap[3]]));
    let mut out = vec![0, 0];
    out.extend((n as u16 + 1).to_be_bytes());
    for i in 0..n {
        let record = &cmap[4 + 8 * i..12 + 8 * i];
        out.extend(&record[..4]);
        let offset = u32::from_be_bytes(record[4..8].try_into().unwrap()) + 8;
        out.extend(offset.to_be_bytes());
    }
    out.extend(platform.to_be_bytes());
    out.extend(encoding.to_be_bytes());
    out.extend((cmap.len() as u32 + 8).to_be_bytes());
    out.extend(&cmap[4 + 8 * n..]);
    out.extend(subtable);
    out
}

#[test]
fn table_policy_rejections_and_losses_are_unchanged() {
    let font = exported(Default::default());
    let text = subset::Options::text("A");
    // A digital signature cannot survive; its removal is a reported loss.
    let signed = rebuild(&font, &[(*b"DSIG", vec![0, 0, 0, 1, 0, 0, 0, 0])]);
    let result = subset::subset(&signed, &text).unwrap();
    assert!(result.report.has_loss());
    assert_eq!(result.report.entries[0].code, "subset.signature_removed");
    assert!(
        Sfnt::parse(&result.bytes, &Default::default())
            .unwrap()
            .table(*b"DSIG")
            .is_none()
    );
    exact_boundary(&signed, &text);
    // Unknown tables need an explicit drop, which is reported as loss.
    let unknown = rebuild(&font, &[(*b"TEST", vec![1, 2])]);
    assert!(matches!(
        subset::subset(&unknown, &text),
        Err(Error::InvalidInput(_))
    ));
    let mut dropping = text.clone();
    dropping.drop_tables.insert(*b"TEST");
    let result = subset::subset(&unknown, &dropping).unwrap();
    let entry = &result.report.entries[0];
    assert_eq!(
        (entry.code.as_str(), entry.message.as_str()),
        ("subset.table_dropped", "Explicitly discarded table TEST")
    );
    exact_boundary(&unknown, &dropping);
    // Required and glyph tables, and half of the vertical pair, stay guarded.
    for tags in [&[*b"cmap"][..], &[*b"glyf"], &[*b"vhea"], &[*b"OS/2"]] {
        let mut options = text.clone();
        options.drop_tables.extend(tags.iter().copied());
        assert!(matches!(
            subset::subset(&font, &options),
            Err(Error::InvalidInput(_))
        ));
    }
    // Variation sequences and legacy cmap records cannot be projected away.
    let cmap = table(&font, *b"cmap");
    let format0 = [&[0, 0, 1, 6, 0, 0][..], &[0; 256]].concat();
    for (platform, encoding, subtable) in [
        (0, 5, &[0, 14, 0, 0, 0, 10, 0, 0, 0, 0][..]),
        (1, 0, &format0[..]),
    ] {
        let cmap = with_cmap_record(&cmap, platform, encoding, subtable);
        let source = rebuild(&font, &[(*b"cmap", cmap)]);
        match subset::subset(&source, &text) {
            Err(Error::InvalidInput(message)) => {
                assert!(message.contains("variation or non-Unicode"), "{message}");
            }
            other => panic!("{other:?}"),
        }
    }
}

#[cfg(feature = "cff")]
#[test]
fn cff_subset_admits_its_output_once_without_item_copies() {
    use sfnt::cff;
    let limits = cff::Limits::default();
    let glyphs = (0..200)
        .map(|i| {
            let mut path = vec![cff::Command::MoveTo((0.0, 0.0))];
            for j in 0..400 {
                let x = f64::from((j * 37 + i) % 900);
                let y = f64::from((j * 53 + 2 * i) % 700);
                path.push(cff::Command::LineTo((x, y)));
            }
            path.push(cff::Command::Close);
            cff::Glyph {
                name: if i == 0 {
                    ".notdef".into()
                } else {
                    format!("g{i}")
                },
                advance: 500.0,
                outline: cff::Outline::Path(path),
            }
        })
        .collect();
    let font = cff::Font {
        postscript_name: "LedgerTest".into(),
        full_name: "Ledger Test".into(),
        units_per_em: 1000,
        glyphs,
        dots: Default::default(),
    };
    let data = font.encode(&limits).unwrap().data;
    let parsed = cff::Cff::parse(&data, 1000, &limits).unwrap();
    let order = (0..200).collect::<Vec<u16>>();
    let output = parsed.subset(&order, &limits).unwrap();
    assert!(output.len() > 64 * 1024);
    let with = |bytes| {
        let mut limits = limits.clone();
        limits.sfnt.resources.max_total_bytes = bytes;
        limits
    };
    let (mut low, mut high) = (0usize, 16 << 20);
    while low + 1 < high {
        let middle = low + (high - low) / 2;
        match parsed.subset(&order, &with(middle)) {
            Ok(_) => high = middle,
            Err(Error::ResourceLimit { .. }) => low = middle,
            Err(other) => panic!("{other:?}"),
        }
    }
    // The output is admitted once at its exact size; there is no INDEX item,
    // DICT template or Private block copy alongside it.
    assert!(high >= output.len());
    assert!(
        high < output.len() + 16 * 1024,
        "{high} vs {}",
        output.len()
    );
    assert_eq!(parsed.subset(&order, &with(high)).unwrap(), output);
    assert!(matches!(
        parsed.subset(&order, &with(high - 1)),
        Err(Error::ResourceLimit {
            resource: "CFF working bytes",
            ..
        })
    ));
    // CID fonts relocate every FD Private and the unused Top DICT Private.
    for data in [
        include_bytes!("../../../tests/data/cff/external-cid.cff").as_slice(),
        include_bytes!("../../../tests/data/cff/external-cid-format0.cff").as_slice(),
    ] {
        let parsed = cff::Cff::parse(data, 1400, &limits).unwrap();
        let order = vec![0, (parsed.len() - 1) as u16];
        let output = parsed.subset(&order, &limits).unwrap();
        let (mut low, mut high) = (0usize, 1 << 20);
        while low + 1 < high {
            let middle = low + (high - low) / 2;
            match parsed.subset(&order, &with(middle)) {
                Ok(_) => high = middle,
                Err(Error::ResourceLimit { .. }) => low = middle,
                Err(other) => panic!("{other:?}"),
            }
        }
        assert_eq!(parsed.subset(&order, &with(high)).unwrap(), output);
        assert!(high >= output.len());
        let reparsed = cff::Cff::parse(&output, 1400, &limits).unwrap();
        assert_eq!(
            parsed.fontations().charstrings().get(order[1].into()),
            reparsed.fontations().charstrings().get(1)
        );
    }
}
