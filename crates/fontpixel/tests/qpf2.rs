//! Original QPF2 wire tables and cached raster fixtures.
#![cfg(feature = "qpf2")]
use fontpixel::formats::qpf2::{self, GlyphFormat, Qpf2WriteOptions};
use fontpixel::model::{Codepoint, Property};
use fontpixel::{FontSet, ResourceLimits};
fn tag(h: &mut Vec<u8>, t: u16, v: &[u8]) {
    h.extend_from_slice(&t.to_be_bytes());
    h.extend_from_slice(&(v.len() as u16).to_be_bytes());
    h.extend_from_slice(v);
}
fn block(b: &mut Vec<u8>, t: u16, v: &[u8]) {
    b.extend_from_slice(&t.to_be_bytes());
    b.extend_from_slice(&[0, 0]);
    b.extend_from_slice(&(v.len() as u32).to_be_bytes());
    b.extend_from_slice(v);
}
fn fixture() -> Vec<u8> {
    let mut h = Vec::new();
    tag(&mut h, 0, b"Original test");
    for (t, v) in [(5, 192i32), (6, 64), (7, -64), (11, 64), (14, 64)] {
        tag(&mut h, t, &v.to_be_bytes());
    }
    tag(&mut h, 15, &[8]);
    tag(&mut h, 16, &[4]);
    tag(&mut h, 99, b"opaque");
    tag(&mut h, 19, &[]);
    while h.len() % 4 != 0 {
        h.push(0);
    }
    let mut b = b"QPF2\xff\xff\xff\xff\x02\x00".to_vec();
    b.extend_from_slice(&(h.len() as u16).to_be_bytes());
    b.extend_from_slice(&h);
    // Independent format 12 table: A and Z alias native glyph 1.
    let mut cmap = vec![
        0, 0, 0, 1, 0, 3, 0, 10, 0, 0, 0, 12, 0, 12, 0, 0, 0, 0, 0, 40, 0, 0, 0, 0, 0, 0, 0, 2,
    ];
    for cp in [65u32, 90] {
        cmap.extend_from_slice(&cp.to_be_bytes());
        cmap.extend_from_slice(&cp.to_be_bytes());
        cmap.extend_from_slice(&1u32.to_be_bytes());
    }
    block(&mut b, 0, &cmap);
    block(&mut b, 1, &[255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 14]);
    block(
        &mut b,
        2,
        &[
            3, 2, 4, 255, 253, 252, 0, 127, 255, 9, 1, 128, 254, 8, 0, 3, 0, 0, 0, 0,
        ],
    );
    block(&mut b, 25, b"RAW!");
    b
}
fn read(b: &[u8]) -> FontSet {
    qpf2::read(b, &Default::default(), &Default::default())
        .unwrap()
        .0
}
#[test]
fn native_gid_aliases_gray_stride_and_exact_replay() {
    let b = fixture();
    let s = read(&b);
    let f = s.single().unwrap();
    assert_eq!(f.slot_count(), 3);
    assert_eq!(f.len(), 2);
    let g = f.glyph_for_codepoint(Codepoint::Unicode(65)).unwrap();
    assert_eq!(
        g.codepoints,
        [Codepoint::Unicode(65), Codepoint::Unicode(90)]
    );
    assert_eq!(g.metrics.bounds.x, -1);
    assert_eq!(g.metrics.bounds.y, 1);
    assert_eq!(g.metrics.advance.unwrap().x, -4);
    assert_eq!(g.bitmap.as_bytes(), [0, 127, 255, 1, 128, 254]);
    assert_eq!(f.metrics.line_gap, Some(-1));
    let (out, r) = qpf2::write(&s, &Default::default()).unwrap();
    assert_eq!(out, b);
    assert!(!r.has_loss(), "{r}");
}
#[test]
fn canonical_writer_keeps_mapping_and_native_signed_advance() {
    let mut s = read(&fixture());
    s.fonts[0].properties.remove("qpf2.source");
    let (out, r) = qpf2::write(&s, &Default::default()).unwrap();
    assert!(!r.has_loss(), "{r}");
    let d = read(&out);
    assert_eq!(
        s.fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap(),
        d.fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap()
    );
    assert_eq!(d.fonts[0].slot_count(), 3);
}
#[test]
fn fractional_metrics_and_linkage_are_explicit_reader_loss() {
    let mut b = fixture();
    let at = b
        .windows(8)
        .position(|v| v == [0, 5, 0, 4, 0, 0, 0, 192])
        .unwrap();
    b[at + 7] = 193;
    let (s, r) = qpf2::read(&b, &Default::default(), &Default::default()).unwrap();
    assert!(r.entries.iter().any(|e| e.code == "qpf2.fractional_metric"));
    assert!(qpf2::write(&s, &Default::default()).unwrap().1.has_loss());
}
#[test]
fn monochrome_requires_explicit_gray_reduction() {
    let mut s = read(&fixture());
    s.fonts[0].properties.remove("qpf2.source");
    let mut o = Qpf2WriteOptions {
        glyph_format: GlyphFormat::Mono,
        ..Default::default()
    };
    assert!(qpf2::write(&s, &o).is_err());
    o.gray_threshold = Some(128);
    let (b, r) = qpf2::write(&s, &o).unwrap();
    assert!(r.has_loss());
    assert_eq!(
        read(&b).fonts[0]
            .glyph_for_codepoint(Codepoint::Unicode(65))
            .unwrap()
            .bitmap
            .as_bytes(),
        [0x20, 0x60]
    );
}
#[test]
fn truncations_out_of_range_offsets_and_budgets() {
    let b = fixture();
    for n in 0..b.len() {
        // Unknown final blocks can legitimately be absent.
        if n == b.len() - 12 {
            continue;
        }
        assert!(
            qpf2::read(&b[..n], &Default::default(), &Default::default()).is_err(),
            "prefix {n}"
        );
    }
    let mut bad = b.clone();
    let at = bad
        .windows(12)
        .position(|v| v == [0, 1, 0, 0, 0, 0, 0, 12, 255, 255, 255, 255])
        .unwrap();
    bad[at + 12..at + 16].copy_from_slice(&65535u32.to_be_bytes());
    assert!(qpf2::read(&bad, &Default::default(), &Default::default()).is_err());
    let s = read(&b);
    for max in [0, 1000, 4096, 10000, 100000] {
        let o = Qpf2WriteOptions {
            limits: ResourceLimits {
                max_total_bytes: max,
                ..Default::default()
            },
            ..Default::default()
        };
        if let Ok((out, _)) = qpf2::write(&s, &o) {
            assert!(qpf2::read(&out, &Default::default(), &o.limits).is_ok());
        }
    }
}
#[test]
fn unrepresented_model_metadata_is_reported() {
    let mut s = read(&fixture());
    s.fonts[0]
        .properties
        .push(Property::new("application-note", "keep me"));
    assert!(qpf2::write(&s, &Default::default()).unwrap().1.has_loss());
}

