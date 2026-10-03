//! Native glyf/loca robustness and semantic roundtrips.
#![cfg(feature = "sfnt")]
use fontpixel::{
    outline::{self, Options, Shape},
    sfnt::{
        Sfnt,
        glyf::{
            Anchor, Component, CompositeGlyph, GlyfTable, Glyph, Limits, LocaFormat, OffsetMode,
            SimpleGlyph,
        },
    },
};
use proptest::prelude::*;
fn square() -> Glyph {
    Glyph::simple(vec![vec![
        (0., 0., true),
        (0., 100., true),
        (100., 100., true),
        (100., 0., true),
    ]])
}
fn component(c: Component) -> Glyph {
    Glyph::Composite(CompositeGlyph {
        components: vec![c],
        instructions: Vec::new(),
    })
}
#[test]
fn simple_compound_and_both_loca_roundtrip() {
    let l = Limits::default();
    let table = GlyfTable {
        glyphs: vec![
            Glyph::Empty,
            square(),
            component(Component::translated(1, 125, -200)),
        ],
    };
    for format in [LocaFormat::Auto, LocaFormat::Short, LocaFormat::Long] {
        let e = table.encode(format, &l).unwrap();
        let d = GlyfTable::decode(&e.glyf, &e.loca, 3, e.index_to_loc_format, &l).unwrap();
        assert_eq!(d, table);
        assert_eq!(d.encode(format, &l).unwrap(), e);
        assert_eq!(e.bounds, [0, -200, 225, 100]);
        assert_eq!(e.statistics.max_component_depth, 1);
        assert_eq!(e.statistics.max_composite_points, 4);
        assert_eq!(e.statistics.unhinted_maxp(3).unwrap().len(), 32);
    }
}
#[test]
fn transforms_point_attachment_nested_and_flags() {
    let l = Limits::default();
    let mut scaled = Component::translated(1, 100, -100);
    scaled.transform = [8192, 4096, -4096, 16384];
    scaled.offset_mode = OffsetMode::Scaled;
    scaled.round_to_grid = true;
    scaled.use_my_metrics = true;
    scaled.overlap = true;
    let mut attached = Component::translated(1, 0, 0);
    attached.anchor = Anchor::Point {
        parent: 2,
        child: 0,
    };
    attached.non_overlapping = true;
    attached.offset_mode = OffsetMode::Default;
    let table = GlyfTable {
        glyphs: vec![
            Glyph::Empty,
            square(),
            Glyph::Composite(CompositeGlyph {
                components: vec![scaled.clone(), attached],
                instructions: vec![0x00, 0x01],
            }),
            component(Component::translated(2, -50, 30)),
        ],
    };
    let e = table.encode(LocaFormat::Long, &l).unwrap();
    assert_eq!(
        GlyfTable::decode(&e.glyf, &e.loca, 4, 1, &l).unwrap(),
        table
    );
    let cs = table.contours(2, &l).unwrap();
    assert_eq!(cs[0][0], (75., -75., true));
    assert_eq!(cs[0][2], cs[1][0]);
    assert!(e.statistics.unhinted_maxp(4).is_err());
    assert_eq!(e.statistics.max_component_depth, 2);
    scaled.offset_mode = OffsetMode::Unscaled;
    let table = GlyfTable {
        glyphs: vec![Glyph::Empty, square(), component(scaled)],
    };
    assert_eq!(table.contours(2, &l).unwrap()[0][0], (100., -100., true));
}
#[test]
fn circles_share_one_glyph() {
    let l = Limits::default();
    let b = "#.#\n.##".parse().unwrap();
    let o = Options {
        shape: Shape::Dot,
        dot_size: 0.9,
        origin: (-1, -2),
        ..Default::default()
    };
    let table = GlyfTable {
        glyphs: vec![
            Glyph::simple(vec![outline::pixel_contour(&o).unwrap()]),
            Glyph::pixels(&b, &o, 0, &l).unwrap(),
        ],
    };
    let cs = table.contours(1, &l).unwrap();
    assert_eq!(cs, outline::contours(&b, &o, &l.geometry).unwrap());
    let e = table.encode(LocaFormat::Auto, &l).unwrap();
    assert_eq!(e.statistics.max_component_elements, 4);
    assert_eq!(e.statistics.max_composite_points, 32);
    assert_eq!(
        GlyfTable::decode(&e.glyf, &e.loca, 2, e.index_to_loc_format, &l).unwrap(),
        table
    );
}
#[test]
fn instructions_overlap_empty_contours_and_zero_records() {
    let l = Limits::default();
    let mut g = if let Glyph::Simple(g) = square() {
        g
    } else {
        unreachable!()
    };
    g.instructions = vec![0x31, 0x22];
    g.overlap = true;
    let t = GlyfTable {
        glyphs: vec![
            Glyph::Simple(g),
            Glyph::Simple(SimpleGlyph {
                instructions: vec![0x31],
                ..Default::default()
            }),
        ],
    };
    let e = t.encode(LocaFormat::Short, &l).unwrap();
    assert_eq!(GlyfTable::decode(&e.glyf, &e.loca, 2, 0, &l).unwrap(), t);
    let empty_header = [0u8; 10];
    assert_eq!(
        GlyfTable::decode(&empty_header, &[0, 0, 0, 5], 1, 0, &l)
            .unwrap()
            .glyphs,
        vec![Glyph::Simple(SimpleGlyph::default())]
    );
}
#[test]
fn malformed_slices_flags_endpoints_and_loca() {
    let l = Limits::default();
    let t = GlyfTable {
        glyphs: vec![square()],
    };
    let e = t.encode(LocaFormat::Long, &l).unwrap();
    for n in 0..e.glyf.len() - 1 {
        let mut loca = vec![0, 0, 0, 0];
        loca.extend((n as u32).to_be_bytes());
        if n != 0 {
            assert!(
                GlyfTable::decode(&e.glyf, &loca, 1, 1, &l).is_err(),
                "truncated at {n}"
            );
        }
    }
    assert!(GlyfTable::decode(&[], &[0, 1, 0, 0], 1, 0, &l).is_err());
    assert!(GlyfTable::decode(&[], &[], 1, 3, &l).is_err());
    let mut g = e.glyf.clone();
    g[14] = 0xff;
    assert!(GlyfTable::decode(&g, &e.loca, 1, 1, &l).is_err());
    // A header with two endpoints in descending order.
    let mut g = vec![0u8; 20];
    g[1] = 2;
    g[11] = 2;
    g[13] = 1;
    assert!(GlyfTable::decode(&g, &[0, 0, 0, 10], 1, 0, &l).is_err());
    // Flag repetition cannot cross the declared point count.
    let mut g = vec![0u8; 16];
    g[1] = 1;
    g[14] = 8;
    g[15] = 255;
    assert!(GlyfTable::decode(&g, &[0, 0, 0, 8], 1, 0, &l).is_err());
    // Coordinate accumulation must not wrap at int16.
    let mut g = vec![0u8; 14];
    g[1] = 1;
    g[11] = 1;
    g.extend([0x21, 0x21, 0x7f, 0xff, 0, 1]);
    assert!(GlyfTable::decode(&g, &[0, 0, 0, 10], 1, 0, &l).is_err());
}
#[test]
fn cycles_depth_visits_and_budget_enforcement() {
    let l = Limits::default();
    for glyphs in [
        vec![component(Component::translated(0, 0, 0))],
        vec![component(Component::translated(1, 0, 0))],
        vec![
            component(Component::translated(1, 0, 0)),
            component(Component::translated(0, 0, 0)),
        ],
    ] {
        let t = GlyfTable { glyphs };
        assert!(t.encode(LocaFormat::Auto, &l).is_err());
        assert!(t.contours(0, &l).is_err());
    }
    let mut t = GlyfTable {
        glyphs: vec![Glyph::Empty],
    };
    for i in 0..12 {
        t.glyphs.push(Glyph::Composite(CompositeGlyph {
            components: vec![Component::translated(i, 0, 0); 2],
            instructions: Vec::new(),
        }));
    }
    assert!(
        t.contours(
            12,
            &Limits {
                max_visits: 100,
                ..l.clone()
            }
        )
        .is_err()
    );
    assert!(
        t.encode(
            LocaFormat::Auto,
            &Limits {
                max_depth: 5,
                ..l.clone()
            }
        )
        .is_err()
    );
    assert!(
        GlyfTable {
            glyphs: vec![square()]
        }
        .encode(
            LocaFormat::Auto,
            &Limits {
                max_total_points: 3,
                ..l
            }
        )
        .is_err()
    );
}
#[test]
fn all_existing_spec_fonts_decode_reencode() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/data/compat");
    let l = Limits::default();
    let mut count = 0;
    for p in std::fs::read_dir(root).unwrap() {
        let p = p.unwrap().path();
        if p.extension().is_none_or(|e| e != "ttf") {
            continue;
        }
        count += 1;
        let bytes = std::fs::read(p).unwrap();
        let font = Sfnt::parse(&bytes, &l.sfnt).unwrap();
        let t = GlyfTable::from_sfnt(&font, &l).unwrap();
        let e = t.encode(LocaFormat::Auto, &l).unwrap();
        assert_eq!(
            GlyfTable::decode(
                &e.glyf,
                &e.loca,
                t.glyphs.len() as u16,
                e.index_to_loc_format,
                &l
            )
            .unwrap(),
            t
        );
    }
    assert!(count >= 3);
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn arbitrary_glyph_bytes_do_not_panic(data in prop::collection::vec(any::<u8>(),0..1024),short in any::<bool>()){
        let mut loca=vec![0; if short{2}else{4}];let mut data=data;if short{if data.len()%2!=0{data.push(0);}loca.extend(((data.len()/2) as u16).to_be_bytes());}else{loca.extend((data.len() as u32).to_be_bytes());}
        let l=Limits::default();if let Ok(t)=GlyfTable::decode(&data,&loca,1,if short{0}else{1},&l) && let Ok(e)=t.encode(LocaFormat::Auto,&l){prop_assert_eq!(GlyfTable::decode(&e.glyf,&e.loca,1,e.index_to_loc_format,&l).unwrap(),t);}
    }
    #[test]
    fn random_bitmap_glyph_roundtrip(data in prop::collection::vec(any::<u8>(),64),ppu in 2i32..200){let mut b=fontpixel::Bitmap::new(8,8).unwrap();for i in 0..64 {b.set_pixel(i%8,i/8,data[i as usize]&1).unwrap();}
        let l=Limits::default();let cs=outline::contours(&b,&Options{units_per_pixel:ppu,..Default::default()},&l.geometry).unwrap();let t=GlyfTable{glyphs:vec![Glyph::simple(cs)]};let e=t.encode(LocaFormat::Auto,&l).unwrap();prop_assert_eq!(GlyfTable::decode(&e.glyf,&e.loca,1,e.index_to_loc_format,&l).unwrap(),t);
    }
}

