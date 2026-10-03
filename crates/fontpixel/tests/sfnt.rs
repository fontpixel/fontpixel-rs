//! Bounded sfnt/common-table roundtrips, corruptions and independent goldens.
#![cfg(feature = "sfnt")]
use fontpixel::sfnt::{
    self, Flavor, Limits, Sfnt, SfntBuilder, cmap, gasp,
    metrics::{self, Axis, Header, LongMetric},
    name::{NameRecord, NameTable},
    os2::{self, Os2Table},
    post::PostTable,
};
use proptest::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
const GOLDEN: &[u8] = include_bytes!("../../../tests/data/compat/mini-square-1.0-0.ttf");
fn sample() -> Vec<u8> {
    GOLDEN.to_vec()
}
fn be16(b: &[u8], o: usize) -> u16 {
    u16::from_be_bytes(b[o..o + 2].try_into().unwrap())
}
fn be32(b: &[u8], o: usize) -> u32 {
    u32::from_be_bytes(b[o..o + 4].try_into().unwrap())
}
fn set16(b: &mut [u8], o: usize, n: u16) {
    b[o..o + 2].copy_from_slice(&n.to_be_bytes());
}
fn set32(b: &mut [u8], o: usize, n: u32) {
    b[o..o + 4].copy_from_slice(&n.to_be_bytes());
}
fn only_bmp(data: &[u8]) -> Vec<u8> {
    let off = be32(data, 8) as usize;
    let len = be16(data, off + 2) as usize;
    let mut out = vec![0, 0, 0, 1, 0, 3, 0, 1, 0, 0, 0, 12];
    out.extend(&data[off..off + len]);
    out
}
#[test]
fn golden_container_reassembly_is_byte_exact() {
    let limits = Limits::default();
    let f = Sfnt::parse(GOLDEN, &limits).unwrap();
    f.validate_checksums().unwrap();
    let mut b = SfntBuilder::new(f.flavor());
    for t in f.tables().iter().rev() {
        b.insert(t.tag, t.data.to_vec()).unwrap();
    }
    assert_eq!(b.build(&limits).unwrap(), GOLDEN);
    assert_eq!(sfnt::checksum(GOLDEN), 0xb1b0afba);
}
#[test]
fn checksum_padding_uses_big_endian_words() {
    assert_eq!(sfnt::checksum(&[1, 2, 3, 4, 5]), 0x06020304);
    assert_eq!(sfnt::checksum(&[255; 8]), 0xfffffffe);
}
#[test]
fn structural_errors_are_rejected_before_access() {
    let l = Limits::default();
    for end in 0..12 {
        assert!(Sfnt::parse(&GOLDEN[..end], &l).is_err());
    }
    let mut d = sample();
    set32(&mut d, 20, u32::MAX);
    assert!(Sfnt::parse(&d, &l).is_err());
    let mut d = sample();
    let tag = d[12..16].to_vec();
    d[28..32].copy_from_slice(&tag);
    assert!(Sfnt::parse(&d, &l).is_err());
    let mut d = sample();
    set32(&mut d, 20, 12);
    assert!(Sfnt::parse(&d, &l).is_err());
    let mut d = sample();
    let off = be32(&d, 20);
    set32(&mut d, 36, off);
    assert!(Sfnt::parse(&d, &l).is_err());
    let mut d = sample();
    d[..4].copy_from_slice(b"ttcf");
    assert!(Sfnt::parse(&d, &l).is_err());
    assert!(
        Sfnt::parse(
            GOLDEN,
            &Limits {
                max_tables: 1,
                ..l.clone()
            }
        )
        .is_err()
    );
    let mut l = l;
    l.resources.max_table_bytes = 1;
    assert!(Sfnt::parse(GOLDEN, &l).is_err());
}
#[test]
fn checksum_checks_are_explicit_and_cover_all_components() {
    let l = Limits::default();
    let mut d = sample();
    d[16] ^= 1;
    let f = Sfnt::parse(&d, &l).unwrap();
    assert!(f.validate_checksums().is_err());
    let mut d = sample();
    d[8] ^= 1;
    assert!(Sfnt::parse(&d, &l).unwrap().validate_checksums().is_err());
    let f = Sfnt::parse(GOLDEN, &l).unwrap();
    let h = f
        .tables()
        .iter()
        .find(|r| r.tag == *b"head")
        .unwrap()
        .offset as usize;
    let mut d = sample();
    d[h + 8] ^= 1;
    assert!(Sfnt::parse(&d, &l).unwrap().validate_checksums().is_err());
    let mut d = sample();
    let a = d[12..28].to_vec();
    let b = d[28..44].to_vec();
    d[12..28].copy_from_slice(&b);
    d[28..44].copy_from_slice(&a);
    assert!(Sfnt::parse(&d, &l).unwrap().validate_checksums().is_err());
}
#[test]
fn writer_rejects_duplicates_missing_head_and_budgets() {
    let l = Limits::default();
    let mut b = SfntBuilder::new(Flavor::Cff);
    assert!(b.build(&l).is_err());
    b.insert(*b"head", vec![0; 12]).unwrap();
    assert!(b.insert(*b"head", vec![]).is_err());
    assert!(b.insert([0; 4], vec![]).is_err());
    let mut small = l;
    small.resources.max_total_bytes = 20;
    assert!(b.build(&small).is_err());
}
#[test]
fn unknown_tables_odd_lengths_and_stale_adjustment_repack() {
    let l = Limits::default();
    let f = Sfnt::parse(GOLDEN, &l).unwrap();
    for flavor in [Flavor::TrueType, Flavor::AppleTrueType, Flavor::Cff] {
        let mut b = SfntBuilder::new(flavor);
        for t in f.tables() {
            b.insert(t.tag, t.data.to_vec()).unwrap();
        }
        b.insert(*b"ZZZZ", vec![1, 2, 3]).unwrap();
        let raw = b.build(&l).unwrap();
        let read = Sfnt::parse(&raw, &l).unwrap();
        read.validate_checksums().unwrap();
        assert_eq!(read.table(*b"ZZZZ"), Some([1, 2, 3].as_slice()));
        assert_eq!(read.flavor(), flavor);
    }
}
#[test]
fn multilingual_name_and_raw_encoding_roundtrip() {
    let l = Limits::default();
    let n = NameTable {
        records: vec![
            NameRecord::unicode(1, 0x8000, "点阵 😀"),
            NameRecord::unicode(2, 0x409, "Regular"),
            NameRecord {
                platform_id: 1,
                encoding_id: 0,
                language_id: 0,
                name_id: 1,
                value: vec![0x80, 0x81],
            },
        ],
        language_tags: vec!["zh-Hant".into()],
    };
    let data = n.encode(&l).unwrap();
    let back = NameTable::decode(&data, &l).unwrap();
    assert_eq!(back.language_tags, n.language_tags);
    assert_eq!(back.records[0].value, vec![0x80, 0x81]);
    assert_eq!(back.records[0].text().unwrap(), None);
    assert!(
        back.records
            .iter()
            .any(|r| r.text().unwrap() == Some("点阵 😀".into()))
    );
    assert_eq!(back.encode(&l).unwrap(), data);
}
#[test]
fn name_malformed_storage_language_and_utf16() {
    let l = Limits::default();
    let n = NameTable {
        records: vec![
            NameRecord::unicode(1, 0x409, "same"),
            NameRecord::unicode(2, 0x409, "same"),
        ],
        language_tags: vec![],
    };
    let mut b = n.encode(&l).unwrap();
    assert_eq!(be16(&b, 16), be16(&b, 28));
    set16(&mut b, 4, 0);
    assert!(NameTable::decode(&b, &l).is_err());
    let bad = NameTable {
        records: vec![NameRecord::unicode(1, 0x8000, "x")],
        language_tags: vec![],
    };
    assert!(bad.encode(&l).is_err());
    let mut bad = NameRecord::unicode(1, 0, "x");
    bad.value = vec![0xd8, 0];
    assert!(bad.text().is_err());
    bad.value = vec![1];
    assert!(bad.text().is_err());
    assert!(
        n.encode(&Limits {
            max_name_records: 1,
            ..l
        })
        .is_err()
    );
}
#[test]
fn cmap_bmp_supplementary_aliases_and_ffff() {
    let l = Limits::default();
    let m = BTreeMap::from([
        (0, 1),
        (65, 2),
        (66, 1),
        (67, 3),
        (0xfffe, 2),
        (0xffff, 3),
        (0x10000, 1),
        (0x10ffff, 2),
    ]);
    let e = cmap::encode(&m, 4, &l).unwrap();
    assert!(e.has_bmp_subtable);
    assert_eq!(cmap::decode(&e.data, 4, &l).unwrap(), m);
    let bmp = only_bmp(&e.data);
    let expected = m
        .iter()
        .filter(|(c, _)| **c < 0xffff)
        .map(|(c, g)| (*c, *g))
        .collect();
    assert_eq!(cmap::decode(&bmp, 4, &l).unwrap(), expected);
    assert!(cmap::encode(&BTreeMap::from([(0xd800, 1)]), 2, &l).is_err());
    assert!(cmap::encode(&BTreeMap::from([(65, 0)]), 2, &l).is_err());
    assert!(cmap::encode(&BTreeMap::from([(65, 2)]), 2, &l).is_err());
}
#[test]
fn cmap_oversized_bmp_falls_back_to_complete_format12() {
    let l = Limits::default();
    let m = (0..0xd800).step_by(2).map(|c| (c, 1)).collect();
    let encoded = cmap::encode(&m, 2, &l).unwrap();
    assert!(!encoded.has_bmp_subtable);
    assert_eq!(cmap::decode(&encoded.data, 2, &l).unwrap(), m);
}
#[test]
fn cmap_empty_and_dense_nonsequential_maps() {
    let l = Limits::default();
    let m = BTreeMap::new();
    assert_eq!(
        cmap::decode(&cmap::encode(&m, 1, &l).unwrap().data, 1, &l).unwrap(),
        m
    );
    let m = (0..1000).map(|c| (c, 1 + (c % 7) as u16)).collect();
    let e = cmap::encode(&m, 8, &l).unwrap();
    let b = only_bmp(&e.data);
    assert!(b.len() < 2100);
    assert_eq!(cmap::decode(&b, 8, &l).unwrap(), m);
}
#[test]
fn cmap_malformed_groups_offsets_order_and_budget() {
    let l = Limits::default();
    let m = BTreeMap::from([(65, 1), (66, 3), (67, 2), (0x10000, 1)]);
    let e = cmap::encode(&m, 4, &l).unwrap();
    let mut b = e.data.clone();
    let off = be32(&b, 16) as usize;
    set32(&mut b, off + 20, u32::MAX);
    assert!(cmap::decode(&b, 4, &l).is_err());
    let mut b = only_bmp(&e.data);
    let sub = 12;
    let count = be16(&b, sub + 6) as usize / 2;
    let rangeoffset = sub + 16 + 6 * count;
    set16(&mut b, rangeoffset, 1);
    assert!(cmap::decode(&b, 4, &l).is_err());
    let mut b = e.data.clone();
    set32(&mut b, 32, 1);
    assert!(cmap::decode(&b, 4, &l).is_err());
    let mut b = only_bmp(&e.data);
    set16(&mut b, sub + 2, 16);
    assert!(cmap::decode(&b, 4, &l).is_err());
    assert!(
        cmap::decode(
            &e.data,
            4,
            &Limits {
                max_cmap_mappings: 2,
                ..l
            }
        )
        .is_err()
    );
}
#[test]
fn metrics_axes_compaction_and_empty_ink() {
    let l = Limits::default();
    let m = vec![
        LongMetric {
            advance: 400,
            bearing: -10,
        },
        LongMetric {
            advance: 500,
            bearing: 20,
        },
        LongMetric {
            advance: 500,
            bearing: 0,
        },
    ];
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let mut h = Header::new(axis);
        h.ascender = 400;
        h.descender = -100;
        h.recompute(&m, &[Some(300), Some(200), None]).unwrap();
        assert_eq!(
            (h.min_side_bearing, h.min_other_side_bearing, h.max_extent),
            (-10, 110, 290)
        );
        for compact in [true, false] {
            let e = metrics::encode(axis, &h, &m, compact, &l).unwrap();
            assert_eq!(e.num_long_metrics, if compact { 2 } else { 3 });
            let (r, rm) = metrics::decode(axis, &e.header, &e.metrics, 3, &l).unwrap();
            h.num_long_metrics = e.num_long_metrics;
            assert_eq!(r, h);
            assert_eq!(rm, m);
        }
    }
}
#[test]
fn metrics_invalid_counts_truncation_and_extents() {
    let l = Limits::default();
    let mut h = Header::new(Axis::Horizontal);
    let m = [LongMetric {
        advance: 500,
        bearing: 0,
    }];
    let mut e = metrics::encode(Axis::Horizontal, &h, &m, true, &l).unwrap();
    assert!(metrics::decode(Axis::Horizontal, &e.header, &e.metrics[..3], 1, &l).is_err());
    set16(&mut e.header, 34, 0);
    assert!(metrics::decode(Axis::Horizontal, &e.header, &e.metrics, 1, &l).is_err());
    assert!(h.recompute(&m, &[Some(65535)]).is_err());
    assert!(metrics::encode(Axis::Horizontal, &h, &[], true, &l).is_err());
}
#[test]
fn os2_every_version_and_field_roundtrips() {
    let l = Limits::default();
    for version in 0..=5 {
        let mut t = Os2Table {
            version,
            ..Default::default()
        };
        if version == 0 {
            t.code_page_ranges = None;
        }
        if version < 2 {
            t.extension = None;
        }
        if version == 5 {
            t.optical_sizes = Some([8 * 20, 72 * 20]);
        }
        t.base.subscript = [100, 200, -3, 4];
        t.base.superscript = [300, 400, -5, 6];
        t.base.strikeout = [7, 8];
        t.base.typo_metrics = [500, -100, 4];
        t.base.windows_metrics = [520, 110];
        t.base.panose = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let b = t.encode(&l).unwrap();
        assert_eq!(
            b.len(),
            match version {
                0 => 78,
                1 => 86,
                5 => 100,
                _ => 96,
            }
        );
        assert_eq!(Os2Table::decode(&b, &l).unwrap(), t);
        for n in 0..b.len() {
            assert!(Os2Table::decode(&b[..n], &l).is_err());
        }
    }
    let t = Os2Table {
        version: 0,
        ..Default::default()
    };
    assert!(t.encode(&l).is_err());
}
#[test]
fn os2_unicode_ranges_and_average_are_not_heuristic_guesses() {
    let mut t = Os2Table::default();
    let cps = BTreeSet::from([65, 0x100, 0x400, 0x4e00, 0x10000, 0x1f600, 0x100000]);
    t.set_unicode_coverage(&cps).unwrap();
    let r = t.base.unicode_ranges;
    for bit in [0, 2, 9, 57, 59, 90] {
        assert!(r[bit / 32] & (1 << (bit % 32)) != 0, "{bit}");
    }
    assert_eq!(t.base.first_char, 65);
    assert_eq!(t.base.last_char, 65535);
    assert!(os2::unicode_ranges(&BTreeSet::from([0xd800])).is_err());
    assert_eq!(os2::average_width([0, 1, 2]).unwrap(), 2);
    assert_eq!(os2::average_width([]).unwrap(), 0);
    assert!(os2::average_width([65535]).is_err());
}
#[test]
fn os2_codepages_require_full_printable_repertoire() {
    let codec = fontpixel::charset::CharsetCodec::new("cp1252", None).unwrap();
    let mut cps: BTreeSet<_> = (0..256)
        .filter_map(|c| codec.cp2char(c))
        .filter(|c| !c.is_control())
        .map(|c| c as u32)
        .collect();
    assert_eq!(os2::code_page_ranges(&cps, &[1252]).unwrap(), [1, 0]);
    cps.remove(&65);
    assert_eq!(os2::code_page_ranges(&cps, &[1252]).unwrap(), [0, 0]);
    assert!(os2::code_page_ranges(&cps, &[42]).is_err());
}
#[test]
fn post_named_and_nameless_preserve_fields() {
    let l = Limits::default();
    for names in [
        None,
        Some(vec![
            ".notdef".into(),
            "A".into(),
            "custom.name".into(),
            "custom.name".into(),
        ]),
    ] {
        let mut p = PostTable {
            names,
            ..Default::default()
        };
        p.header.italic_angle = -12 * 65536;
        p.header.underline_position = -50;
        p.header.underline_thickness = 25;
        let b = p.encode(4, &l).unwrap();
        assert_eq!(PostTable::decode(&b, 4, &l).unwrap(), p);
    }
    let mut p = PostTable {
        names: Some(vec!["bad/name".into()]),
        ..Default::default()
    };
    assert!(p.encode(1, &l).is_err());
    p.names = Some(vec!["x".repeat(256)]);
    assert!(p.encode(1, &l).is_err());
}
#[test]
fn post_bad_name_indexes_fail() {
    let l = Limits::default();
    let p = PostTable {
        names: Some(vec!["custom".into()]),
        ..Default::default()
    };
    let mut b = p.encode(1, &l).unwrap();
    set16(&mut b, 34, 65535);
    assert!(PostTable::decode(&b, 1, &l).is_err());
}
#[test]
fn gasp_versions_ranges_and_reserved_flags() {
    let l = Limits::default();
    for version in 0..=1 {
        let g = gasp::GaspTable {
            version,
            ranges: vec![
                gasp::Range {
                    max_ppem: 8,
                    behavior: 0,
                },
                gasp::Range {
                    max_ppem: 65535,
                    behavior: if version == 0 { 3 } else { 15 },
                },
            ],
        };
        let b = g.encode(&l).unwrap();
        assert_eq!(gasp::GaspTable::decode(&b, &l).unwrap(), g);
    }
    let bad = gasp::GaspTable {
        version: 0,
        ranges: vec![gasp::Range {
            max_ppem: 65535,
            behavior: 4,
        }],
    };
    assert!(bad.encode(&l).is_err());
}
proptest! {
    #[test]
    fn cmap_roundtrip(codes in prop::collection::btree_set(any::<char>(),0..200),ids in prop::collection::vec(1u16..500,200)){
        let l=Limits::default();let m: BTreeMap<_,_>=codes.iter().zip(ids).map(|(c,g)|(*c as u32,g)).collect();let b=cmap::encode(&m,500,&l).unwrap();prop_assert_eq!(cmap::decode(&b.data,500,&l).unwrap(),m);
    }
    #[test]
    fn metrics_roundtrip(ms in prop::collection::vec((any::<u16>(),any::<i16>()),1..200),compact in any::<bool>()){
        let l=Limits::default();let m:Vec<_>=ms.iter().map(|(a,b)|LongMetric{advance:*a,bearing:*b}).collect();let e=metrics::encode(Axis::Horizontal,&Header::new(Axis::Horizontal),&m,compact,&l).unwrap();let (_,back)=metrics::decode(Axis::Horizontal,&e.header,&e.metrics,m.len() as u16,&l).unwrap();prop_assert_eq!(back,m);
    }
    #[test]
    fn arbitrary_bytes_do_not_panic(bytes in prop::collection::vec(any::<u8>(),0..500)){
        let l=Limits::default();let _=Sfnt::parse(&bytes,&l);let _=cmap::decode(&bytes,100,&l);let _=NameTable::decode(&bytes,&l);let _=PostTable::decode(&bytes,100,&l);let _=Os2Table::decode(&bytes,&l);let _=gasp::GaspTable::decode(&bytes,&l);let _=metrics::decode(Axis::Vertical,&bytes,&bytes,100,&l);
    }
}

