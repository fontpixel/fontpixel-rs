//! `Registry::write_owned`: outline writers charge the handed-over model on
//! their own ledger and drop it before encoding; every other writer (and any
//! gzip-named output) has exactly the model's retained usage reserved before
//! the borrowing route. Output and report always equal the borrowing route.
#![cfg(all(
    feature = "pcf",
    feature = "yaff",
    feature = "woff",
    feature = "woff2",
    feature = "ttc"
))]
#[path = "support/variants_fixture.rs"]
#[allow(dead_code)]
mod fixture;
use fontpixel::{
    Error, FontSet, Report, ReportEntry, ResourceLimits, ResourcePath, Result, WriteOptions,
    formats::TransportLimits,
    model::Rational,
    report::{Impact, Level},
    resources::Written,
};

const DEFAULT: usize = 256 * 1024 * 1024;

/// Smallest accepted total; the output there equals the output at `DEFAULT`
/// and one byte less is a `ResourceLimit` error.
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

fn face(seed: u32) -> fontpixel::Font {
    let mut f = fixture::face(&[
        (0x41, "#.\n##", 3),
        (0x42, "###\n#.#\n###", 4),
        (0x4e00 + seed, "#####\n.....\n#####", 6),
        (0x20000, "#.#\n.#.\n#.#", 4),
    ]);
    // Source fields regenerated outlines cannot carry produce loss notes,
    // which the owned route must determine before the face is consumed.
    f.metrics.point_size = Some(Rational::integer(14));
    f
}

/// A model whose clones retain exactly as much as it does (cloning drops
/// spare capacity), so `usage` describes every clone handed over.
fn model(faces: u32) -> FontSet {
    FontSet {
        fonts: (0..faces).map(face).collect(),
        ..FontSet::default()
    }
    .clone()
}

fn options(name: &str, total: usize) -> WriteOptions {
    let mut o = WriteOptions::new(ResourcePath::new(name).unwrap());
    let resources = ResourceLimits {
        max_total_bytes: total,
        ..ResourceLimits::default()
    };
    o.transport_limits = Some(TransportLimits {
        resources,
        max_output_bytes: total,
    });
    o
}

fn usage(set: &FontSet) -> usize {
    set.retained_usage(usize::MAX).unwrap().bytes
}

const OUTLINES: [(&str, &str, u32); 6] = [
    ("ttf", "a.ttf", 1),
    ("otf", "a.otf", 1),
    ("woff", "a.woff", 1),
    ("woff2", "a.woff2", 1),
    ("vector-raster", "a.ttf", 1),
    ("ttc", "a.ttc", 2),
];

#[test]
fn owned_outline_writes_equal_the_borrowing_route() {
    let registry = fontpixel::registry();
    for (id, name, faces) in OUTLINES {
        let set = model(faces);
        let used = usage(&set);
        let owned = registry
            .write_owned(set.clone(), id, &options(name, DEFAULT), Report::default())
            .unwrap();
        let borrowed = registry
            .write_with_report(&set, id, &options(name, DEFAULT - used), Report::default())
            .unwrap();
        assert_eq!(owned, borrowed, "{id}");
        assert!(
            owned
                .report
                .entries
                .iter()
                .any(|e| e.code == "vector-raster.font_metrics"),
            "{id}"
        );
    }
}

#[test]
fn owned_outline_writes_have_exact_thresholds_covering_the_model() {
    let registry = fontpixel::registry();
    for (id, name, faces) in OUTLINES {
        let set = model(faces);
        let used = usage(&set);
        let (owned, written) = minimal(|total| {
            registry.write_owned(set.clone(), id, &options(name, total), Report::default())
        });
        let (borrowed, expected): (usize, Written) = minimal(|total| {
            registry.write_with_report(&set, id, &options(name, total), Report::default())
        });
        assert_eq!(written, expected, "{id}");
        // The model is charged on the same allowance, and it is released
        // before tables and the webfont envelope are encoded, so the owned
        // route never needs the model on top of the borrowing route's peak.
        assert!(owned > used, "{id}: {owned} <= {used}");
        assert!(
            owned <= borrowed + used,
            "{id}: {owned} > {borrowed} + {used}"
        );
        // The envelope encoders dominate these peaks; with the model gone by
        // then, the owned route needs strictly less than model plus peak.
        if ["woff", "woff2", "ttc"].contains(&id) {
            assert!(
                owned < borrowed + used,
                "{id}: {owned} = {borrowed} + {used}"
            );
        }
    }
}

#[test]
fn declined_and_gzip_writes_reserve_exactly_the_model() {
    let registry = fontpixel::registry();
    let set = model(1);
    let used = usage(&set);
    for (id, name) in [("pcf", "a.pcf"), ("ttf", "a.ttf.gz"), ("yaff", "a.yaff.gz")] {
        let (owned, written) = minimal(|total| {
            registry.write_owned(set.clone(), id, &options(name, total), Report::default())
        });
        let (borrowed, expected) = minimal(|total| {
            registry.write_with_report(&set, id, &options(name, total), Report::default())
        });
        assert_eq!(written, expected, "{id} {name}");
        assert_eq!(owned, borrowed + used, "{id} {name}");
    }
}

#[test]
fn incoming_errors_and_strict_loss_precede_owned_accounting() {
    let registry = fontpixel::registry();
    let entry = |level, impact| ReportEntry {
        level,
        impact,
        code: "test.incoming".into(),
        location: Default::default(),
        message: "incoming".into(),
    };
    let mut errors = Report::default();
    errors.push(entry(Level::Error, Impact::Loss));
    assert!(matches!(
        registry.write_owned(model(1), "woff2", &options("a.woff2", 0), errors),
        Err(Error::ReportedErrors(_))
    ));
    let mut loss = Report::default();
    loss.push(entry(Level::Warning, Impact::Loss));
    let mut strict = options("a.woff2", 0);
    strict.strict = true;
    assert!(matches!(
        registry.write_owned(model(1), "woff2", &strict, loss),
        Err(Error::StrictLoss(_))
    ));
    // Without incoming diagnostics a zero allowance is a resource error.
    assert!(matches!(
        registry.write_owned(model(1), "woff2", &options("a.woff2", 0), Report::default()),
        Err(Error::ResourceLimit { .. })
    ));
}