#[test]
fn long_loca_selection_and_explicit_short_overflow() {
    let table = GlyfTable {
        glyphs: vec![square(); 7000],
    };
    let l = Limits::default();
    let e = table.encode(LocaFormat::Auto, &l).unwrap();
    assert_eq!(e.index_to_loc_format, 1);
    assert!(table.encode(LocaFormat::Short, &l).is_err());
    assert_eq!(
        GlyfTable::decode(&e.glyf, &e.loca, 7000, 1, &l).unwrap(),
        table
    );
}

#[test]
fn truncated_composite_instructions_and_invalid_flags_fail() {
    let l = Limits::default();
    let t = GlyfTable {
        glyphs: vec![
            square(),
            Glyph::Composite(CompositeGlyph {
                components: vec![Component::translated(0, 1, 2)],
                instructions: vec![0x31, 0x31],
            }),
        ],
    };
    let e = t.encode(LocaFormat::Long, &l).unwrap();
    let off = u32::from_be_bytes(e.loca[4..8].try_into().unwrap()) as usize;
    let mut g = e.glyf.clone();
    // Both offset modes are mutually exclusive.
    g[off + 10] |= 0x18;
    assert!(GlyfTable::decode(&g, &e.loca, 2, 1, &l).is_err());
    let mut truncated = e.loca.clone();
    truncated[8..12].copy_from_slice(&((e.glyf.len() - 1) as u32).to_be_bytes());
    assert!(GlyfTable::decode(&e.glyf, &truncated, 2, 1, &l).is_err());
    assert!(
        GlyfTable::decode(
            &e.glyf,
            &e.loca,
            2,
            1,
            &Limits {
                max_total_instructions: 1,
                ..l
            }
        )
        .is_err()
    );
}
