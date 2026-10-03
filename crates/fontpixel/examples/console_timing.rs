//! Reproducible five-run medians for W3.1, including independent-font concurrency.
use fontpixel::ResourceLimits;
use fontpixel::formats::{
    hex::{self, HexReadOptions, HexWriteOptions},
    psf::{self, PsfWriteOptions},
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
fn median(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[2]
}
fn bench(f: impl Fn()) -> f64 {
    median(
        (0..5)
            .map(|_| {
                let t = Instant::now();
                f();
                t.elapsed().as_secs_f64() * 1000.
            })
            .collect(),
    )
}
fn batch(bytes: &[u8], kind: &str, workers: usize) {
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let next = &next;
            scope.spawn(move || {
                while next.fetch_add(1, Ordering::Relaxed) < 8 {
                    if kind == "hex" {
                        let (f, _) = hex::read(
                            bytes,
                            &HexReadOptions::default(),
                            &ResourceLimits::default(),
                        )
                        .unwrap();
                        let (out, _) = hex::write(&f, &HexWriteOptions::default()).unwrap();
                        assert_eq!(bytes, out);
                    } else {
                        let (f, _) = psf::read(bytes, &ResourceLimits::default()).unwrap();
                        let (out, _) = psf::write(&f, &PsfWriteOptions::default()).unwrap();
                        assert_eq!(bytes, out);
                    }
                }
            });
        }
    });
}
fn main() {
    let mut hex_bytes = String::new();
    let mut count = 0;
    for cp in 0..=0x10ffffu32 {
        if char::from_u32(cp).is_none() {
            continue;
        }
        hex_bytes.push_str(&format!("{cp:04X}:{}\n", "815A244281A5DB00".repeat(4)));
        count += 1;
        if count == 38653 {
            break;
        }
    }
    let h = hex_bytes.as_bytes();
    let (hf, _) = hex::read(h, &HexReadOptions::default(), &ResourceLimits::default()).unwrap();
    let p = psf::write(&hf, &PsfWriteOptions::default()).unwrap().0;
    let pf = psf::read(&p, &ResourceLimits::default()).unwrap().0;
    let hr = bench(|| {
        assert_eq!(
            hex::read(h, &HexReadOptions::default(), &ResourceLimits::default())
                .unwrap()
                .0
                .len(),
            38653
        );
    });
    let hw = bench(|| {
        assert_eq!(hex::write(&hf, &HexWriteOptions::default()).unwrap().0, h);
    });
    let pr = bench(|| {
        assert_eq!(
            psf::read(&p, &ResourceLimits::default()).unwrap().0.len(),
            38653
        );
    });
    let pw = bench(|| {
        assert_eq!(psf::write(&pf, &PsfWriteOptions::default()).unwrap().0, p);
    });
    println!(
        "{{\"glyphs\":38653,\"cell\":[16,16],\"runs\":5,\"available_parallelism\":{},\"hex_bytes\":{},\"psf_bytes\":{},\"median_ms\":{{\"hex_read\":{hr:.3},\"hex_write\":{hw:.3},\"psf_read\":{pr:.3},\"psf_write\":{pw:.3}}},\"eight_font_batch_ms\":[",
        std::thread::available_parallelism().unwrap().get(),
        h.len(),
        p.len()
    );
    for (i, workers) in [1, 2, 4, 8].into_iter().enumerate() {
        let h = bench(|| batch(h, "hex", workers));
        let p = bench(|| batch(&p, "psf", workers));
        println!(
            "{}{{\"workers\":{workers},\"hex\":{h:.3},\"psf\":{p:.3}}}",
            if i == 0 { "" } else { "," }
        );
    }
    println!("]}}");
}
