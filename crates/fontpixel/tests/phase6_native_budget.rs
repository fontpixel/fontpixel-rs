//! Common limits restrict native writers before encoding, with no plain replay.
#![cfg(any(feature = "bdf", feature = "raw"))]

use fontpixel::formats::TransportLimits;
use fontpixel::{Error, FontSet, ResourceLimits, ResourcePath, WriteOptions};

fn options(name: &str, limits: Option<TransportLimits>) -> WriteOptions {
    let mut options = WriteOptions::new(ResourcePath::new(name).unwrap());
    options.transport_limits = limits;
    options
}

#[cfg(any(feature = "bdf", all(feature = "raw", feature = "gzip")))]
fn assert_limit(
    result: fontpixel::Result<fontpixel::resources::Written>,
    resource: &str,
    limit: usize,
) {
    match result {
        Err(Error::ResourceLimit {
            resource: actual,
            limit: actual_limit,
        }) => {
            assert_eq!(actual, resource);
            assert_eq!(actual_limit, limit);
        }
        other => panic!("expected {resource} limit {limit}, got {other:?}"),
    }
}

#[cfg(feature = "bdf")]
mod bdf {
    use super::*;
    use fontpixel::formats::{FormatWriter, bdf::{self, BdfCodec}};
    use fontpixel::model::Property;
    use fontpixel::report::{Impact, Level, Location, ReportEntry};
    use fontpixel::Report;

    const MINI: &[u8] = b"STARTFONT 2.1\nFONT tiny\nSIZE 8 75 75\nFONTBOUNDINGBOX 8 8 0 -1\nSTARTPROPERTIES 3\nFAMILY_NAME \"tiny\"\nFONT_ASCENT 7\nFONT_DESCENT 1\nENDPROPERTIES\nCHARS 1\nSTARTCHAR A\nENCODING 65\nDWIDTH 8 0\nBBX 1 1 0 0\nBITMAP\n80\nENDCHAR\nENDFONT\n";

    fn model() -> FontSet {
        FontSet::from(bdf::read(MINI, &ResourceLimits::default()).unwrap().0)
    }

    #[test]
    fn common_model_limits_reject_plain_and_gzip_before_native_encoding() {
        let fonts = model();
        let registry = fontpixel::registry();
        // BDF has no typed ResourceLimits field. The shared wrapper must apply
        // these model limits rather than relying on BDF's output-byte cap.
        for (resources, resource) in [
            (ResourceLimits { max_glyphs: 0, ..Default::default() }, "glyph count"),
            (ResourceLimits { max_pixels_per_glyph: 0, ..Default::default() }, "glyph pixels"),
            (ResourceLimits { max_total_bytes: 0, ..Default::default() }, "model bitmap bytes"),
        ] {
            for name in ["font.bdf", "font.bdf.gz"] {
                assert_limit(
                    registry.write(&fonts, "bdf", &options(name, Some(TransportLimits {
                        resources: resources.clone(),
                        ..Default::default()
                    }))),
                    resource,
                    0,
                );
            }
        }
    }

    #[test]
    fn common_output_cap_has_an_exact_native_boundary() {
        let fonts = model();
        let registry = fontpixel::registry();
        let plain = options("font.bdf", None);
        let original = registry.write(&fonts, "bdf", &plain).unwrap();
        let size = original.output.assets()[0].bytes.len();
        assert!(size > 1);
        let common = |maximum| options("font.bdf", Some(TransportLimits {
            max_output_bytes: maximum,
            ..Default::default()
        }));
        assert!(matches!(registry.write(&fonts, "bdf", &common(size - 1)),
            Err(Error::ResourceLimit { .. })));
        assert_eq!(registry.write(&fonts, "bdf", &common(size)).unwrap(), original);
        // BDF's adapter also restricts its native output cap by common total.
        for maximum in [size - 1, size] {
            let result = registry.write(&fonts, "bdf", &options("font.bdf", Some(TransportLimits {
                resources: ResourceLimits { max_total_bytes: maximum, ..Default::default() },
                ..Default::default()
            })));
            if maximum < size {
                assert!(matches!(result, Err(Error::ResourceLimit { .. })));
            } else {
                assert_eq!(result.unwrap(), original);
            }
        }
    }

    #[test]
    fn default_and_nonrestrictive_plain_paths_preserve_bytes_and_reports() {
        let mut fonts = model();
        fonts.properties.push(Property::new("opaque-container-property", "kept in source"));
        let plain = options("font.bdf", None);
        let direct = BdfCodec::default().write(&fonts, &plain).unwrap();
        assert!(direct.report.has_loss());
        let registry = fontpixel::registry();
        assert_eq!(registry.write(&fonts, "bdf", &plain).unwrap(), direct);
        assert_eq!(registry.write(&fonts, "bdf", &options("font.bdf", Some(
            TransportLimits::default(),
        ))).unwrap(), direct);
        // Diagnostics are returned once in writer order, not from a replay.
        let mut strict = plain;
        strict.strict = true;
        match registry.write(&fonts, "bdf", &strict) {
            Err(Error::StrictLoss(report)) => assert_eq!(report, direct.report),
            other => panic!("expected strict native report, got {other:?}"),
        }
    }

