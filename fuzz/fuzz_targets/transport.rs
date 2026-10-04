//! Public native/gzip transport with identical typed profiles and bounded budgets.
#![no_main]
use fontpixel::bitmap::{BitOrder, RowOrder};
use fontpixel::formats::{raw, ssfn};
use fontpixel::model::{Codepoint, Glyph, Vector};
use fontpixel::report::{Impact, Level, Location, ReportEntry};
use fontpixel::resources::{TransportLimits, Written};
use fontpixel::{
    Bitmap, Error, Font, FontRegistry, FontSet, Input, ReadOptions, Report, ResourceLimits,
    ResourcePath, WriteOptions,
};
use libfuzzer_sys::fuzz_target;

const WIRE: usize = 262144;
const TOTAL: usize = 2097152;
const MARKER: &str = "transport.fuzz.original";
fn broad() -> ResourceLimits {
    ResourceLimits {
        max_input_bytes: WIRE,
        max_resource_bytes: WIRE,
        max_total_bytes: TOTAL,
        max_resources: 8,
        max_decompressed_bytes: WIRE,
        max_glyphs: 256,
        max_pixels_per_glyph: 4096,
        max_table_bytes: WIRE,
    }
}
fn cap(v: u8) -> usize {
    [0, 1, 19, 20, 21, 22, 32, 128, 512, 2048, 8192, 65536, WIRE][usize::from(v).min(12)]
}
fn budget(control: &[u8]) -> TransportLimits {
    let mut resources = broad();
    resources.max_input_bytes = cap(control[1]);
    resources.max_decompressed_bytes = cap(control[2]);
    resources.max_total_bytes =
        [0, 21, 22, 128, 512, 2048, 8192, 65536, 262144, TOTAL][usize::from(control[3]).min(9)];
    TransportLimits {
        resources,
        max_output_bytes: cap(control[0]),
    }
}
fn intersect(a: &TransportLimits, b: &TransportLimits) -> TransportLimits {
    // Other fields are the same explicit broad bounds in both profiles.
    let mut out = a.clone();
    out.max_output_bytes = out.max_output_bytes.min(b.max_output_bytes);
    out.resources.max_input_bytes = out
        .resources
        .max_input_bytes
        .min(b.resources.max_input_bytes);
    out.resources.max_total_bytes = out
        .resources
        .max_total_bytes
        .min(b.resources.max_total_bytes);
    out.resources.max_decompressed_bytes = out
        .resources
        .max_decompressed_bytes
        .min(b.resources.max_decompressed_bytes);
    out
}
fn layout(h: &[u8]) -> raw::RawLayout {
    let mut l = raw::RawLayout::new(1 + u32::from(h[12] % 16), 1 + u32::from(h[13] % 16));
    l.count = Some(1 + usize::from(h[14] % 4));
    l.first_codepoint = 32 + u32::from(h[15] % 64);
    l.bit_order = if h[2] & 8 == 0 {
        BitOrder::MsbFirst
    } else {
        BitOrder::LsbFirst
    };
    l.row_order = if h[2] & 4 == 0 {
        RowOrder::TopDown
    } else {
        RowOrder::BottomUp
    };
    if h[2] & 1 != 0 {
        l.packing = raw::RawPacking::BitAligned;
    } else {
        l.row_padding = usize::from(h[7] & 1);
        l.byte_order = if h[2] & 2 == 0 {
            raw::RawByteOrder::RowMajor
        } else {
            raw::RawByteOrder::ColumnMajor
        };
        l.strike_count = if h[2] & 32 == 0 { 1 } else { l.count.unwrap() };
    }
    if h[2] & 16 != 0 {
        l.offset = 2;
        l.default_load_address = Some(0x0801);
    }
    l
}
fn writer(id: &str, l: &raw::RawLayout, ascii: bool, limits: &TransportLimits) -> FontRegistry {
    let mut r = fontpixel::registry();
    match id {
        "raw" => r.set_raw_write_options(raw::RawWriteOptions {
            layout: Some(l.clone()),
            limits: limits.resources.clone(),
            max_output_bytes: limits.max_output_bytes,
            ..Default::default()
        }),
        "bdf" => {}
        _ => r.set_ssfn_write_options(ssfn::SsfnWriteOptions {
            encoding: if ascii {
                ssfn::SsfnEncoding::Ascii
            } else {
                ssfn::SsfnEncoding::Binary
            },
            legacy_as_unicode: true,
            limits: limits.resources.clone(),
            max_output_bytes: limits.max_output_bytes,
            ..Default::default()
        }),
    }
    // Deliberately never set this registry's RAW reader. A gzip validation must
    // keep the writer-only layout, not accidentally use a default reader.
    r
}
fn reader(l: &raw::RawLayout) -> FontRegistry {
    let mut r = fontpixel::registry();
    r.set_raw_read_options(raw::RawReadOptions {
        layout: Some(l.clone()),
    });
    r
}
fn options(name: &str) -> WriteOptions {
    WriteOptions::new(ResourcePath::new(name).unwrap())
}
fn load(
    r: &FontRegistry,
    id: &str,
    bytes: &[u8],
    limits: ResourceLimits,
) -> fontpixel::Result<FontSet> {
    Ok(r.read(
        &Input::new(bytes),
        &ReadOptions {
            format: Some(id.into()),
            limits,
        },
    )?
    .fonts)
}
fn original(l: &raw::RawLayout, payload: &[u8]) -> FontSet {
    let mut f = Font::default();
    for i in 0..l.count.unwrap() {
        let mut b = Bitmap::<fontpixel::bitmap::Mono>::new(l.width, l.height).unwrap();
        for y in 0..l.height {
            for x in 0..l.width {
                let at = i * 31 + y as usize * 7 + x as usize;
                let value = payload
                    .get(at % payload.len().max(1))
                    .copied()
                    .unwrap_or(0xa5);
                b.set_pixel(x, y, (value >> (x % 8)) & 1).unwrap();
            }
        }
        let mut g = Glyph::new(b);
        g.codepoints
            .push(Codepoint::Legacy(l.first_codepoint + i as u32));
        g.metrics.advance = Some(Vector::new(i64::from(l.width), 0));
        f.add_glyph(g).unwrap();
    }
    f.into()
}
fn marker_count(report: &Report) -> usize {
    report.entries.iter().filter(|e| e.code == MARKER).count()
}
fn checked_error(error: &Error) {
    if let Error::StrictLoss(r) | Error::ReportedErrors(r) = error {
        assert_eq!(
            marker_count(r),
            1,
            "original diagnostics must survive exactly once"
        );
    }
}
fn check_no_loss(source: &FontSet, back: &FontSet, id: &str) {
    if id != "bdf" {
        assert_eq!(source, back, "no-loss native model roundtrip");
    } else {
        // BDF source-line provenance can be recanonicalized without losing font
        // data. Compare every effective glyph field; full transport-vs-native
        // model equality is independently asserted below, including provenance.
        assert_eq!(source.fonts.len(), back.fonts.len());
        for (a, b) in source.fonts.iter().zip(&back.fonts) {
            assert_eq!(a.len(), b.len());
            for ((_, g), (_, h)) in a.glyphs().zip(b.glyphs()) {
                assert_eq!(g.bitmap, h.bitmap);
                assert_eq!(g.metrics, h.metrics);
                assert_eq!(g.codepoints, h.codepoints);
                assert_eq!(g.unicode_sequences, h.unicode_sequences);
                assert_eq!(g.name, h.name);
            }
        }
    }
}
fn run(data: &[u8]) {
    if data.len() < 16 || data.len() > WIRE {
        return;
    }
    let (h, payload) = data.split_at(16);
    let id = ["raw", "bdf", "ssfn"][usize::from(h[0] % 3)];
    let l = layout(h);
    let ascii = h[2] & 128 != 0;
    let read = reader(&l);
    let generous = TransportLimits {
        resources: broad(),
        max_output_bytes: WIRE,
    };
    let generated = h[1] & 1 == 0;
    let source = if generated {
        // Canonical native baseline from a tiny original model, before varying
        // budgets. This is <=4 glyphs, each <=16x16; failures are legitimate for
        // profiles whose cell alignment is incompatible with the requested data.
        let base = writer(id, &l, ascii, &generous);
        let Ok(w) = base.write(&original(&l, payload), id, &options("native")) else {
            return;
        };
        let Ok(bytes) = w.output.into_single() else {
            return;
        };
        let Ok(s) = load(&read, id, &bytes, broad()) else {
            return;
        };
        s
    } else {
        // Exercise envelope decoding with an independently varied read budget,
        // even when the generous parse used for writer checks succeeds.
        let _ = load(&read, id, payload, budget(&h[8..12]).resources);
        let Ok(s) = load(&read, id, payload, broad()) else {
            return;
        };
        s
    };
    let typed = budget(&h[3..7]);
    let mut common = if h[1] & 32 == 0 {
        generous.clone()
    } else {
        budget(&h[8..12])
    };
    // BDF has no public typed registry setter. Exercise its built-in adapter
    // with common transport limits; RAW/SSFN independently vary both policies.
    let actual_typed = if id == "bdf" {
        common = intersect(&common, &typed);
        generous.clone()
    } else {
        typed.clone()
    };
    let effective = intersect(&actual_typed, &common);
    let write = writer(id, &l, ascii, &typed);
    let mut native_options = options("native");
    native_options.transport_limits = Some(common.clone());
    let native = write.write(&source, id, &native_options);
    let mut o = options(if h[1] & 2 == 0 {
        "native.gz"
    } else {
        "native.GZ"
    });
    o.transport_limits = Some(common);
    let earlier = Report {
        entries: vec![ReportEntry {
            level: Level::Warning,
            impact: if h[1] & 16 != 0 {
                Impact::Loss
            } else {
                Impact::Repair
            },
            code: MARKER.into(),
            location: Location::default(),
            message: format!("original input diagnostic {}", h[7]),
        }],
    };
    let result = write.write_with_report(&source, id, &o, earlier.clone());
    if generated && typed == generous && o.transport_limits.as_ref() == Some(&generous) {
        assert!(
            result.is_ok(),
            "generous writer-only native profile must work: {result:?}"
        );
    }
    match result {
        Ok(Written { output, report }) => {
            assert_eq!(marker_count(&report), 1);
            let bytes = output.into_single().unwrap();
            assert!(bytes.starts_with(&[31, 139]));
            assert!(
                bytes.len()
                    <= effective
                        .max_output_bytes
                        .min(effective.resources.max_input_bytes)
            );
            let back = load(&read, id, &bytes, effective.resources.clone()).expect(
                "successful gzip writer must read under identical effective limits/profile",
            );
            let plain = native
                .expect("gzip success requires successful native serialization")
                .output
                .into_single()
                .unwrap();
            assert!(plain.len() <= effective.resources.max_decompressed_bytes);
            let plain_back = load(&read, id, &plain, broad()).expect("native output reads");
            assert_eq!(
                plain_back, back,
                "gzip must preserve the complete native model"
            );
            if !report.has_loss() {
                check_no_loss(&source, &back, id);
            }
            o.strict = true;
            let expected_strict_report = if earlier.has_loss() {
                earlier.clone()
            } else {
                report.clone()
            };
            let strict = write.write_with_report(&source, id, &o, earlier);
            if report.has_loss() {
                match strict {
                    Err(Error::StrictLoss(r)) => assert_eq!(r, expected_strict_report),
                    other => panic!("strict loss escaped: {other:?}"),
                }
            } else {
                let strict = strict.expect("loss-free strict output");
                assert_eq!(strict.output.into_single().unwrap(), bytes);
                assert_eq!(strict.report, report);
            }
        }
        Err(e) => checked_error(&e),
    }
}
fuzz_target!(|data: &[u8]| run(data));
