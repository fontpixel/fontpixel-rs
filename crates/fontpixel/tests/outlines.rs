//! Native contour and centre-fill contract tests.
use fontpixel::{
    Bitmap,
    outline::{self, Limits, Options, Shape},
};
use proptest::prelude::*;
fn area(cs: &[outline::Contour]) -> f64 {
    cs.iter()
        .map(|c| {
            c.iter()
                .zip(c.iter().cycle().skip(1))
                .map(|(a, b)| a.0 * b.1 - a.1 * b.0)
                .sum::<f64>()
        })
        .sum()
}
#[test]
fn exhaustive_three_by_three_geometry() {
    let l = Limits::default();
    for mask in 0u16..512 {
        let mut b = Bitmap::new(3, 3).unwrap();
        for y in 0..3 {
            for x in 0..3 {
                b.set_pixel(x, y, u8::from(mask & (1 << (y * 3 + x)) != 0))
                    .unwrap();
            }
        }
        let o = Options {
            origin: (-2, -3),
            ..Default::default()
        };
        let cs = outline::contours(&b, &o, &l).unwrap();
        assert_eq!(area(&cs), -20000.0 * f64::from(mask.count_ones()));
        assert_eq!(
            outline::rasterize(&cs, 100.0, o.origin, 3, 3, &l).unwrap(),
            b
        );
        for c in cs {
            assert!(c.len() >= 4);
            assert_eq!(
                c[0],
                *c.iter()
                    .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.total_cmp(&b.0)))
                    .unwrap()
            );
            for i in 0..c.len() {
                let a = c[i];
                let b = c[(i + 1) % c.len()];
                let d = c[(i + 2) % c.len()];
                assert!(a.0 == b.0 || a.1 == b.1);
                assert_ne!((a.0 - b.0) * (d.1 - b.1), (a.1 - b.1) * (d.0 - b.0));
            }
        }
    }
}
#[test]
fn holes_corners_separate_and_dots() {
    let l = Limits::default();
    let b: Bitmap = "###\n#.#\n###".parse().unwrap();
    assert_eq!(
        outline::contours(&b, &Options::default(), &l)
            .unwrap()
            .len(),
        2
    );
    let diagonal: Bitmap = "#.\n.#".parse().unwrap();
    assert_eq!(
        outline::contours(&diagonal, &Options::default(), &l)
            .unwrap()
            .len(),
        2
    );
    for shape in [Shape::SquareSeparate, Shape::Dot] {
        let o = Options {
            shape,
            dot_size: 0.9,
            ..Default::default()
        };
        let cs = outline::contours(&b, &o, &l).unwrap();
        assert_eq!(cs.len(), 8);
        assert_eq!(outline::rasterize(&cs, 100.0, (0, 0), 3, 3, &l).unwrap(), b);
    }
    assert_eq!(
        outline::pixel_contour(&Options {
            shape: Shape::Dot,
            dot_size: 0.9,
            ..Default::default()
        })
        .unwrap(),
        vec![
            (95., 50., true),
            (91., 91., false),
            (50., 95., true),
            (9., 91., false),
            (5., 50., true),
            (9., 9., false),
            (50., 5., true),
            (91., 9., false)
        ]
    );
    assert_eq!(
        outline::contours(
            &b,
            &Options {
                shape: Shape::SquareSeparate,
                ..Default::default()
            },
            &l
        )
        .unwrap()
        .len(),
        8
    );
}
#[test]
fn limits_and_invalid_geometry() {
    let b: Bitmap = "##\n##".parse().unwrap();
    assert!(
        outline::contours(
            &b,
            &Options::default(),
            &Limits {
                max_pixels: 3,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        outline::contours(
            &b,
            &Options::default(),
            &Limits {
                max_edges: 1,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(
        outline::contours(
            &b,
            &Options::default(),
            &Limits {
                max_points: 3,
                ..Default::default()
            }
        )
        .is_err()
    );
    for size in [0.0, -1.0, f64::NAN, f64::INFINITY, 1.6] {
        assert!(
            outline::contours(
                &b,
                &Options {
                    dot_size: size,
                    ..Default::default()
                },
                &Limits::default()
            )
            .is_err()
        );
    }
    assert!(
        outline::rasterize(
            &[vec![(f64::NAN, 0., true)]],
            1.,
            (0, 0),
            2,
            2,
            &Limits::default()
        )
        .is_err()
    );
    let cs = outline::contours(&b, &Options::default(), &Limits::default()).unwrap();
    assert!(
        outline::rasterize(
            &cs,
            100.,
            (0, 0),
            2,
            2,
            &Limits {
                max_intersections: 0,
                ..Default::default()
            }
        )
        .is_err()
    );
}
#[test]
fn implicit_quadratics_and_tangent_edges() {
    let c = vec![
        (0., 2., false),
        (2., 2., false),
        (2., 0., false),
        (0., 0., false),
    ];
    assert_eq!(outline::segments(&c).unwrap().len(), 4);
    assert_eq!(
        outline::rasterize(&[c], 1., (0, 0), 2, 2, &Limits::default())
            .unwrap()
            .to_string(),
        "##\n##"
    );
    let c = vec![(0., 0., true), (1., 4., false), (2., 0., true)];
    let image = outline::rasterize(&[c], 1., (0, 0), 2, 3, &Limits::default()).unwrap();
    assert_eq!(image.to_string(), "..\n#.\n##");
}
proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]
    #[test]
    fn random_square_roundtrip(data in prop::collection::vec(any::<u8>(),64),x in -50i32..50,y in -50i32..50,ppu in 1i32..1000){
        let mut b=Bitmap::new(8,8).unwrap();for i in 0..64 {b.set_pixel(i%8,i/8,data[i as usize]&1).unwrap();}
        let o=Options{origin:(x,y),units_per_pixel:ppu,..Default::default()};let l=Limits::default();
        let cs=outline::contours(&b,&o,&l).unwrap();prop_assert_eq!(outline::rasterize(&cs,f64::from(ppu),(x,y),8,8,&l).unwrap(),b);
    }
}
