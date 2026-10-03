//! WOFF transport fidelity, malformed input, limits, and property regressions.
#![cfg(all(feature = "woff", feature = "woff2"))]
use fontpixel::{
    sfnt::{Sfnt, SfntBuilder},
    woff::{self, Auxiliary, Limits, woff1, woff2},
};
use proptest::prelude::*;
const FONT: &[u8] = include_bytes!("../../../tests/data/compat/mini-square-1.0-1.ttf");
fn options() -> woff2::Options {
    woff2::Options {
        quality: 2,
        ..Default::default()
    }
}
fn replace(source: &[u8], tag: [u8; 4], value: Vec<u8>) -> Vec<u8> {
    let l = Limits::default();
    let f = Sfnt::parse(source, &l.sfnt).unwrap();
    let mut b = SfntBuilder::new(f.flavor());
    for t in f.tables() {
        if t.tag != tag {
            b.insert(t.tag, t.data.to_vec()).unwrap();
        }
    }
    b.insert(tag, value).unwrap();
    b.build(&l.sfnt).unwrap()
}
fn equivalent(original: &[u8], encoded: &[u8], is2: bool) {
    let l = Limits::default();
    let d = if is2 {
        woff2::decode(encoded, &l)
    } else {
        woff1::decode(encoded, &l)
    }
    .unwrap();
    let a = Sfnt::parse(original, &l.sfnt).unwrap();
    let b = Sfnt::parse(&d.sfnt, &l.sfnt).unwrap();
    b.validate_checksums().unwrap();
    for table in a.tables() {
        if ![*b"head", *b"glyf", *b"loca"].contains(&table.tag) {
            assert_eq!(table.data, b.table(table.tag).unwrap());
        }
    }
    let gl = fontpixel::sfnt::glyf::Limits::default();
    assert_eq!(
        fontpixel::sfnt::glyf::GlyfTable::from_sfnt(&a, &gl).unwrap(),
        fontpixel::sfnt::glyf::GlyfTable::from_sfnt(&b, &gl).unwrap()
    );
}
#[test]
fn fixture_roundtrips_and_quality() {
    let l = Limits::default();
    for q in 0..=11 {
        let e = woff2::encode(
            FONT,
            &woff2::Options {
                quality: q,
                ..Default::default()
            },
            &l,
        )
        .unwrap();
        equivalent(FONT, &e.bytes, true);
    }
    let e = woff1::encode(FONT, &woff1::Options::default(), &l).unwrap();
    equivalent(FONT, &e.bytes, false);
}
#[test]
fn truncation_and_mutation_never_panic() {
    let l = Limits::default();
    let e2 = woff2::encode(FONT, &options(), &l).unwrap().bytes;
    let e1 = woff1::encode(FONT, &woff1::Options::default(), &l)
        .unwrap()
        .bytes;
    for (data, is2) in [(&e1, false), (&e2, true)] {
        for cut in 0..data.len() {
            let mut truncated = data[..cut].to_vec();
            if cut >= 12 {
                truncated[8..12].copy_from_slice(&(cut as u32).to_be_bytes());
            }
            let _ = if is2 {
                woff2::decode(&truncated, &l)
            } else {
                woff1::decode(&truncated, &l)
            };
        }
        for i in 0..data.len() {
            let mut mutated = data.clone();
            mutated[i] ^= 255;
            let _ = if is2 {
                woff2::decode(&mutated, &l)
            } else {
                woff1::decode(&mutated, &l)
            };
        }
    }
}
#[test]
fn limits_precede_decompression() {
    let mut l = Limits::default();
    let e = woff2::encode(FONT, &options(), &l).unwrap().bytes;
    for limit in [0, 1, 32] {
        l.sfnt.resources.max_decompressed_bytes = limit;
        assert!(woff2::decode(&e, &l).is_err());
    }
    l = Limits::default();
    l.max_total_points = 0;
    assert!(woff2::decode(&e, &l).is_err());
    assert!(woff2::encode(FONT, &options(), &l).is_err());
    l = Limits::default();
    l.sfnt.resources.max_total_bytes = 16;
    assert!(woff2::decode(&e, &l).is_err());
    assert!(woff1::encode(FONT, &woff1::Options::default(), &l).is_err());
}
#[test]
fn woff2_advisory_sizes_reserved_and_null_transform() {
    let l = Limits::default();
    let mut e = woff2::encode(FONT, &options(), &l).unwrap().bytes;
    e[14..16].copy_from_slice(&65535u16.to_be_bytes());
    e[16..20].copy_from_slice(&1u32.to_be_bytes());
    equivalent(FONT, &e, true);
    let o = woff2::Options {
        transform_glyf: false,
        transform_hmtx: false,
        ..options()
    };
    let e = woff2::encode(FONT, &o, &l).unwrap();
    assert_eq!(woff2::decode(&e.bytes, &l).unwrap().sfnt, FONT);
}
#[test]
fn signature_loss_is_explicit() {
    let l = Limits::default();
    let f = replace(FONT, *b"DSIG", vec![0; 8]);
    let e = woff2::encode(&f, &options(), &l).unwrap();
    assert!(e.report.has_loss());
    let d = woff2::decode(&e.bytes, &l).unwrap();
    assert!(
        Sfnt::parse(&d.sfnt, &l.sfnt)
            .unwrap()
            .table(*b"DSIG")
            .is_none()
    );
    let e = woff1::encode(&f, &woff1::Options::default(), &l).unwrap();
    assert!(!e.report.has_loss());
    assert_eq!(woff1::decode(&e.bytes, &l).unwrap().sfnt, f);
}
#[test]
fn metadata_errors_and_auxiliary_limits() {
    let l = Limits::default();
    let a = Auxiliary {
        metadata: Some(b"<metadata version=\"1.0\"/>".to_vec()),
        private_data: Some(vec![8; 7]),
        ..Default::default()
    };
    for is2 in [false, true] {
        let mut e = if is2 {
            woff2::encode(
                FONT,
                &woff2::Options {
                    auxiliary: a.clone(),
                    ..options()
                },
                &l,
            )
        } else {
            woff1::encode(
                FONT,
                &woff1::Options {
                    auxiliary: a.clone(),
                    ..Default::default()
                },
                &l,
            )
        }
        .unwrap()
        .bytes;
        let off = if is2 { 28 } else { 24 };
        let pos = u32::from_be_bytes(e[off..off + 4].try_into().unwrap()) as usize;
        e[pos] ^= 255;
        let d = if is2 {
            woff2::decode(&e, &l)
        } else {
            woff1::decode(&e, &l)
        }
        .unwrap();
        assert!(d.report.has_loss());
        assert!(d.auxiliary.metadata.is_none());
        assert_eq!(d.auxiliary.private_data, a.private_data);
        let mut limited = l.clone();
        limited.max_auxiliary_bytes = 2;
        assert!(
            if is2 {
                woff2::decode(&e, &limited)
            } else {
                woff1::decode(&e, &limited)
            }
            .is_err()
        );
    }
}
#[test]
fn invalid_options_and_collections() {
    let l = Limits::default();
    assert!(
        woff2::encode(
            FONT,
            &woff2::Options {
                quality: 12,
                ..options()
            },
            &l
        )
        .is_err()
    );
    assert!(
        woff2::encode(
            FONT,
            &woff2::Options {
                compression_threads: 0,
                ..options()
            },
            &l
        )
        .is_err()
    );
    assert!(
        woff1::encode(
            FONT,
            &woff1::Options {
                compression_level: 10,
                ..Default::default()
            },
            &l
        )
        .is_err()
    );
    let mut data = woff2::encode(FONT, &options(), &l).unwrap().bytes;
    data[4..8].copy_from_slice(b"ttcf");
    assert!(
        woff2::decode(&data, &l)
            .unwrap_err()
            .to_string()
            .contains("collection")
    );
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(80))]
    #[test]fn opaque_table_roundtrip(extra in prop::collection::vec(any::<u8>(),0..4096),null in any::<bool>()){
        let l=Limits::default();let f=replace(FONT,*b"ZZZZ",extra);
        let a=woff1::encode(&f,&woff1::Options::default(),&l).unwrap();equivalent(&f,&a.bytes,false);
        let b=woff2::encode(&f,&woff2::Options{transform_glyf:!null,transform_hmtx:!null,..options()},&l).unwrap();equivalent(&f,&b.bytes,true);
    }
    #[test]fn arbitrary_bytes_are_safe(data in prop::collection::vec(any::<u8>(),0..8192)){
        let mut l=woff::Limits::default();l.sfnt.resources.max_total_bytes=1<<20;l.sfnt.resources.max_decompressed_bytes=1<<20;
        let _=woff1::decode(&data,&l);let _=woff2::decode(&data,&l);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]
    #[test]
    fn random_simple_glyph_transform_roundtrip(points in prop::collection::vec((-2000i16..2000,-2000i16..2000,any::<bool>()),1..50),overlap in any::<bool>(),short in any::<bool>()){
        use fontpixel::sfnt::glyf::{GlyfTable,Glyph,SimpleGlyph,LocaFormat};
        let contours=vec![points.iter().map(|&(x,y,on)|(x as f64,y as f64,on)).collect()];
        let table=GlyfTable{glyphs:vec![Glyph::Simple(SimpleGlyph{contours,instructions:vec![0xb0,0,0x21],overlap});4]};
        let l=fontpixel::sfnt::glyf::Limits::default();
        let encoded=table.encode(if short{LocaFormat::Short}else{LocaFormat::Long},&l).unwrap();
        let mut data=replace(FONT,*b"glyf",encoded.glyf);data=replace(&data,*b"loca",encoded.loca);
        let mut head=Sfnt::parse(&data,&l.sfnt).unwrap().table(*b"head").unwrap().to_vec();head[50..52].copy_from_slice(&encoded.index_to_loc_format.to_be_bytes());data=replace(&data,*b"head",head);
        let result=woff2::encode(&data,&options(),&Limits::default()).unwrap();equivalent(&data,&result.bytes,true);
    }
}

