//! Whole-call heap of the public WOFF1/WOFF2 codecs versus the working ledger.
//!
//! For each operation the smallest `max_total_bytes` that succeeds is found;
//! the operation is then run at exactly that limit under the counting
//! allocator. Every byte the call allocates must be inside the ledger, apart
//! from a small allowance for report strings, so the observed peak may exceed
//! the limit by at most [`UNLEDGERED`]. This covers the fixed miniz deflate and
//! inflate workspaces, the Brotli decoder cells, and the Brotli encoder bound.
//! Set `FONTPIXEL_HEAP_CHECK_FONT` to a large sfnt to add it.
use fontpixel::woff::{Auxiliary, Limits, woff1, woff2};
use fontpixel_heap_check::measure;

const FONT: &[u8] = include_bytes!("../../../tests/data/compat/mini-square-1.0-1.ttf");
const OTHER: &[u8] = include_bytes!("../../../tests/data/compat/mini-circle-1.0-1.ttf");
/// Report entries and error strings are not ledgered.
const UNLEDGERED: usize = 16 * 1024;

fn limits(total: usize) -> Limits {
    let mut l = Limits::default();
    l.sfnt.resources.max_total_bytes = total;
    l
}

fn check<T>(name: &str, run: impl Fn(&Limits) -> fontpixel::Result<T>) {
    let mut high = Limits::default().sfnt.resources.max_total_bytes;
    if run(&limits(high)).is_err() {
        eprintln!("{name}: fails at the default working limit; not measured");
        return;
    }
    let mut low = 0usize;
    while low + 1 < high {
        let mid = low + (high - low) / 2;
        if run(&limits(mid)).is_ok() {
            high = mid;
        } else {
            low = mid;
        }
    }
    let (result, peak) = measure(|| run(&limits(high)));
    assert!(result.is_ok(), "{name}: not deterministic at {high}");
    drop(result);
    eprintln!("{name}: minimal max_total_bytes {high}, observed heap {peak}");
    assert!(
        peak <= high + UNLEDGERED,
        "{name}: heap {peak} exceeds ledger {high} + {UNLEDGERED}"
    );
}

#[test]
#[ignore = "heavy; run: cargo test --locked --release -p fontpixel-heap-check -- --ignored --nocapture"]
fn codec_heap_stays_inside_the_working_ledger() {
    let aux = Auxiliary {
        major_version: 1,
        minor_version: 0,
        metadata: Some(b"<metadata version=\"1.0\"><x/></metadata>".repeat(40)),
        private_data: Some(vec![5; 300]),
    };
    let mut fonts: Vec<(String, Vec<u8>)> = vec![("mini-square".into(), FONT.to_vec())];
    if let Some(path) = std::env::var_os("FONTPIXEL_HEAP_CHECK_FONT") {
        let data = std::fs::read(&path).expect("FONTPIXEL_HEAP_CHECK_FONT is readable");
        fonts.push((path.to_string_lossy().into_owned(), data));
    } else {
        eprintln!("FONTPIXEL_HEAP_CHECK_FONT unset: multi-MB sfnt case skipped");
    }
    for (name, font) in &fonts {
        for level in [0, 6, 9] {
            let o = woff1::Options {
                compression_level: level,
                auxiliary: aux.clone(),
            };
            check(&format!("{name} woff1 encode level {level}"), |l| {
                woff1::encode(font, &o, l)
            });
            let encoded = woff1::encode(font, &o, &Limits::default()).unwrap();
            check(&format!("{name} woff1 decode level {level}"), |l| {
                woff1::decode(&encoded.bytes, l)
            });
        }
        for quality in [0, 5, 9, 11] {
            for threads in [1, 2] {
                for transform in [true, false] {
                    let o = woff2::Options {
                        quality,
                        compression_threads: threads,
                        transform_glyf: transform,
                        transform_hmtx: transform,
                        auxiliary: aux.clone(),
                    };
                    let case = format!("{name} woff2 q{quality} t{threads} transform {transform}");
                    check(&format!("{case} encode"), |l| woff2::encode(font, &o, l));
                    let unlimited = limits(usize::MAX);
                    let encoded = woff2::encode(font, &o, &unlimited).unwrap();
                    check(&format!("{case} decode"), |l| {
                        woff2::decode(&encoded.bytes, l)
                    });
                }
            }
        }
    }
    let (ttc, _) =
        fontpixel::sfnt::collection::encode(&[FONT, OTHER, FONT], 0x20000, &Default::default())
            .unwrap();
    for quality in [2, 9] {
        let o = woff2::Options {
            quality,
            auxiliary: aux.clone(),
            ..Default::default()
        };
        check(&format!("ttc woff2 q{quality} encode"), |l| {
            woff2::encode(&ttc, &o, l)
        });
        let encoded = woff2::encode(&ttc, &o, &Limits::default()).unwrap();
        check(&format!("ttc woff2 q{quality} decode"), |l| {
            woff2::decode(&encoded.bytes, l)
        });
    }
}
