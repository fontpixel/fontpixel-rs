//! Public registry gzip budgets, native profiles, strict reports and bundles.
#![cfg(feature = "gzip")]
use fontpixel::formats::{FormatInfo, FormatWriter, Registration, Registry, TransportLimits};
use fontpixel::report::{Impact, Level, Location, ReportEntry};
use fontpixel::resources::{OutputAsset, OutputBundle, ReadContext, Written};
use fontpixel::{Error, Input, Report, ResourceLimits, ResourcePath, WriteOptions};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

fn path(name: &str) -> ResourcePath {
    ResourcePath::new(name).unwrap()
}
fn diagnostic(impact: Impact, code: &str) -> ReportEntry {
    ReportEntry {
        level: Level::Warning,
        impact,
        code: code.into(),
        location: Location::default(),
        message: "original diagnostic retained once".into(),
    }
}
#[derive(Clone, Copy)]
enum Shape {
    File,
    Multiple,
    Directory,
}
type Validation = Arc<Mutex<Option<TransportLimits>>>;
struct BytesWriter {
    limits: TransportLimits,
    shape: Shape,
    report: Report,
    calls: Arc<AtomicUsize>,
    validation: Validation,
}
impl FormatWriter<Vec<u8>> for BytesWriter {
    fn write(&self, data: &Vec<u8>, o: &WriteOptions) -> fontpixel::Result<Written> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let output = match self.shape {
            Shape::File => OutputBundle::single(o.primary_name.clone(), data.clone()),
            Shape::Multiple => OutputBundle::new(
                o.primary_name.clone(),
                vec![
                    OutputAsset {
                        path: o.primary_name.clone(),
                        bytes: data.clone(),
                    },
                    OutputAsset {
                        path: path("other.bin"),
                        bytes: vec![1],
                    },
                ],
            )?,
            Shape::Directory => OutputBundle::directory(
                o.primary_name.clone(),
                vec![OutputAsset {
                    path: path(&format!("{}/41.txt", o.primary_name)),
                    bytes: data.clone(),
                }],
            )?,
        };
        Ok(Written {
            output,
            report: self.report.clone(),
        })
    }
    fn transport_limits(&self) -> TransportLimits {
        self.limits.clone()
    }
    fn validate_transport(
        &self,
        _: &Vec<u8>,
        _: &WriteOptions,
        limits: &TransportLimits,
    ) -> fontpixel::Result<()> {
        *self.validation.lock().unwrap() = Some(limits.clone());
        Ok(())
    }
}
fn registry(
    limits: TransportLimits,
    shape: Shape,
    report: Report,
) -> (Registry<Vec<u8>>, Arc<AtomicUsize>, Validation) {
    let calls = Arc::new(AtomicUsize::new(0));
    let validation = Arc::new(Mutex::new(None));
    let mut r = Registry::new();
    r.register(Registration {
        format: FormatInfo {
            id: "transport-test",
            name: "original transport test",
            extensions: &[],
            feature_enabled: true,
        },
        detector: None,
        reader: None,
        writer: Some(Box::new(BytesWriter {
            limits,
            shape,
            report,
            calls: calls.clone(),
            validation: validation.clone(),
        })),
    })
    .unwrap();
    (r, calls, validation)
}
fn wire(r: &Registry<Vec<u8>>, bytes: Vec<u8>, name: &str) -> fontpixel::Result<Written> {
    r.write(&bytes, "transport-test", &WriteOptions::new(path(name)))
}

#[test]
fn decoded_growth_is_capped_and_its_spare_capacity_is_reserved() {
    use std::io::Write;
    let original = vec![19; 14_000];
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&original).unwrap();
    let compressed = encoder.finish().unwrap();
    let input = Input::new(&compressed);
    let limits = ResourceLimits {
        max_total_bytes: 16_000,
        max_decompressed_bytes: 20_000,
        ..Default::default()
    };
    let context = ReadContext::new(&input, &limits).unwrap();
    assert_eq!(context.bytes(), original);
    // The second chunk needs growth beyond 8192. Its geometric reservation
    // must stop at the remaining total allowance and include unused capacity.
    assert_eq!(context.retained_input_bytes(), limits.max_total_bytes);
    let limits = ResourceLimits {
        max_total_bytes: compressed.len() + original.len() - 1,
        ..limits
    };
    assert!(matches!(
        ReadContext::new(&input, &limits),
        Err(Error::ResourceLimit { .. })
    ));
}

