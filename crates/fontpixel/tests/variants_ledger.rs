//! Exact-capacity and one-byte-short checks for the variants working ledger.
//!
//! Planning and export admit every owned allocation on one ledger whose total
//! is `limits.sfnt.resources.max_total_bytes`. For each configuration the
//! smallest accepted total produces the default output byte for byte, and one
//! byte less is a `ResourceLimit` error.
#![cfg(all(feature = "sfnt", feature = "cff"))]
#[path = "support/variants_fixture.rs"]
mod fixture;
use fontpixel::{
    Error, Result,
    outline::Shape,
    sfnt::gsub::LayoutLimits,
    variants::{self, DotReuse, Options, OutlineFlavor},
};

const DEFAULT: usize = 256 * 1024 * 1024;

/// One export configuration applied to default options.
type Edit = Box<dyn Fn(&mut Options)>;

/// Smallest accepted total. `run` must succeed at the default total.
fn minimal<T: PartialEq + std::fmt::Debug>(run: impl Fn(usize) -> Result<T>) -> (usize, T) {
    let expected = run(DEFAULT).unwrap();
    let (mut lo, mut hi) = (0usize, DEFAULT);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if run(mid).is_ok() {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let at = run(hi).unwrap();
    assert_eq!(at, expected, "minimal total changed the output");
    match run(hi - 1) {
        Err(Error::ResourceLimit { .. }) => {}
        other => panic!("one byte short must be a resource error: {other:?}"),
    }
    (hi, at)
}

fn options(total: usize) -> Options {
    let mut o = Options::default();
    o.limits.sfnt.resources.max_total_bytes = total;
    o
}

fn layout(total: usize) -> LayoutLimits {
    options(total).limits
}

#[test]
fn planning_has_an_exact_ledger_threshold() {
    let v = fixture::variants();
    let (total, plan) = minimal(|total| {
        let plan = v.plan(&layout(total))?;
        Ok((plan.glyphs, plan.cmap, plan.gsub))
    });
    assert!(total > 0);
    // Alternates deduplicate: equal TC/HK faces share glyphs, JP reuses TC's B.
    let names = plan.0.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "u20000",
            "uni0041",
            "uni0041.latin",
            "uni0042",
            "uni0042.tc",
            "uni4E00",
            "uni4E00.jp",
            "uni4E01"
        ]
    );
}

#[test]
fn every_export_configuration_has_an_exact_ledger_threshold() {
    let v = fixture::variants();
    let cases: Vec<(&str, Edit)> = vec![
        ("square ttf", Box::new(|_| {})),
        ("square otf", Box::new(|o| o.flavor = OutlineFlavor::Cff)),
        (
            "separate ttf",
            Box::new(|o| {
                o.shape = Shape::SquareSeparate;
                o.dot_size = 0.8;
            }),
        ),
        (
            "dot literal",
            Box::new(|o| {
                o.shape = Shape::Dot;
                o.dot_size = 0.9;
                o.dot_reuse = DotReuse::Literal;
            }),
        ),
        (
            "dot rows",
            Box::new(|o| {
                o.shape = Shape::Dot;
                o.dot_size = 0.9;
                o.dot_reuse = DotReuse::Rows;
            }),
        ),
        (
            "dot patterns",
            Box::new(|o| {
                o.shape = Shape::Dot;
                o.dot_size = 0.9;
            }),
        ),
        (
            "dot otf",
            Box::new(|o| {
                o.shape = Shape::Dot;
                o.dot_size = 0.9;
                o.flavor = OutlineFlavor::Cff;
            }),
        ),
        ("embedded ttf", Box::new(|o| o.embed_bitmaps = true)),
        (
            "embedded otf",
            Box::new(|o| {
                o.embed_bitmaps = true;
                o.flavor = OutlineFlavor::Cff;
            }),
        ),
    ];
    for (name, edit) in &cases {
        let (total, bytes) = minimal(|total| {
            let mut o = options(total);
            edit(&mut o);
            variants::encode(&v, &o).map(|e| e.bytes)
        });
        // The plan alone must fit; export then needs more.
        let plan = minimal(|total| v.plan(&layout(total)).map(|p| p.glyphs.len())).0;
        assert!(total > plan, "{name}: {total} <= {plan}");
        assert!(!bytes.is_empty(), "{name}");
    }
}

#[test]
fn prepared_encodes_charge_the_shared_plan_once_per_call() {
    let v = fixture::variants();
    let direct = minimal(|total| variants::encode(&v, &options(total)).map(|e| e.bytes));
    let (total, bytes) = minimal(|total| {
        let prepared = v.prepare(&layout(total))?;
        let first = prepared.encode(&options(total))?.bytes;
        // A second call has its own ledger and the same allowance.
        assert_eq!(prepared.encode(&options(total))?.bytes, first);
        assert_eq!(
            prepared.encode_allowance(),
            total - prepared.retained_bytes()
        );
        Ok(first)
    });
    assert_eq!(bytes, direct.1);
    let prepared = v.prepare(&layout(DEFAULT)).unwrap();
    assert!(prepared.retained_bytes() > 0);
    assert!(prepared.retained_bytes() < total);
    // Concurrent calls each admit their own allowance; none sees the other's.
    let tight = v.prepare(&layout(total)).unwrap();
    std::thread::scope(|scope| {
        let a = scope.spawn(|| tight.encode(&options(total)).map(|e| e.bytes));
        let b = scope.spawn(|| tight.encode(&options(total)).map(|e| e.bytes));
        assert_eq!(a.join().unwrap().unwrap(), bytes);
        assert_eq!(b.join().unwrap().unwrap(), bytes);
    });
}

#[test]
fn single_font_export_has_an_exact_ledger_threshold() {
    let font = fixture::single();
    for (shape, dot_size, flavor) in [
        (Shape::Square, 1., OutlineFlavor::TrueType),
        (Shape::Dot, 0.9, OutlineFlavor::TrueType),
        (Shape::Square, 1., OutlineFlavor::Cff),
    ] {
        let (_, (bytes, report)) = minimal(|total| {
            let mut o = options(total);
            o.shape = shape;
            o.dot_size = dot_size;
            o.flavor = flavor;
            o.gray_threshold = Some(128);
            variants::encode_single(&font, &o).map(|e| (e.bytes, e.report))
        });
        assert!(!bytes.is_empty());
        // Loss diagnostics are retained under the minimal ledger.
        let codes = report
            .entries
            .iter()
            .map(|e| e.code.as_str())
            .collect::<Vec<_>>();
        assert!(codes.contains(&"variants.gray_threshold"), "{codes:?}");
        assert!(codes.contains(&"opentype.glyph_name"), "{codes:?}");
    }
}

#[test]
fn ledger_exhaustion_is_a_resource_error_for_every_entry_point() {
    let v = fixture::variants();
    let font = fixture::single();
    let mut o = options(4096);
    o.gray_threshold = Some(128);
    for result in [
        v.plan(&o.limits).map(|_| ()),
        variants::encode(&v, &o).map(|_| ()),
        variants::encode_single(&font, &o).map(|_| ()),
        v.prepare(&o.limits).map(|_| ()),
    ] {
        assert!(
            matches!(result, Err(Error::ResourceLimit { .. })),
            "{result:?}"
        );
    }
}
