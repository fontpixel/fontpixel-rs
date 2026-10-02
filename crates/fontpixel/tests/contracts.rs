//! Infrastructure contracts using an explicitly synthetic non-font codec.
use fontpixel::{
    Error, Input, Operation, OutputAsset, OutputBundle, ReadOptions, Registry, Report, ReportEntry,
    ResourceLimits, ResourcePath, ResourceResolver, WriteOptions,
    formats::{Confidence, FormatDetector, FormatInfo, FormatReader, FormatWriter, Registration},
    report::{Impact, Level, Location},
    resources::{ReadContext, Written},
};
use std::collections::BTreeMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

const FIXTURE: FormatInfo = FormatInfo {
    id: "test-fixture",
    name: "Synthetic infrastructure fixture (not a font)",
    extensions: &["fixture"],
    feature_enabled: true,
};
const OTHER: FormatInfo = FormatInfo {
    id: "test-other",
    name: "Second synthetic fixture",
    extensions: &["fixture"],
    feature_enabled: true,
};

fn path(value: &str) -> ResourcePath {
    ResourcePath::new(value).unwrap()
}
fn loss() -> Report {
    Report {
        entries: vec![ReportEntry {
            level: Level::Info,
            impact: Impact::Loss,
            code: "test.loss".into(),
            location: Location {
                glyph_index: Some(2),
                codepoint: Some(0x1f600),
                ..Location::default()
            },
            message: "Synthetic information loss".into(),
        }],
    }
}

struct FixedDetector(Confidence);
impl FormatDetector for FixedDetector {
    fn detect(&self, _: &mut ReadContext<'_>) -> fontpixel::Result<Confidence> {
        Ok(self.0)
    }
}
struct TestCodec;
impl FormatDetector for TestCodec {
    fn detect(&self, input: &mut ReadContext<'_>) -> fontpixel::Result<Confidence> {
        Ok(if input.bytes() == b"FPTEST\0" {
            Confidence::Certain
        } else {
            Confidence::NoMatch
        })
    }
}
impl FormatReader<Vec<u8>> for TestCodec {
    fn read(
        &self,
        input: &mut ReadContext<'_>,
        _: &ReadOptions,
    ) -> fontpixel::Result<(Vec<u8>, Report)> {
        if input.bytes() != b"FPTEST\0" {
            return Err(Error::InvalidInput("not the synthetic test magic".into()));
        }
        Ok((
            input.resource(&path("pages/payload.bin"))?,
            Report::default(),
        ))
    }
}
impl FormatWriter<Vec<u8>> for TestCodec {
    fn write(&self, model: &Vec<u8>, options: &WriteOptions) -> fontpixel::Result<Written> {
        Ok(Written {
            output: OutputBundle::new(
                options.primary_name.clone(),
                vec![
                    OutputAsset {
                        path: path("pages/payload.bin"),
                        bytes: model.clone(),
                    },
                    OutputAsset {
                        path: options.primary_name.clone(),
                        bytes: b"FPTEST\0".to_vec(),
                    },
                ],
            )?,
            report: Report::default(),
        })
    }
}
struct MapResolver(BTreeMap<String, Vec<u8>>);
impl ResourceResolver for MapResolver {
    fn resolve(&self, path: &ResourcePath, max_bytes: usize) -> fontpixel::Result<Vec<u8>> {
        let bytes = self
            .0
            .get(path.as_str())
            .ok_or_else(|| Error::MissingResource(path.to_string()))?;
        if bytes.len() > max_bytes {
            return Err(Error::ResourceLimit {
                resource: "fixture sidecar",
                limit: max_bytes,
            });
        }
        Ok(bytes.clone())
    }
}
fn registry() -> Registry<Vec<u8>> {
    let mut registry = Registry::new();
    registry
        .register(Registration {
            format: FIXTURE,
            detector: Some(Box::new(TestCodec)),
            reader: Some(Box::new(TestCodec)),
            writer: Some(Box::new(TestCodec)),
        })
        .unwrap();
    registry
}
fn detected_registry(a: Confidence, b: Confidence) -> Registry<()> {
    let mut registry = Registry::new();
    for (format, confidence) in [(FIXTURE, a), (OTHER, b)] {
        registry
            .register(Registration {
                format,
                detector: Some(Box::new(FixedDetector(confidence))),
                reader: None,
                writer: None,
            })
            .unwrap();
    }
    registry
}