#[test]
fn envelope_growth_and_plaintext_are_one_budget() {
    // One plaintext byte has a 21-byte deterministic gzip representation.
    for (total, success) in [(21, false), (22, true)] {
        let limits = TransportLimits {
            resources: ResourceLimits {
                max_total_bytes: total,
                ..Default::default()
            },
            ..Default::default()
        };
        let (r, _, _) = registry(limits.clone(), Shape::File, Report::default());
        let value = wire(&r, vec![128], "font.gz");
        assert_eq!(value.is_ok(), success);
        if let Ok(value) = value {
            let bytes = &value.output.assets()[0].bytes;
            assert_eq!(bytes.len(), 21);
            let input = Input::new(bytes);
            let c = ReadContext::new(&input, &limits.resources).unwrap();
            assert_eq!(c.bytes(), [128]);
        }
    }
}

#[test]
fn caps_include_header_trailer_and_decompressed_body() {
    let mut limits = TransportLimits {
        max_output_bytes: 20,
        ..Default::default()
    };
    for field in 0..3 {
        if field == 1 {
            limits.max_output_bytes = 1024;
            limits.resources.max_input_bytes = 20;
        }
        if field == 2 {
            limits.resources.max_input_bytes = 1024;
            limits.resources.max_decompressed_bytes = 0;
        }
        let (r, _, _) = registry(limits.clone(), Shape::File, Report::default());
        assert!(matches!(
            wire(&r, vec![128], "font.gz"),
            Err(Error::ResourceLimit { .. })
        ));
        assert_eq!(
            wire(&r, vec![128], "font.raw").unwrap().output.assets()[0].bytes,
            [128]
        );
    }
}

#[test]
fn common_limits_can_only_restrict_typed_limits() {
    let (r, _, _) = registry(
        TransportLimits {
            max_output_bytes: 20,
            ..Default::default()
        },
        Shape::File,
        Report::default(),
    );
    let mut options = WriteOptions::new(path("font.gz"));
    options.transport_limits = Some(TransportLimits {
        max_output_bytes: usize::MAX,
        ..Default::default()
    });
    assert!(matches!(
        r.write(&vec![1], "transport-test", &options),
        Err(Error::ResourceLimit { .. })
    ));
    let (r, _, _) = registry(TransportLimits::default(), Shape::File, Report::default());
    options
        .transport_limits
        .as_mut()
        .unwrap()
        .resources
        .max_input_bytes = 20;
    assert!(matches!(
        r.write(&vec![1], "transport-test", &options),
        Err(Error::ResourceLimit { .. })
    ));
}