#[test]
fn fuzz_regression_truncated_fontations_lazy_arrays() {
    let l = Limits::default();
    let bytes = include_bytes!("../../../tests/data/sfnt/truncated-format4.cmap");
    assert!(cmap::decode(bytes, 256, &l).is_err());
    assert!(PostTable::decode(&[0, 3, 0, 0], 4, &l).is_err());
    let e = cmap::encode(&BTreeMap::from([(65, 1)]), 2, &l).unwrap();
    let mut b = e.data;
    let off = be32(&b, 32) as usize;
    set32(&mut b, off + 12, 1000);
    assert!(cmap::decode(&b, 2, &l).is_err());
}

#[test]
fn unicode_range_boundaries_match_fonttools() {
    let cases: Vec<(u32, [u32; 4])> =
        serde_json::from_str(include_str!("../../../tests/data/sfnt/unicode-ranges.json")).unwrap();
    for (cp, expected) in cases {
        assert_eq!(
            os2::unicode_ranges(&BTreeSet::from([cp])).unwrap(),
            expected,
            "U+{cp:X}"
        );
    }
}

#[test]
fn raw_flags_are_not_silently_truncated_by_fontations() {
    let l = Limits::default();
    let mut o = Os2Table::default();
    o.base.selection_flags = 0xabcd;
    let b = o.encode(&l).unwrap();
    assert_eq!(
        Os2Table::decode(&b, &l).unwrap().base.selection_flags,
        0xabcd
    );
    let g = gasp::GaspTable {
        version: 1,
        ranges: vec![gasp::Range {
            max_ppem: 65535,
            behavior: 15,
        }],
    };
    let mut raw = g.encode(&l).unwrap();
    set16(&mut raw, 6, 0x800f);
    assert!(gasp::GaspTable::decode(&raw, &l).is_err());
}
#[test]
fn native_post_names_follow_opentype_63_byte_alphabet() {
    let l = Limits::default();
    for s in ["x".repeat(64), "x-y".into(), "x+y".into()] {
        let p = PostTable {
            names: Some(vec![s]),
            ..Default::default()
        };
        assert!(p.encode(1, &l).is_err());
    }
}