#[test]
fn catalogue_exactly_matches_specification_ids_including_pk_and_gf() {
    let source = include_str!("../../../FORMATS.md");
    let mut expected = Vec::new();
    for line in source.lines().filter(|line| line.starts_with("| `")) {
        let first_column = line.split('|').nth(1).unwrap();
        for (index, token) in first_column.split('`').enumerate() {
            if index % 2 == 1 {
                expected.push(token);
            }
        }
    }
    assert_eq!(expected.len(), 46);
    let registry = Registry::<()>::new();
    let actual: Vec<_> = registry.formats().map(|entry| entry.format.id).collect();
    assert_eq!(actual, expected);
    let unique: std::collections::BTreeSet<_> = actual.iter().collect();
    assert_eq!(unique.len(), 46);
    assert!(
        registry
            .formats()
            .all(|status| !status.read && !status.write && !status.detect)
    );
}

#[test]
fn feature_state_matches_the_compiled_build() {
    let registry = Registry::<()>::new();
    assert_eq!(
        registry.status("bdf").unwrap().format.feature_enabled,
        cfg!(feature = "bdf")
    );
    assert_eq!(
        registry.status("pcf").unwrap().format.feature_enabled,
        cfg!(feature = "pcf")
    );
    assert_eq!(
        registry.status("woff2").unwrap().format.feature_enabled,
        cfg!(feature = "woff2")
    );
}

#[test]
fn unimplemented_is_not_a_successful_empty_font() {
    let registry = Registry::<()>::new();
    let options = ReadOptions {
        format: Some("bdf".into()),
        ..ReadOptions::default()
    };
    assert!(matches!(
        registry.read(&Input::new(b"STARTFONT 2.1"), &options),
        Err(Error::NotImplemented {
            operation: Operation::Read,
            ..
        })
    ));
    assert!(matches!(
        registry.write(&(), "bdf", &WriteOptions::new(path("out.bdf"))),
        Err(Error::NotImplemented {
            operation: Operation::Write,
            ..
        })
    ));
    assert!(matches!(
        registry.status("missing"),
        Err(Error::UnknownFormat(_))
    ));
}

#[test]
fn filename_is_only_a_hint_and_ambiguous_extensions_are_not_guessed() {
    let registry = Registry::<()>::new();
    let detected = registry
        .detect(
            &Input::new(b"not parsed").with_name("FONT.BDF.GZ"),
            &ResourceLimits::default(),
        )
        .unwrap();
    assert_eq!(detected.format.id, "bdf");
    assert_eq!(detected.confidence, Confidence::Extension);
    let fnt = registry.extension_candidates("foo.fnt");
    assert_eq!(
        fnt.iter().map(|format| format.id).collect::<Vec<_>>(),
        ["fnt", "fontx2", "bmfont", "gem"]
    );
    assert!(matches!(
        registry.format_from_name("foo.fnt"),
        Err(Error::AmbiguousFormat { .. })
    ));
    assert!(matches!(
        registry.format_from_name("foo.ttf"),
        Err(Error::AmbiguousFormat { .. })
    ));
    assert!(matches!(
        registry.format_from_name("foo.unknown"),
        Err(Error::UnrecognizedFormat)
    ));
}

#[test]
fn content_evidence_precedes_wrong_extensions() {
    let registry = registry();
    let detected = registry
        .detect(
            &Input::new(b"FPTEST\0").with_name("pretend.bdf"),
            &ResourceLimits::default(),
        )
        .unwrap();
    assert_eq!(detected.format.id, FIXTURE.id);
    assert_eq!(detected.confidence, Confidence::Certain);
}