#[test]
fn retained_diagnostics_are_charged_once_before_native_validation() {
    let own = Report {
        entries: vec![diagnostic(Impact::None, "test.write")],
    };
    let earlier = Report {
        entries: vec![diagnostic(Impact::Repair, "test.read")],
    };
    let limits = TransportLimits {
        resources: ResourceLimits {
            max_total_bytes: 8192,
            ..Default::default()
        },
        ..Default::default()
    };
    let (r, calls, validation) = registry(limits, Shape::File, own);
    let result = r
        .write_with_report(
            &vec![128],
            "transport-test",
            &WriteOptions::new(path("font.gz")),
            earlier,
        )
        .unwrap();
    assert_eq!(
        result
            .report
            .entries
            .iter()
            .map(|e| e.code.as_str())
            .collect::<Vec<_>>(),
        ["test.read", "test.write"]
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let retained = result.report.entries.capacity() * std::mem::size_of::<ReportEntry>()
        + result
            .report
            .entries
            .iter()
            .map(|e| e.code.capacity() + e.message.capacity())
            .sum::<usize>()
        + result.output.assets()[0].bytes.capacity();
    assert_eq!(
        validation
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .resources
            .max_total_bytes,
        8192 - retained
    );
    let (r, _, _) = registry(
        TransportLimits {
            resources: ResourceLimits {
                max_total_bytes: 22,
                ..Default::default()
            },
            ..Default::default()
        },
        Shape::File,
        result.report,
    );
    assert!(matches!(
        wire(&r, vec![128], "font.gz"),
        Err(Error::ResourceLimit {
            resource: "gzip retained diagnostics",
            ..
        })
    ));
}

#[test]
fn strict_loss_precedes_compression_and_does_not_duplicate_reports() {
    let (r, calls, validation) = registry(
        TransportLimits {
            max_output_bytes: 0,
            ..Default::default()
        },
        Shape::File,
        Report {
            entries: vec![diagnostic(Impact::Loss, "test.loss")],
        },
    );
    let mut o = WriteOptions::new(path("font.gz"));
    o.strict = true;
    let error = r
        .write_with_report(
            &vec![1],
            "transport-test",
            &o,
            Report {
                entries: vec![diagnostic(Impact::None, "test.input")],
            },
        )
        .unwrap_err();
    match error {
        Error::StrictLoss(r) => assert_eq!(
            r.entries
                .iter()
                .map(|e| e.code.as_str())
                .collect::<Vec<_>>(),
            ["test.input", "test.loss"]
        ),
        e => panic!("{e:?}"),
    }
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(validation.lock().unwrap().is_none());
}

#[test]
fn failed_native_validation_keeps_read_report_and_deduplicates_replayed_prefix() {
    struct AuditFailure;
    impl FormatWriter<Vec<u8>> for AuditFailure {
        fn write(&self, bytes: &Vec<u8>, o: &WriteOptions) -> fontpixel::Result<Written> {
            Ok(Written {
                output: OutputBundle::single(o.primary_name.clone(), bytes.clone()),
                report: Report {
                    entries: vec![diagnostic(Impact::Loss, "test.write")],
                },
            })
        }
        fn validate_transport(
            &self,
            _: &Vec<u8>,
            _: &WriteOptions,
            _: &TransportLimits,
        ) -> fontpixel::Result<()> {
            let mut failed = diagnostic(Impact::Loss, "test.validation.failed");
            failed.level = Level::Error;
            Err(Error::ReportedErrors(Report {
                entries: vec![diagnostic(Impact::Loss, "test.write"), failed],
            }))
        }
    }
    let mut r = Registry::new();
    r.register(Registration {
        format: FormatInfo {
            id: "audit-test",
            name: "native validation test",
            extensions: &[],
            feature_enabled: true,
        },
        detector: None,
        reader: None,
        writer: Some(Box::new(AuditFailure)),
    })
    .unwrap();
    let error = r
        .write_with_report(
            &vec![128],
            "audit-test",
            &WriteOptions::new(path("font.gz")),
            Report {
                entries: vec![diagnostic(Impact::Repair, "test.read")],
            },
        )
        .unwrap_err();
    let Error::ReportedErrors(report) = error else {
        panic!("{error}")
    };
    assert_eq!(
        report
            .entries
            .iter()
            .map(|e| e.code.as_str())
            .collect::<Vec<_>>(),
        ["test.read", "test.write", "test.validation.failed"]
    );
}

#[test]
fn directories_multiple_assets_and_nongzip_names_keep_their_contracts() {
    let (r, calls, validation) = registry(
        TransportLimits {
            max_output_bytes: 0,
            ..Default::default()
        },
        Shape::Directory,
        Report::default(),
    );
    assert!(
        wire(&r, vec![3], "directory.gz")
            .unwrap()
            .output
            .is_directory()
    );
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(validation.lock().unwrap().is_none());
    let (r, _, _) = registry(
        TransportLimits::default(),
        Shape::Multiple,
        Report::default(),
    );
    assert!(matches!(
        wire(&r, vec![3], "font.gz"),
        Err(Error::MultipleAssets { count: 2 })
    ));
    let (r, _, validation) = registry(TransportLimits::default(), Shape::File, Report::default());
    assert_eq!(
        wire(&r, vec![3], "gz").unwrap().output.assets()[0].bytes,
        [3]
    );
    assert!(validation.lock().unwrap().is_none());
    assert!(
        wire(&r, vec![3], "font.GZ").unwrap().output.assets()[0]
            .bytes
            .starts_with(&[31, 139])
    );
    assert!(matches!(
        wire(&r, vec![31, 139], "font.gz"),
        Err(Error::InvalidInput(_))
    ));
}

#[test]
fn incompressible_input_hits_bounded_sink_and_large_success_reopens() {
    let mut state = 0x6543_2189_u32;
    let data = (0..65_536)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect::<Vec<_>>();
    let (r, _, _) = registry(
        TransportLimits {
            max_output_bytes: 128,
            ..Default::default()
        },
        Shape::File,
        Report::default(),
    );
    assert!(matches!(
        wire(&r, data.clone(), "font.gz"),
        Err(Error::ResourceLimit { .. })
    ));
    let limits = TransportLimits::default();
    let (r, _, _) = registry(limits.clone(), Shape::File, Report::default());
    let result = wire(&r, data.clone(), "font.gz").unwrap();
    let input = Input::new(&result.output.assets()[0].bytes);
    assert_eq!(
        ReadContext::new(&input, &limits.resources).unwrap().bytes(),
        data
    );
}

#[cfg(feature = "raw")]
mod native {
    use super::*;
    use fontpixel::formats::raw::{self, RawLayout, RawReadOptions, RawWriteOptions};
    fn model() -> fontpixel::FontSet {
        raw::read(
            &[128],
            &RawReadOptions {
                layout: Some(RawLayout::new(1, 1)),
            },
            &ResourceLimits::default(),
        )
        .unwrap()
        .0
    }
    #[test]
    fn exact_public_raw_regression_and_writer_only_layout() {
        let fonts = model();
        for (max_input, success) in [(1, false), (20, false), (21, true)] {
            let limits = ResourceLimits {
                max_input_bytes: max_input,
                ..Default::default()
            };
            let mut r = fontpixel::registry();
            r.set_raw_write_options(RawWriteOptions {
                layout: Some(RawLayout::new(1, 1)),
                limits: limits.clone(),
                ..Default::default()
            });
            assert_eq!(
                r.write(&fonts, "raw", &WriteOptions::new(path("font.raw")))
                    .unwrap()
                    .output
                    .assets()[0]
                    .bytes,
                [128]
            );
            // The registry reader deliberately has no layout. Validation must
            // use the writer's profile and must not blindly invoke that reader.
            let result = r.write(&fonts, "raw", &WriteOptions::new(path("font.raw.gz")));
            assert_eq!(result.is_ok(), success, "{result:?}");
            if let Ok(result) = result {
                r.set_raw_read_options(RawReadOptions {
                    layout: Some(RawLayout::new(1, 1)),
                });
                let input = Input::new(&result.output.assets()[0].bytes);
                assert_eq!(
                    r.read(
                        &input,
                        &fontpixel::ReadOptions {
                            format: Some("raw".into()),
                            limits
                        }
                    )
                    .unwrap()
                    .fonts
                    .fonts[0]
                        .len(),
                    1
                );
            }
        }
    }
    #[test]
    fn independent_typed_output_and_total_native_audit_caps() {
        let fonts = model();
        let mut r = fontpixel::registry();
        r.set_raw_write_options(RawWriteOptions {
            layout: Some(RawLayout::new(1, 1)),
            max_output_bytes: 1,
            ..Default::default()
        });
        assert!(matches!(
            r.write(&fonts, "raw", &WriteOptions::new(path("font.gz"))),
            Err(Error::ResourceLimit { .. })
        ));
        r.set_raw_write_options(RawWriteOptions {
            layout: Some(RawLayout::new(1, 1)),
            ..Default::default()
        });
        let mut o = WriteOptions::new(path("font.gz"));
        o.transport_limits = Some(TransportLimits {
            resources: ResourceLimits {
                max_total_bytes: 64,
                ..Default::default()
            },
            ..Default::default()
        });
        // Enough for the gzip wire and plaintext, but not the native model/audit.
        assert!(r.write(&fonts, "raw", &o).is_err());
    }

    #[test]
    fn smallest_successful_native_aggregate_budget_reopens() {
        let fonts = model();
        let mut r = fontpixel::registry();
        r.set_raw_read_options(RawReadOptions {
            layout: Some(RawLayout::new(1, 1)),
        });
        r.set_raw_write_options(RawWriteOptions {
            layout: Some(RawLayout::new(1, 1)),
            ..Default::default()
        });
        let write_at = |total| {
            let mut o = WriteOptions::new(path("font.raw.gz"));
            o.transport_limits = Some(TransportLimits {
                resources: ResourceLimits {
                    max_total_bytes: total,
                    ..Default::default()
                },
                ..Default::default()
            });
            r.write(&fonts, "raw", &o)
        };
        let (mut lower, mut upper) = (0, 1024 * 1024);
        assert!(write_at(upper).is_ok());
        while upper - lower > 1 {
            let middle = lower + (upper - lower) / 2;
            if write_at(middle).is_ok() {
                upper = middle;
            } else {
                lower = middle;
            }
        }
        assert!(write_at(upper - 1).is_err());
        let written = write_at(upper).unwrap();
        let loaded = r
            .read(
                &Input::new(&written.output.assets()[0].bytes),
                &fontpixel::ReadOptions {
                    format: Some("raw".into()),
                    limits: ResourceLimits {
                        max_total_bytes: upper,
                        ..Default::default()
                    },
                },
            )
            .unwrap();
        assert_eq!(loaded.fonts, fonts);
    }

    #[cfg(feature = "bbc")]
    #[test]
    fn macro_registered_writer_preserves_its_typed_input_cap() {
        let mut layout = RawLayout::new(8, 8);
        layout.first_codepoint = 65;
        let fonts = raw::read(
            &[129, 66, 36, 24, 24, 36, 66, 129],
            &RawReadOptions {
                layout: Some(layout),
            },
            &ResourceLimits::default(),
        )
        .unwrap()
        .0;
        let mut r = fontpixel::registry();
        let limits = ResourceLimits {
            max_input_bytes: 10,
            ..Default::default()
        };
        r.set_bbc_write_options(fontpixel::formats::bbc::BbcWriteOptions {
            limits,
            ..Default::default()
        });
        assert_eq!(
            r.write(&fonts, "bbc", &WriteOptions::new(path("font.bbc")))
                .unwrap()
                .output
                .assets()[0]
                .bytes
                .len(),
            10
        );
        assert!(matches!(
            r.write(&fonts, "bbc", &WriteOptions::new(path("font.bbc.gz"))),
            Err(Error::ResourceLimit { .. })
        ));
        r.set_bbc_write_options(Default::default());
        let written = r
            .write(&fonts, "bbc", &WriteOptions::new(path("font.bbc.gz")))
            .unwrap();
        let loaded = r
            .read(
                &Input::new(&written.output.assets()[0].bytes),
                &fontpixel::ReadOptions {
                    format: Some("bbc".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(loaded.fonts.fonts[0].len(), 1);
    }

    #[cfg(feature = "svg")]
    #[test]
    fn older_nested_limits_reach_the_transport_layer() {
        let fonts = model();
        let mut r = fontpixel::registry();
        let mut options = fontpixel::formats::svg::WriteOptions::default();
        options.limits.resources.max_decompressed_bytes = 0;
        r.set_svg_write_options(options);
        assert!(
            r.write(&fonts, "svg", &WriteOptions::new(path("font.svg")))
                .is_ok()
        );
        assert!(matches!(
            r.write(&fonts, "svg", &WriteOptions::new(path("font.svg.gz"))),
            Err(Error::ResourceLimit { .. })
        ));
        r.set_svg_write_options(Default::default());
        let written = r
            .write(&fonts, "svg", &WriteOptions::new(path("font.svg.gz")))
            .unwrap();
        let loaded = r
            .read(
                &Input::new(&written.output.assets()[0].bytes),
                &fontpixel::ReadOptions {
                    format: Some("svg".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(loaded.fonts.fonts[0].len(), 1);
    }
}
