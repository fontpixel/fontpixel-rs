//! Aggregate returned-asset limits, independent of optional native codec caps.
//! Reconstructed source-only regression coverage; execution is still pending.
use fontpixel::{
    Error, OutputAsset, OutputBundle, Registry, Report, ReportEntry, ResourcePath, WriteOptions,
    formats::{FormatInfo, FormatWriter, Registration, TransportLimits},
    report::{Impact, Level, Location},
    resources::Written,
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const ID: &str = "plain-output-budget-fixture";

struct FixtureWriter {
    directory: bool,
    report: Report,
    calls: Arc<AtomicUsize>,
}
impl FormatWriter<()> for FixtureWriter {
    fn write(&self, _: &(), options: &WriteOptions) -> fontpixel::Result<Written> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let assets = [b"ab".as_slice(), b"cde".as_slice()]
            .into_iter()
            .enumerate()
            .map(|(index, data)| {
                let name = if self.directory {
                    format!("{}/page{index}.bin", options.primary_name)
                } else if index == 0 {
                    options.primary_name.to_string()
                } else {
                    "page.bin".into()
                };
                // Capacity is deliberately larger than serialized length. This
                // cap is for returned wire bytes, not the owned-storage ledger.
                let mut bytes = Vec::with_capacity(data.len() + 64);
                bytes.extend_from_slice(data);
                OutputAsset { path: ResourcePath::new(name).unwrap(), bytes }
            })
            .collect();
        let output = if self.directory {
            OutputBundle::directory(options.primary_name.clone(), assets)?
        } else {
            OutputBundle::new(options.primary_name.clone(), assets)?
        };
        Ok(Written { output, report: self.report.clone() })
    }

    fn transport_limits(&self) -> TransportLimits {
        // This codec's own three-byte asset/envelope bound must not silently
        // become a new aggregate limit for plain five-byte output.
        TransportLimits { max_output_bytes: 3, ..Default::default() }
    }
}
fn registry(directory: bool, report: Report) -> (Registry<()>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = Registry::new();
    registry.register(Registration {
        format: FormatInfo {
            id: ID,
            name: "Synthetic plain output budget fixture",
            extensions: &["fixture"],
            feature_enabled: true,
        },
        detector: None,
        reader: None,
        writer: Some(Box::new(FixtureWriter { directory, report, calls: Arc::clone(&calls) })),
    }).unwrap();
    (registry, calls)
}
fn options(name: &str, maximum: Option<usize>) -> WriteOptions {
    let mut options = WriteOptions::new(ResourcePath::new(name).unwrap());
    options.transport_limits = maximum.map(|max_output_bytes| TransportLimits {
        max_output_bytes,
        ..Default::default()
    });
    options
}
fn diagnostic(level: Level, impact: Impact) -> Report {
    Report { entries: vec![ReportEntry {
        level, impact, code: "fixture.original".into(), location: Location::default(),
        message: "Original diagnostic retains precedence".into(),
    }] }
}
fn assert_output_limit(result: fontpixel::Result<Written>, maximum: usize) {
    match result {
        Err(Error::ResourceLimit { resource, limit }) => {
            assert_eq!(resource, "plain output asset bytes");
            assert_eq!(limit, maximum);
        }
        other => panic!("expected aggregate output limit {maximum}, got {other:?}"),
    }
}

#[test]
fn aggregate_bound_covers_sidecars_and_directory_assets_at_exact_length() {
    for directory in [false, true] {
        let (registry, _) = registry(directory, Report::default());
        let name = if directory { "fontdir" } else { "font.fixture" };
        let original = registry.write(&(), ID, &options(name, None)).unwrap();
        assert_eq!(original.output.assets().len(), 2);
        assert_eq!(original.output.assets().iter().map(|asset| asset.bytes.len()).sum::<usize>(), 5);
        for maximum in [0, 3, 4] {
            assert_output_limit(registry.write(&(), ID, &options(name, Some(maximum))), maximum);
        }
        assert_eq!(registry.write(&(), ID, &options(name, Some(5))).unwrap(), original);
        let mut defaults = options(name, None);
        defaults.transport_limits = Some(TransportLimits::default());
        assert_eq!(registry.write(&(), ID, &defaults).unwrap(), original);
    }
}

#[test]
fn incoming_error_and_strict_loss_precede_writer_and_plain_output_check() {
    let (registry, calls) = registry(false, Report::default());
    let mut options = options("font.fixture", Some(0));
    options.strict = true;
    for (level, impact) in [(Level::Error, Impact::None), (Level::Warning, Impact::Loss)] {
        let report = diagnostic(level, impact);
        let result = registry.write_with_report(&(), ID, &options, report.clone());
        match result {
            Err(Error::ReportedErrors(actual)) if level == Level::Error => assert_eq!(actual, report),
            Err(Error::StrictLoss(actual)) if level == Level::Warning => assert_eq!(actual, report),
            other => panic!("original diagnostic lost precedence: {other:?}"),
        }
    }
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn writer_error_and_strict_loss_precede_plain_output_check() {
    for (level, impact) in [(Level::Error, Impact::None), (Level::Warning, Impact::Loss)] {
        let report = diagnostic(level, impact);
        let (registry, calls) = registry(false, report.clone());
        let mut options = options("font.fixture", Some(0));
        options.strict = true;
        match registry.write(&(), ID, &options) {
            Err(Error::ReportedErrors(actual)) if level == Level::Error => assert_eq!(actual, report),
            Err(Error::StrictLoss(actual)) if level == Level::Warning => assert_eq!(actual, report),
            other => panic!("writer diagnostic lost precedence: {other:?}"),
        }
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}

#[test]
fn non_strict_loss_still_obeys_the_plain_output_cap() {
    let report = diagnostic(Level::Warning, Impact::Loss);
    let (registry, _) = registry(false, report.clone());
    assert_output_limit(registry.write(&(), ID, &options("font.fixture", Some(4))), 4);
    assert_eq!(registry.write(&(), ID, &options("font.fixture", Some(5))).unwrap().report, report);
}

#[cfg(feature = "otb")]
#[test]
fn otb_common_zero_short_exact_and_default_caps() {
    use fontpixel::{Bitmap, Font, FontSet, Glyph, model::{Codepoint, Vector}};
    let mut font = Font::default();
    font.metrics.pixel_size = Some(8);
    font.metrics.ascent = Some(7);
    font.metrics.descent = Some(1);
    let mut glyph = Glyph::new("#".parse::<Bitmap>().unwrap());
    glyph.name = Some("A".into());
    glyph.codepoints.push(Codepoint::Unicode(65));
    glyph.metrics.advance = Some(Vector::new(2, 0));
    font.add_glyph(glyph).unwrap();
    let fonts = FontSet::from(font);
    let registry = fontpixel::registry();
    let original = registry.write(&fonts, "otb", &options("font.otb", None)).unwrap();
    let length = original.output.assets().iter().map(|asset| asset.bytes.len()).sum::<usize>();
    assert!(length > 0);
    for maximum in [0, length - 1] {
        assert_output_limit(registry.write(&fonts, "otb", &options("font.otb", Some(maximum))), maximum);
    }
    assert_eq!(registry.write(&fonts, "otb", &options("font.otb", Some(length))).unwrap(), original);
    let mut defaults = options("font.otb", None);
    defaults.transport_limits = Some(TransportLimits::default());
    assert_eq!(registry.write(&fonts, "otb", &defaults).unwrap(), original);
}
