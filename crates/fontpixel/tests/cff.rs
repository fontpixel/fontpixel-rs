//! CFF1 round trips, numeric boundaries, malformed input and property checks.
#![cfg(feature = "cff")]
use fontpixel::{
    Bitmap,
    outline::{self, Options, Shape},
    sfnt::cff::*,
};
use proptest::prelude::*;
fn empty() -> Font {
    Font {
        postscript_name: "FontPixel-Test".into(),
        full_name: "FontPixel Test".into(),
        units_per_em: 1400,
        glyphs: vec![Glyph {
            name: ".notdef".into(),
            advance: 700.,
            outline: Outline::Path(vec![]),
        }],
        dots: DotOptions::default(),
    }
}
fn font(rows: &[&str], shape: Shape) -> Font {
    let mut bitmap = Bitmap::new(
        rows.first().map_or(0, |r| r.len()) as u32,
        rows.len() as u32,
    )
    .unwrap();
    for (y, r) in rows.iter().enumerate() {
        for (x, b) in r.bytes().enumerate() {
            if b == b'#' {
                bitmap.set_pixel(x as u32, y as u32, 1).unwrap();
            }
        }
    }
    let mut f = empty();
    let o = Options {
        shape,
        dot_size: if shape == Shape::Square { 1. } else { 0.9 },
        origin: (-1, -2),
        ..Default::default()
    };
    f.glyphs
        .push(Glyph::from_bitmap("A", 400., &bitmap, &o, &Limits::default()).unwrap());
    f
}
#[test]
fn square_dot_and_separate_roundtrip() {
    let l = Limits::default();
    for shape in [Shape::Square, Shape::SquareSeparate, Shape::Dot] {
        let f = font(&["###", "#.#", "###"], shape);
        let e = f.encode(&l).unwrap();
        assert_eq!(e, f.encode(&l).unwrap());
        let r = Cff::parse(&e.data, 1400, &l).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r.glyph_name(0), Some(&b".notdef"[..]));
        assert_eq!(r.glyph_name(1), Some(&b"A"[..]));
        assert_eq!(r.global_subr_count(), usize::from(shape == Shape::Dot));
        let glyphs = r.read_all(&l).unwrap();
        assert!(glyphs[0].commands.is_empty());
        assert_eq!(glyphs[0].advance, 700.);
        assert_eq!(glyphs[1].advance, 400.);
        if shape == Shape::Dot {
            assert_eq!(
                glyphs[1]
                    .commands
                    .iter()
                    .filter(|c| matches!(c, Command::CurveTo(..)))
                    .count(),
                32
            );
        } else {
            let mut cs = Vec::new();
            let mut c = Vec::new();
            for cmd in &glyphs[1].commands {
                match cmd {
                    Command::MoveTo(p) | Command::LineTo(p) => c.push((p.0, p.1, true)),
                    Command::Close => cs.push(std::mem::take(&mut c)),
                    _ => panic!(),
                }
            }
            let b = outline::rasterize(&cs, 100., (-1, -2), 3, 3, &l.geometry).unwrap();
            assert_eq!(
                (0..3)
                    .flat_map(|y| (0..3).map(move |x| (x, y)))
                    .filter(|&(x, y)| b.get_pixel(x, y).unwrap() != 0)
                    .count(),
                8
            );
        }
    }
}
#[test]
fn alternating_axis_runs_split_and_reset_without_geometry_changes() {
    // Cross the 40-operand packing boundary, then break runs with every other
    // command kind. Consecutive same-axis and zero-length lines must survive.
    for vertical_first in [false, true] {
        let mut path = vec![Command::MoveTo((0., 0.))];
        let mut p = (0., 0.);
        for index in 0..97 {
            if (index % 2 == 0) == vertical_first {
                p.1 += 1.;
            } else {
                p.0 += 1.;
            }
            path.push(Command::LineTo(p));
        }
        path.push(Command::LineTo(p));
        p.0 += 2.;
        path.push(Command::LineTo(p));
        p.0 += 3.;
        path.push(Command::LineTo(p));
        p = (p.0 + 4., p.1 + 5.);
        path.push(Command::LineTo(p));
        path.push(Command::CurveTo((60., 60.), (70., 70.), (80., 80.)));
        path.push(Command::LineTo((80., 81.)));
        path.push(Command::Close);
        path.push(Command::MoveTo((-5., -5.)));
        path.push(Command::LineTo((-4., -5.)));
        path.push(Command::LineTo((-4., -4.)));
        path.push(Command::Close);
        let mut f = empty();
        f.glyphs[0].outline = Outline::Path(path.clone());
        let limits = Limits::default();
        let encoded = f.encode(&limits).unwrap();
        let parsed = Cff::parse(&encoded.data, f.units_per_em, &limits).unwrap();
        assert_eq!(parsed.glyph(0, &limits).unwrap().commands, path);
        assert_eq!(parsed.glyph(0, &limits).unwrap().advance, 700.);
    }
}
#[test]
fn all_3x3_square_masks() {
    let l = Limits::default();
    for mask in 0..512 {
        let rows = (0..3)
            .map(|y| {
                (0..3)
                    .map(|x| {
                        if mask & (1 << (y * 3 + x)) != 0 {
                            '#'
                        } else {
                            '.'
                        }
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>();
        let f = font(
            &rows.iter().map(String::as_str).collect::<Vec<_>>(),
            Shape::Square,
        );
        let e = f.encode(&l).unwrap();
        let r = Cff::parse(&e.data, 1400, &l).unwrap();
        assert_eq!(r.glyph(1, &l).unwrap().advance, 400.);
    }
}
#[test]
fn cubic_bounds_and_fractional_advance() {
    let mut f = empty();
    f.units_per_em = 2048;
    f.glyphs[0].advance = 250.25;
    f.glyphs[0].outline = Outline::Path(vec![
        Command::MoveTo((0., 0.)),
        Command::CurveTo((100., 100.), (200., 100.), (300., 0.)),
        Command::Close,
    ]);
    let e = f.encode(&Limits::default()).unwrap();
    assert_eq!(e.bounds, [0, 0, 300, 75]);
    let r = Cff::parse(&e.data, 2048, &Limits::default()).unwrap();
    let g = r.glyph(0, &Limits::default()).unwrap();
    assert_eq!(g.advance, 250.25);
    assert_eq!(
        g.commands,
        match f.glyphs[0].outline.clone() {
            Outline::Path(p) => p,
            _ => unreachable!(),
        }
    );
}
#[test]
fn reject_numeric_names_and_unclosed_paths() {
    let l = Limits::default();
    for v in [f64::NAN, f64::INFINITY, -1., 32768., 65535.] {
        let mut f = empty();
        f.glyphs[0].advance = v;
        assert!(f.encode(&l).is_err());
    }
    let mut f = empty();
    f.glyphs[0].outline = Outline::Path(vec![Command::MoveTo((0., 0.))]);
    assert!(f.encode(&l).is_err());
    for n in ["", "bad name", "bad/name"] {
        let mut f = empty();
        f.postscript_name = n.into();
        assert!(f.encode(&l).is_err());
    }
    let mut f = empty();
    f.glyphs.push(f.glyphs[0].clone());
    assert!(f.encode(&l).is_err());
    let mut f = empty();
    f.glyphs[0].outline = Outline::Path(vec![
        Command::MoveTo((-32768., 0.)),
        Command::LineTo((32767., 0.)),
        Command::Close,
    ]);
    assert!(f.encode(&l).is_err());
}
#[test]
fn truncation_and_mutation_are_bounded() {
    let l = Limits::default();
    let e = font(&["##", "##"], Shape::Dot).encode(&l).unwrap();
    for end in 0..e.data.len() {
        if let Ok(r) = Cff::parse(&e.data[..end], 1400, &l) {
            assert!(r.read_all(&l).is_err(), "truncation accepted at {end}");
        }
    }
    for i in 0..e.data.len() {
        let mut d = e.data.clone();
        d[i] ^= 255;
        if let Ok(r) = Cff::parse(&d, 1400, &l) {
            let _ = r.read_all(&l);
        }
    }
}
#[test]
fn enforce_resource_limits_before_expansion() {
    let f = font(&["###", "###"], Shape::Dot);
    let l = Limits::default();
    let e = f.encode(&l).unwrap();
    let r = Cff::parse(&e.data, 1400, &l).unwrap();
    for tiny in [
        Limits {
            max_commands: 2,
            ..l.clone()
        },
        Limits {
            max_program_bytes: 4,
            ..l.clone()
        },
        Limits {
            max_total_commands: 2,
            ..l.clone()
        },
        Limits {
            max_total_program_bytes: 4,
            ..l.clone()
        },
    ] {
        assert!(r.read_all(&tiny).is_err());
        assert!(f.encode(&tiny).is_err());
    }
    let mut tiny = l.clone();
    tiny.sfnt.resources.max_glyphs = 1;
    assert!(Cff::parse(&e.data, 1400, &tiny).is_err());
    tiny = l.clone();
    tiny.max_dict_bytes = 1;
    assert!(Cff::parse(&e.data, 1400, &tiny).is_err());
}
proptest! {
    #[test]
    fn random_square_roundtrip(mask in any::<u64>()){
        let rows=(0..8).map(|y|(0..8).map(|x|if mask&(1<<(y*8+x))!=0{'#'}else{'.'}).collect::<String>()).collect::<Vec<_>>();
        let f=font(&rows.iter().map(String::as_str).collect::<Vec<_>>(),Shape::Square);let l=Limits::default();let e=f.encode(&l).unwrap();let r=Cff::parse(&e.data,1400,&l).unwrap();let mut g=r.read_all(&l).unwrap();
        let mut f2=f.clone();for(i,d)in g.drain(..).enumerate(){f2.glyphs[i].outline=Outline::Path(d.commands);}
        prop_assert_eq!(e,f2.encode(&l).unwrap());
    }
    #[test]
    fn arbitrary_input_never_panics(data in proptest::collection::vec(any::<u8>(),0..2048)){
        let l=Limits{max_commands:128,max_total_commands:1024,max_program_bytes:4096,max_total_program_bytes:16384,..Default::default()};
        if let Ok(r)=Cff::parse(&data,1000,&l){let _=r.read_all(&l);}
    }
}

#[test]
fn independent_fonttools_program_fixtures() {
    let fixtures: [&[u8]; 6] = [
        include_bytes!("../../../tests/data/cff/dot-holes-offsets-native.cff"),
        include_bytes!("../../../tests/data/cff/separate-holes-offsets-native.cff"),
        include_bytes!("../../../tests/data/cff/external-operators.cff"),
        include_bytes!("../../../tests/data/cff/external-cid.cff"),
        include_bytes!("../../../tests/data/cff/external-cid-format0.cff"),
        include_bytes!("../../../tests/data/cff/external-fontmatrix.cff"),
    ];
    let l = Limits::default();
    for (i, data) in fixtures.into_iter().enumerate() {
        let f = Cff::parse(data, 1400, &l).unwrap();
        assert_eq!(f.len(), 3);
        let glyphs = f.read_all(&l).unwrap();
        assert_eq!(glyphs[0].advance, if i == 5 { 1400. } else { 700. });
        assert_eq!(f.is_cid(), matches!(i, 3 | 4));
        assert!(!glyphs[1].commands.is_empty());
    }
}

#[test]
fn sfnt_binding_requires_otto_head_and_matching_maxp() {
    use fontpixel::sfnt::{Flavor, Sfnt, SfntBuilder};
    let l = Limits::default();
    let e = empty().encode(&l).unwrap();
    for (flavor, upem, maxp, valid) in [
        (Flavor::Cff, 1400, e.maxp.clone(), true),
        (Flavor::TrueType, 1400, e.maxp.clone(), false),
        (Flavor::Cff, 0, e.maxp.clone(), false),
        (Flavor::Cff, 1400, vec![0, 0, 0x50, 0, 0, 2], false),
        (Flavor::Cff, 1400, vec![0, 1, 0, 0, 0, 1], false),
    ] {
        let mut head = vec![0; 54];
        head[18..20].copy_from_slice(&(upem as u16).to_be_bytes());
        let mut builder = SfntBuilder::new(flavor);
        builder.insert(*b"head", head).unwrap();
        builder.insert(*b"maxp", maxp).unwrap();
        builder.insert(*b"CFF ", e.data.clone()).unwrap();
        let data = builder.build(&l.sfnt).unwrap();
        let container = Sfnt::parse(&data, &l.sfnt).unwrap();
        assert_eq!(Cff::from_sfnt(&container, &l).is_ok(), valid);
    }
}