#[test]
fn normalized_short_loca_upgrades_without_losing_glyphs() {
    use fontpixel::sfnt::glyf::{GlyfTable, Glyph, LocaFormat};
    let gl = fontpixel::sfnt::glyf::Limits::default();
    let count = 7000u16;
    let table = GlyfTable {
        glyphs: vec![Glyph::simple(vec![vec![(1.0, 1.0, true)]]); count as usize],
    };
    let e = table.encode(LocaFormat::Short, &gl).unwrap();
    assert_eq!(e.index_to_loc_format, 0);
    let mut data = replace(FONT, *b"glyf", e.glyf);
    data = replace(&data, *b"loca", e.loca);
    let mut head = Sfnt::parse(&data, &gl.sfnt)
        .unwrap()
        .table(*b"head")
        .unwrap()
        .to_vec();
    head[50..52].copy_from_slice(&0u16.to_be_bytes());
    data = replace(&data, *b"head", head);
    data = replace(&data, *b"maxp", e.statistics.unhinted_maxp(count).unwrap());
    let mut hhea = Sfnt::parse(&data, &gl.sfnt)
        .unwrap()
        .table(*b"hhea")
        .unwrap()
        .to_vec();
    hhea[34..36].copy_from_slice(&1u16.to_be_bytes());
    data = replace(&data, *b"hhea", hhea);
    let mut hmtx = vec![0; 2 * (count as usize + 1)];
    hmtx[..2].copy_from_slice(&500u16.to_be_bytes());
    data = replace(&data, *b"hmtx", hmtx);
    let limits = Limits::default();
    let e = woff2::encode(&data, &options(), &limits).unwrap();
    let d = woff2::decode(&e.bytes, &limits).unwrap();
    let f = Sfnt::parse(&d.sfnt, &limits.sfnt).unwrap();
    assert_eq!(&f.table(*b"head").unwrap()[50..52], &1u16.to_be_bytes());
    assert_eq!(
        fontpixel::sfnt::glyf::GlyfTable::from_sfnt(&f, &gl).unwrap(),
        table
    );
}
