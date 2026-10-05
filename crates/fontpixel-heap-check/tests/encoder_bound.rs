//! Observed Brotli encoder heap versus `brotli_encoder_heap_bound`.
//!
//! Each case calls the locked encoder exactly as fontpixel's WOFF2 writer does
//! (same parameters, single-stream/parallel choice, `StandardAlloc` workers)
//! with output and input buffers allocated before measurement, so the
//! measured peak is the encoder's own heap. The admitted bound is proven for
//! quality 0-9 and an empirical envelope for quality 10-11; this matrix is the
//! evidence that the envelope covers observed use, including adversarial
//! match/repeat structures. Set `FONTPIXEL_HEAP_CHECK_FONT` to a
//! colon-separated list of large sfnts to add them to the matrix; `FONTPIXEL_HEAP_CHECK_QUICK=1` limits
//! qualities to {0, 2, 5, 9, 10, 11}.
use brotli::enc::{
    BrotliEncoderParams, SendAlloc, StandardAlloc,
    backward_references::{BrotliEncoderMode, UnionHasher},
};
use fontpixel::woff::woff2::{brotli_encoder_heap_bound, brotli_encoder_proven_heap_bound};
use fontpixel_heap_check::measure;

const PARALLEL_MIN_BYTES: usize = 128 * 1024;

fn xorshift(len: usize, mut state: u32) -> Vec<u8> {
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect()
}
/// Highly repetitive structured data with many overlapping match lengths.
fn structured(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 17 + i / 97) & 255) as u8).collect()
}
/// Font-like data: a real sfnt repeated with small mutations.
fn font_like(len: usize) -> Vec<u8> {
    const FONT: &[u8] = include_bytes!("../../../tests/data/compat/mini-square-1.0-1.ttf");
    let mut out = Vec::with_capacity(len);
    let mut i = 0u32;
    while out.len() < len {
        out.extend_from_slice(FONT);
        out.extend_from_slice(&i.to_be_bytes());
        i = i.wrapping_add(0x9e37);
    }
    out.truncate(len);
    out
}

/// A vocabulary of random words, recombined: many repeats of varied lengths.
fn vocabulary(len: usize) -> Vec<u8> {
    let mut words = Vec::new();
    let mut state = 0x0bad_5eedu32;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };
    for _ in 0..512 {
        let n = 3 + (next() % 38) as usize;
        words.push((0..n).map(|_| next() as u8).collect::<Vec<_>>());
    }
    let mut out = Vec::with_capacity(len + 64);
    while out.len() < len {
        let word = &words[(next() % 512) as usize];
        out.extend_from_slice(word);
        if next() % 7 == 0 {
            out.push(next() as u8);
        }
    }
    out.truncate(len);
    out
}
/// Each 1 KiB chunk repeats the previous one with one byte changed: every
/// position has candidate matches at many distances and lengths.
fn mutating_chunks(len: usize) -> Vec<u8> {
    let mut out = xorshift(1024, 0x5151_7777);
    let mut state = 0x2468_ace1u32;
    while out.len() < len {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let start = out.len() - 1024;
        let mut chunk = out[start..].to_vec();
        chunk[(state % 1024) as usize] ^= (state >> 10) as u8 | 1;
        out.extend_from_slice(&chunk);
    }
    out.truncate(len);
    out
}
/// The Fibonacci word over two bytes: matches of every length at every
/// position.
fn fibonacci(len: usize) -> Vec<u8> {
    let (mut a, mut b) = (vec![b'a'], vec![b'a', b'b']);
    while b.len() < len {
        let next = [b.as_slice(), a.as_slice()].concat();
        a = b;
        b = next;
    }
    b.truncate(len);
    b
}

fn params(quality: u32, lgwin: u32, len: usize) -> BrotliEncoderParams {
    BrotliEncoderParams {
        quality: quality as i32,
        mode: BrotliEncoderMode::BROTLI_MODE_FONT,
        lgwin: lgwin as i32,
        size_hint: len,
        ..Default::default()
    }
}

