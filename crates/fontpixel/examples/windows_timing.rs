//! Reproducible W3.2 five-run medians, including independent-font concurrency.
use fontpixel::formats::{
    fnt::{self, FntWriteOptions, Version},
    fon,
};
use fontpixel::model::{Codepoint, Vector};
use fontpixel::{Bitmap, Font, FontSet, Glyph, ResourceLimits};
use std::{
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
fn bench(f: impl Fn()) -> f64 {
    let mut times = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        f();
        times.push(start.elapsed().as_secs_f64() * 1000.);
    }
    times.sort_by(f64::total_cmp);
    times[2]
}
fn main() {
    let mut f = Font::default();
    f.charset.name = Some("cp1252".into());
    f.names.family = Some("Benchmark".into());
    f.metrics.ascent = Some(13);
    f.metrics.descent = Some(3);
    for cp in 0..256u32 {
        let data: Vec<u8> = (0..16)
            .map(|row| ((cp * 17 + row * 73) & 255) as u8)
            .collect();
        let mut g = Glyph::new(
            Bitmap::<fontpixel::bitmap::Mono>::from_raw(8, 16, &data, Default::default()).unwrap(),
        );
        g.codepoints.push(Codepoint::Legacy(cp));
        g.metrics.bounds.y = -3;
        g.metrics.advance = Some(Vector::new(8, 0));
        f.add_glyph(g).unwrap();
    }
    println!(
        "{{\"runs\":5,\"glyphs\":256,\"cell\":[8,16],\"iterations\":1000,\"available_parallelism\":{},\"formats\":[",
        std::thread::available_parallelism().unwrap().get()
    );
    let limits = ResourceLimits::default();
    for (i, version) in [Version::One, Version::Two, Version::Three]
        .into_iter()
        .enumerate()
    {
        let options = FntWriteOptions {
            version: Some(version),
            ..Default::default()
        };
        let bytes = fnt::write(&f, &options).unwrap().0;
        let source = fnt::read_default(&bytes, &limits).unwrap().0;
        let read = bench(|| {
            for _ in 0..1000 {
                black_box(fnt::read_default(black_box(&bytes), &limits).unwrap());
            }
        });
        let write = bench(|| {
            for _ in 0..1000 {
                black_box(fnt::write(black_box(&source), &options).unwrap());
            }
        });
        println!(
            "{}{{\"format\":\"fnt{}\",\"bytes\":{},\"read_1000_ms\":{read:.3},\"write_1000_ms\":{write:.3}}}",
            if i > 0 { "," } else { "" },
            i + 1,
            bytes.len()
        );
    }
    let source = fnt::read_default(&fnt::write(&f, &Default::default()).unwrap().0, &limits)
        .unwrap()
        .0;
    let set = FontSet {
        fonts: vec![source; 8],
        ..Default::default()
    };
    let bytes = fon::write(&set, &Default::default()).unwrap().0;
    let read = bench(|| {
        for _ in 0..1000 {
            black_box(fon::read(black_box(&bytes), &Default::default(), &limits).unwrap());
        }
    });
    let write = bench(|| {
        for _ in 0..1000 {
            black_box(fon::write(black_box(&set), &Default::default()).unwrap());
        }
    });
    println!(
        ",{{\"format\":\"ne-eight-font\",\"bytes\":{},\"read_1000_ms\":{read:.3},\"write_1000_ms\":{write:.3}}}],\"batch_1000_ne_ms\":[",
        bytes.len()
    );
    for (i, workers) in [1, 2, 4].into_iter().enumerate() {
        let elapsed = bench(|| {
            let next = AtomicUsize::new(0);
            std::thread::scope(|s| {
                for _ in 0..workers {
                    let next = &next;
                    let bytes = &bytes;
                    let limits = &limits;
                    s.spawn(move || {
                        while next.fetch_add(1, Ordering::Relaxed) < 1000 {
                            let (f, _) = fon::read(bytes, &Default::default(), limits).unwrap();
                            let (out, _) = fon::write(&f, &Default::default()).unwrap();
                            assert_eq!(&out, bytes);
                        }
                    });
                }
            });
        });
        println!(
            "{}{{\"workers\":{workers},\"median_ms\":{elapsed:.3}}}",
            if i > 0 { "," } else { "" }
        );
    }
    println!("]}}");
}