    #[test]
    fn incoming_error_and_strict_loss_precede_new_common_budget_checks() {
        let fonts = model();
        let registry = fontpixel::registry();
        let mut limited = options("font.bdf", Some(TransportLimits {
            resources: ResourceLimits { max_glyphs: 0, ..Default::default() },
            ..Default::default()
        }));
        limited.strict = true;
        for (level, impact) in [(Level::Error, Impact::Loss), (Level::Warning, Impact::Loss)] {
            let report = Report { entries: vec![ReportEntry {
                level,
                impact,
                code: "input.original".into(),
                location: Location::default(),
                message: "original input diagnostic".into(),
            }] };
            let result = registry.write_with_report(&fonts, "bdf", &limited, report.clone());
            match (level, result) {
                (Level::Error, Err(Error::ReportedErrors(actual))) => assert_eq!(actual, report),
                (Level::Warning, Err(Error::StrictLoss(actual))) => assert_eq!(actual, report),
                (_, other) => panic!("incoming diagnostic lost precedence: {other:?}"),
            }
        }
        assert_limit(registry.write(&fonts, "bdf", &limited), "glyph count", 0);
    }
}

#[cfg(feature = "raw")]
mod raw {
    use super::*;
    use fontpixel::formats::raw::{self, RawLayout, RawReadOptions, RawWriteOptions};

    fn model() -> FontSet {
        raw::read(&[128], &RawReadOptions {
            layout: Some(RawLayout::new(1, 1)),
        }, &ResourceLimits::default()).unwrap().0
    }

    fn registry(typed: RawWriteOptions) -> fontpixel::FontRegistry {
        let mut registry = fontpixel::registry();
        registry.set_raw_write_options(typed);
        registry
    }

    #[test]
    fn common_native_input_and_output_limits_are_forwarded() {
        let fonts = model();
        let registry = registry(RawWriteOptions {
            layout: Some(RawLayout::new(1, 1)),
            ..Default::default()
        });
        for common in [
            TransportLimits { max_output_bytes: 0, ..Default::default() },
            TransportLimits {
                resources: ResourceLimits { max_input_bytes: 0, ..Default::default() },
                ..Default::default()
            },
        ] {
            assert!(matches!(registry.write(&fonts, "raw", &options("font.raw", Some(common))),
                Err(Error::ResourceLimit { limit: 0, .. })));
        }
        let exact = registry.write(&fonts, "raw", &options("font.raw", Some(TransportLimits {
            max_output_bytes: 1,
            ..Default::default()
        }))).unwrap();
        assert_eq!(exact.output.assets()[0].bytes, [128]);
    }

    #[test]
    fn higher_common_limits_cannot_raise_native_typed_model_or_wire_limits() {
        let fonts = model();
        for typed in [
            RawWriteOptions { limits: ResourceLimits { max_glyphs: 0, ..Default::default() }, ..Default::default() },
            RawWriteOptions { limits: ResourceLimits { max_input_bytes: 0, ..Default::default() }, ..Default::default() },
            RawWriteOptions { max_output_bytes: 0, ..Default::default() },
        ] {
            let registry = registry(typed);
            assert!(matches!(registry.write(&fonts, "raw", &options("font.raw", Some(
                TransportLimits::default(),
            ))), Err(Error::ResourceLimit { limit: 0, .. })));
        }
    }

    #[test]
    fn typed_output_limit_wins_at_its_exact_native_boundary() {
        let fonts = raw::read(&[128, 0], &RawReadOptions {
            layout: Some(RawLayout::new(1, 1)),
        }, &ResourceLimits::default()).unwrap().0;
        let common = options("font.raw", Some(TransportLimits {
            max_output_bytes: 1000,
            ..Default::default()
        }));
        for maximum in [1, 2] {
            let registry = registry(RawWriteOptions {
                layout: Some(RawLayout::new(1, 1)),
                max_output_bytes: maximum,
                ..Default::default()
            });
            let result = registry.write(&fonts, "raw", &common);
            if maximum == 1 {
                assert!(matches!(result, Err(Error::ResourceLimit { limit: 1, .. })));
            } else {
                assert_eq!(result.unwrap().output.assets()[0].bytes, [128, 0]);
            }
        }
    }

    #[cfg(feature = "gzip")]
    #[test]
    fn gzip_keeps_default_bytes_reports_and_writer_only_native_profile() {
        let fonts = model();
        // The registry reader has no raw layout. Gzip validation must keep the
        // writer's profile; the new hook must not add an independent readback.
        let registry = registry(RawWriteOptions {
            layout: Some(RawLayout::new(1, 1)),
            ..Default::default()
        });
        let plain = registry.write(&fonts, "raw", &options("font.raw", None)).unwrap();
        let original = registry.write(&fonts, "raw", &options("font.raw.gz", None)).unwrap();
        let restricted = registry.write(&fonts, "raw", &options("font.raw.gz", Some(
            TransportLimits::default(),
        ))).unwrap();
        assert_eq!(restricted, original);
        assert_eq!(restricted.report, plain.report);
        let input = fontpixel::Input::new(&restricted.output.assets()[0].bytes);
        let limits = ResourceLimits::default();
        let decoded = fontpixel::resources::ReadContext::new(&input, &limits).unwrap();
        assert_eq!(decoded.bytes(), plain.output.assets()[0].bytes);
        for name in ["font.raw", "font.raw.gz"] {
            assert_limit(registry.write(&fonts, "raw", &options(name, Some(TransportLimits {
                resources: ResourceLimits { max_glyphs: 0, ..Default::default() },
                ..Default::default()
            }))), "glyph count", 0);
        }
    }
}