#[test]
fn reads_legacy_and_trimmed_unicode_cmap_formats() {
    fn wrap(sub: &[u8]) -> Vec<u8> {
        let mut data = vec![0, 0, 0, 1, 0, 0, 0, 3, 0, 0, 0, 12];
        data.extend(sub);
        data
    }
    let l = Limits::default();
    let mut zero = vec![0; 262];
    set16(&mut zero, 2, 262);
    zero[6 + 65] = 2;
    assert_eq!(
        cmap::decode(&wrap(&zero), 4, &l).unwrap(),
        BTreeMap::from([(65, 2)])
    );
    let six = [0, 6, 0, 14, 0, 0, 0, 65, 0, 2, 0, 2, 0, 3];
    assert_eq!(
        cmap::decode(&wrap(&six), 4, &l).unwrap(),
        BTreeMap::from([(65, 2), (66, 3)])
    );
    let mut ten = vec![0; 24];
    set16(&mut ten, 0, 10);
    set32(&mut ten, 4, 24);
    set32(&mut ten, 12, 0x10000);
    set32(&mut ten, 16, 2);
    set16(&mut ten, 20, 2);
    set16(&mut ten, 22, 3);
    assert_eq!(
        cmap::decode(&wrap(&ten), 4, &l).unwrap(),
        BTreeMap::from([(0x10000, 2), (0x10001, 3)])
    );
    set32(&mut ten, 16, 100);
    assert!(cmap::decode(&wrap(&ten), 4, &l).is_err());
}

#[test]
fn old_vertical_header_and_post_standard_names_are_readable() {
    let l = Limits::default();
    let h = Header {
        version: 0x10000,
        ..Header::new(Axis::Vertical)
    };
    let e = metrics::encode(
        Axis::Vertical,
        &h,
        &[LongMetric {
            advance: 100,
            bearing: -1,
        }],
        true,
        &l,
    )
    .unwrap();
    assert_eq!(
        metrics::decode(Axis::Vertical, &e.header, &e.metrics, 1, &l)
            .unwrap()
            .0
            .version,
        0x10000
    );
    let mut p = PostTable::default().encode(258, &l).unwrap();
    set32(&mut p, 0, 0x10000);
    let names = PostTable::decode(&p, 258, &l).unwrap().names.unwrap();
    assert_eq!(names.len(), 258);
    assert_eq!(names[0], ".notdef");
    assert_eq!(names[36], "A");
}