/// Encoder-internal peak for fontpixel's call shape.
fn observed(data: &[u8], quality: u32, lgwin: u32, threads: usize) -> (usize, usize) {
    let p = params(quality, lgwin, data.len());
    if threads == 1 || data.len() < PARALLEL_MIN_BYTES {
        let mut out = Vec::with_capacity(data.len() * 2 + 4096);
        let (result, peak) = measure(|| brotli::BrotliCompress(&mut &data[..], &mut out, &p));
        result.unwrap();
        assert!(out.capacity() >= out.len());
        (peak, out.len())
    } else {
        let size = brotli::enc::encode::BrotliEncoderMaxCompressedSizeMulti(data.len(), threads);
        let mut out = vec![0u8; size];
        let mut allocators = (0..threads)
            .map(|_| SendAlloc::new(StandardAlloc::default(), UnionHasher::Uninit))
            .collect::<Vec<_>>();
        struct Bytes(Vec<u8>);
        impl brotli::SliceWrapper<u8> for Bytes {
            fn slice(&self) -> &[u8] {
                &self.0
            }
        }
        let mut owned = brotli::enc::Owned::new(Bytes(data.to_vec()));
        let (result, peak) =
            measure(|| brotli::enc::compress_multi(&p, &mut owned, &mut out, &mut allocators));
        (peak, result.unwrap())
    }
}

#[test]
#[ignore = "heavy; run: cargo test --locked --release -p fontpixel-heap-check -- --ignored --nocapture"]
fn encoder_heap_never_exceeds_the_admitted_bound() {
    let quick = std::env::var_os("FONTPIXEL_HEAP_CHECK_QUICK").is_some();
    let qualities: Vec<u32> = if quick {
        vec![0, 2, 5, 9, 10, 11]
    } else {
        (0..=11).collect()
    };
    let mut inputs: Vec<(String, Vec<u8>)> = vec![
        ("empty".into(), Vec::new()),
        ("tiny".into(), b"fontpixel heap check".repeat(5)),
        ("64KiB-random".into(), xorshift(64 << 10, 0x1234_5678)),
        ("64KiB-structured".into(), structured(64 << 10)),
        ("160KiB-font-like".into(), font_like(160 << 10)),
        ("1MiB-structured".into(), structured(1 << 20)),
        ("1MiB-font-like".into(), font_like(1 << 20)),
        ("256KiB-vocabulary".into(), vocabulary(256 << 10)),
        ("1MiB-vocabulary".into(), vocabulary(1 << 20)),
        ("256KiB-mutating".into(), mutating_chunks(256 << 10)),
        ("1MiB-mutating".into(), mutating_chunks(1 << 20)),
        ("4MiB-mutating".into(), mutating_chunks(4 << 20)),
        ("256KiB-fibonacci".into(), fibonacci(256 << 10)),
        ("2MiB-fibonacci".into(), fibonacci(2 << 20)),
    ];
    // Colon-separated list of real fonts (e.g. full Tong TTF/OTF exports).
    if let Some(paths) = std::env::var_os("FONTPIXEL_HEAP_CHECK_FONT") {
        for path in std::env::split_paths(&paths) {
            let data = std::fs::read(&path).expect("FONTPIXEL_HEAP_CHECK_FONT is readable");
            inputs.push((format!("file:{}", path.to_string_lossy()), data));
        }
    } else {
        eprintln!("FONTPIXEL_HEAP_CHECK_FONT unset: multi-MB sfnt cases skipped");
    }
    eprintln!("input,len,quality,lgwin,threads,bound,observed,ratio,proven");
    let mut failures = Vec::new();
    for (name, data) in &inputs {
        for &quality in &qualities {
            let windows: &[u32] = if name == "64KiB-structured" {
                &[10, 16, 22, 24]
            } else {
                &[22]
            };
            for &lgwin in windows {
                for threads in [1, 2] {
                    let bound = brotli_encoder_heap_bound(quality, lgwin, data.len(), threads)
                        .expect("bound fits usize");
                    let proven =
                        brotli_encoder_proven_heap_bound(quality, lgwin, data.len(), threads)
                            .expect("bound fits usize");
                    let (peak, _) = observed(data, quality, lgwin, threads);
                    eprintln!(
                        "{name},{},{quality},{lgwin},{threads},{bound},{peak},{:.3},{proven}",
                        data.len(),
                        peak as f64 / bound as f64
                    );
                    assert!(proven >= bound || quality >= 10);
                    if peak > bound {
                        failures.push(format!(
                            "{name} q{quality} lgwin{lgwin} t{threads}: {peak} > {bound}"
                        ));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "bound exceeded:\n{}",
        failures.join("\n")
    );
}