#[test]
fn strict_registry_rejects_unrepresented_metadata_and_removed_slots() {
    let mut set = read(&fixture());
    set.fonts[0]
        .properties
        .push(fontpixel::model::Property::new(
            "application-note",
            "must report",
        ));
    let id = set.fonts[0]
        .add_glyph(fontpixel::model::Glyph::default())
        .unwrap();
    set.fonts[0].remove_glyph(id);
    let mut options =
        fontpixel::WriteOptions::new(fontpixel::ResourcePath::new("out.qpf2").unwrap());
    options.strict = true;
    assert!(matches!(
        fontpixel::write(&set, "qpf2", &options),
        Err(fontpixel::Error::StrictLoss(_))
    ));
}

#[test]
fn cached_glyph_zero_is_native_unencoded_data_not_a_default_character() {
    let mut data = fixture();
    let at = data
        .windows(12)
        .position(|v| v == [0, 1, 0, 0, 0, 0, 0, 12, 255, 255, 255, 255])
        .unwrap();
    data[at + 8..at + 12].copy_from_slice(&0u32.to_be_bytes());
    let mut set = read(&data);
    assert_eq!(set.fonts[0].len(), 3);
    assert!(set.fonts[0].metrics.default_glyph.is_none());
    assert!(
        set.fonts[0]
            .glyphs()
            .next()
            .unwrap()
            .1
            .codepoints
            .is_empty()
    );
    set.fonts[0].properties.remove("qpf2.source");
    let (out, report) = qpf2::write(&set, &Default::default()).unwrap();
    assert!(!report.has_loss(), "{report}");
    let parsed = read(&out);
    assert_eq!(parsed.fonts[0].len(), 3);
    assert!(parsed.fonts[0].metrics.default_glyph.is_none());
}