#[test]
fn stronger_content_wins_and_equal_strength_remains_ambiguous() {
    let registry = detected_registry(Confidence::Possible, Confidence::Certain);
    assert_eq!(
        registry
            .detect(&Input::new(b"x"), &ResourceLimits::default())
            .unwrap()
            .format
            .id,
        OTHER.id
    );
    let registry = detected_registry(Confidence::Certain, Confidence::Certain);
    assert!(
        matches!(registry.detect(&Input::new(b"x").with_name("hint.bdf"), &ResourceLimits::default()), Err(Error::AmbiguousFormat { candidates }) if candidates == [FIXTURE.id, OTHER.id])
    );
}

#[test]
fn no_evidence_errors_and_detector_cannot_mislabel_filename_evidence() {
    assert!(matches!(
        Registry::<()>::new().detect(&Input::new(b"x"), &ResourceLimits::default()),
        Err(Error::UnrecognizedFormat)
    ));
    let registry = detected_registry(Confidence::Extension, Confidence::NoMatch);
    assert!(matches!(
        registry.detect(&Input::new(b"x"), &ResourceLimits::default()),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn registration_rejects_duplicates_disabled_features_and_changed_catalogue() {
    let mut registry = registry();
    assert!(registry.register(Registration::planned(FIXTURE)).is_err());
    assert!(matches!(
        registry.register(Registration::planned(FormatInfo {
            id: "disabled-fixture",
            feature_enabled: false,
            ..FIXTURE
        })),
        Err(Error::FeatureDisabled { .. })
    ));
    assert!(
        registry
            .register(Registration::planned(FormatInfo {
                id: "BAD ID",
                ..FIXTURE
            }))
            .is_err()
    );
    assert!(
        registry
            .register(Registration::planned(FormatInfo {
                id: "bdf",
                ..FIXTURE
            }))
            .is_err()
    );
}

#[test]
fn explicit_read_bypasses_detection_and_sidecar_request_is_real() {
    let registry = registry();
    let options = ReadOptions {
        format: Some(FIXTURE.id.into()),
        ..ReadOptions::default()
    };
    assert!(matches!(
        registry.read(&Input::new(b"FPTEST\0"), &options),
        Err(Error::MissingResource(_))
    ));
    assert!(matches!(
        registry.read(&Input::new(b"wrong"), &options),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn multi_asset_write_and_resource_backed_read_roundtrip_synthetic_model() {
    let registry = registry();
    let model = vec![0, 1, 0xfe, 0xff];
    let options = WriteOptions::new(path("index.fixture"));
    let written = registry.write(&model, FIXTURE.id, &options).unwrap();
    assert_eq!(written.output.primary().as_str(), "index.fixture");
    assert_eq!(written.output.assets().len(), 2);
    assert_eq!(
        written,
        registry.write(&model, FIXTURE.id, &options).unwrap()
    );
    assert!(matches!(
        written.output.clone().into_single(),
        Err(Error::MultipleAssets { count: 2 })
    ));
    let map = MapResolver(
        written
            .output
            .assets()
            .iter()
            .map(|asset| (asset.path.to_string(), asset.bytes.clone()))
            .collect(),
    );
    let input = Input::new(&map.0["index.fixture"]).with_resources(&map);
    let loaded = registry.read(&input, &ReadOptions::default()).unwrap();
    assert_eq!(loaded.fonts, model);
    assert_eq!(loaded.format, FIXTURE.id);
    assert!(loaded.report.entries.is_empty());
}

#[test]
fn asset_paths_block_absolute_parent_windows_and_noncanonical_paths() {
    for value in [
        "", "/x", "../x", "a/../b", "./x", "a//b", "a/", "C:/x", "a\\b", "a\0b", "a\nb", "a\tb",
    ] {
        assert!(ResourcePath::new(value).is_err(), "{value:?}");
    }
    assert_eq!(path("pages/字.png").as_str(), "pages/字.png");
}

#[test]
fn output_bundle_validates_collisions_entry_point_and_order() {
    let asset = |name| OutputAsset {
        path: path(name),
        bytes: vec![1],
    };
    assert!(OutputBundle::new(path("x"), vec![]).is_err());
    assert!(OutputBundle::new(path("x"), vec![asset("y")]).is_err());
    assert!(OutputBundle::new(path("x"), vec![asset("x"), asset("x")]).is_err());
    assert!(OutputBundle::new(path("x"), vec![asset("x"), asset("x/a")]).is_err());
    let bundle = OutputBundle::new(path("z"), vec![asset("z"), asset("a")]).unwrap();
    assert_eq!(
        bundle
            .assets()
            .iter()
            .map(|asset| asset.path.as_str())
            .collect::<Vec<_>>(),
        ["a", "z"]
    );
    assert_eq!(
        OutputBundle::single(path("x"), vec![1, 2])
            .into_single()
            .unwrap(),
        [1, 2]
    );
}

#[test]
fn primary_and_aggregate_input_limits_are_enforced() {
    let limits = ResourceLimits {
        max_input_bytes: 1,
        ..ResourceLimits::default()
    };
    assert!(matches!(
        ReadContext::new(&Input::new(b"xx"), &limits),
        Err(Error::ResourceLimit { .. })
    ));
    let limits = ResourceLimits {
        max_total_bytes: 1,
        ..ResourceLimits::default()
    };
    assert!(matches!(
        ReadContext::new(&Input::new(b"xx"), &limits),
        Err(Error::ResourceLimit { .. })
    ));
    let resolver = MapResolver(BTreeMap::from([("x".into(), vec![1, 2])]));
    let input = Input::new(b"0").with_resources(&resolver);
    let limits = ResourceLimits {
        max_total_bytes: 4,
        ..ResourceLimits::default()
    };
    let mut context = ReadContext::new(&input, &limits).unwrap();
    assert_eq!(context.resource(&path("x")).unwrap(), [1, 2]);
    assert!(matches!(
        context.resource(&path("x")),
        Err(Error::ResourceLimit { .. })
    ));
}

#[test]
fn resolver_results_and_call_count_are_checked() {
    struct Oversized;
    impl ResourceResolver for Oversized {
        fn resolve(&self, _: &ResourcePath, _: usize) -> fontpixel::Result<Vec<u8>> {
            Ok(vec![0; 4])
        }
    }
    let input = Input::new(b"").with_resources(&Oversized);
    let limits = ResourceLimits {
        max_resource_bytes: 3,
        ..ResourceLimits::default()
    };
    let mut context = ReadContext::new(&input, &limits).unwrap();
    assert!(matches!(
        context.resource(&path("x")),
        Err(Error::ResourceLimit { limit: 3, .. })
    ));
    let limits = ResourceLimits {
        max_resources: 0,
        ..ResourceLimits::default()
    };
    let mut context = ReadContext::new(&input, &limits).unwrap();
    assert!(matches!(
        context.resource(&path("x")),
        Err(Error::ResourceLimit {
            resource: "resource count",
            ..
        })
    ));
}

struct DiagnosticWriter {
    report: Report,
    calls: Arc<AtomicUsize>,
}
impl FormatWriter<()> for DiagnosticWriter {
    fn write(&self, _: &(), options: &WriteOptions) -> fontpixel::Result<Written> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(Written {
            output: OutputBundle::single(options.primary_name.clone(), vec![1]),
            report: self.report.clone(),
        })
    }
}
fn diagnostic_registry(report: Report) -> (Registry<()>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = Registry::new();
    registry
        .register(Registration {
            format: FIXTURE,
            detector: None,
            reader: None,
            writer: Some(Box::new(DiagnosticWriter {
                report,
                calls: calls.clone(),
            })),
        })
        .unwrap();
    (registry, calls)
}

#[test]
fn strict_rejects_write_loss_at_any_severity_without_returning_assets() {
    let (registry, calls) = diagnostic_registry(loss());
    let options = WriteOptions {
        strict: true,
        primary_name: path("output"),
    };
    let error = registry.write(&(), FIXTURE.id, &options).unwrap_err();
    assert!(matches!(error, Error::StrictLoss(_)));
    assert_eq!(error.exit_code(), 3);
    assert_eq!(error.report(), Some(&loss()));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn strict_preexisting_read_loss_rejects_before_invoking_writer() {
    let (registry, calls) = diagnostic_registry(Report::default());
    let options = WriteOptions {
        strict: true,
        primary_name: path("output"),
    };
    assert!(matches!(
        registry.write_with_report(&(), FIXTURE.id, &options, loss()),
        Err(Error::StrictLoss(_))
    ));
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn nonstrict_retains_loss_and_merges_reports_in_encounter_order() {
    let (registry, _) = diagnostic_registry(loss());
    let mut prior = loss();
    prior.entries[0].code = "test.prior".into();
    let written = registry
        .write_with_report(&(), FIXTURE.id, &WriteOptions::new(path("output")), prior)
        .unwrap();
    assert_eq!(
        written
            .report
            .entries
            .iter()
            .map(|entry| entry.code.as_str())
            .collect::<Vec<_>>(),
        ["test.prior", "test.loss"]
    );
    assert!(written.report.has_loss());
}

#[test]
fn strict_allows_nonlossy_warnings_and_rejects_error_diagnostics() {
    let mut report = loss();
    report.entries[0].impact = Impact::Repair;
    report.entries[0].level = Level::Warning;
    let (registry, _) = diagnostic_registry(report.clone());
    let options = WriteOptions {
        strict: true,
        primary_name: path("output"),
    };
    assert!(registry.write(&(), FIXTURE.id, &options).is_ok());
    report.entries[0].level = Level::Error;
    let (registry, _) = diagnostic_registry(report);
    assert!(matches!(
        registry.write(&(), FIXTURE.id, &options),
        Err(Error::ReportedErrors(_))
    ));
}

#[test]
fn read_error_diagnostics_are_never_success() {
    struct ErrorReader;
    impl FormatReader<()> for ErrorReader {
        fn read(
            &self,
            _: &mut ReadContext<'_>,
            _: &ReadOptions,
        ) -> fontpixel::Result<((), Report)> {
            let mut report = loss();
            report.entries[0].level = Level::Error;
            Ok(((), report))
        }
    }
    let mut registry = Registry::new();
    registry
        .register(Registration {
            format: FIXTURE,
            detector: None,
            reader: Some(Box::new(ErrorReader)),
            writer: None,
        })
        .unwrap();
    let options = ReadOptions {
        format: Some(FIXTURE.id.into()),
        ..ReadOptions::default()
    };
    assert!(matches!(
        registry.read(&Input::new(b""), &options),
        Err(Error::ReportedErrors(_))
    ));
}

#[test]
fn reports_preserve_location_and_have_deterministic_text() {
    let report = loss();
    assert_eq!(report.entries[0].location.glyph_index, Some(2));
    assert_eq!(
        report.to_string(),
        "Info [test.loss] Loss: Synthetic information loss\n"
    );
    assert!(!report.has_errors());
    assert_eq!(Error::UnrecognizedFormat.exit_code(), 1);
    let io_error = Error::from(std::io::Error::other("test"));
    assert!(std::error::Error::source(&io_error).is_some());
}

#[cfg(feature = "serde")]
#[test]
fn report_json_is_structured_and_roundtrips() {
    let report = loss();
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(value["entries"][0]["impact"], "loss");
    assert_eq!(value["entries"][0]["location"]["codepoint"], 0x1f600);
    assert_eq!(serde_json::from_value::<Report>(value).unwrap(), report);
}
