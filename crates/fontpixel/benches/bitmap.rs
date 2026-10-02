//! Criterion microbenchmarks; all fixture construction is outside measured loops.
use criterion::{Criterion, criterion_group, criterion_main};
use fontpixel::bitmap::{Alignment, Bitmap, Direction};
use std::{hint::black_box, time::Duration};

fn pattern(w: u32, h: u32) -> Bitmap {
    let mut bitmap = Bitmap::new(w, h).unwrap();
    for y in 0..h {
        for x in 0..w {
            bitmap
                .set_pixel(x, y, u8::from((x * 13 + y * 7) % 11 < 4))
                .unwrap();
        }
    }
    bitmap
}
fn benchmarks(c: &mut Criterion) {
    let small = pattern(64, 64);
    let glyph = pattern(14, 14);
    let large = pattern(4096, 1024);
    c.bench_function("crop_64x64_to_31x31", |b| {
        b.iter(|| black_box(&small).crop(31, 31, 3, 5).unwrap())
    });
    c.bench_function("concat_two_64x64", |b| {
        b.iter(|| {
            black_box(&small)
                .concat(&small, Direction::Right, Alignment::End, 0)
                .unwrap()
        })
    });
    c.bench_function("concat_80_glyphs", |b| {
        b.iter(|| {
            Bitmap::concat_all(
                black_box(&[&glyph; 80]),
                Direction::Right,
                Alignment::End,
                &[],
                glyph.limits(),
            )
            .unwrap()
        })
    });
    let mut canvas = Bitmap::new(1120, 350).unwrap();
    c.bench_function("paste_screen_80x25", |b| {
        b.iter(|| {
            for y in 0..25 {
                for x in 0..80 {
                    canvas.paste(black_box(&glyph), x * 14, y * 14);
                }
            }
            black_box(canvas.as_bytes());
        })
    });
    c.bench_function("crop_4096x1024", |b| {
        b.iter(|| black_box(&large).crop(4000, 1000, 3, 5).unwrap())
    });
    let mut dst = large.clone();
    c.bench_function("overlay_4096x1024_unaligned", |b| {
        b.iter(|| {
            dst.paste(black_box(&large), 3, -2);
            black_box(dst.as_bytes());
        })
    });
    c.bench_function("bold_64x64_radius8", |b| {
        b.iter(|| black_box(&small).bold(8).unwrap())
    });
    c.bench_function("rotate_64x64", |b| {
        b.iter(|| black_box(&small).rotate(90).unwrap())
    });
}
criterion_group! { name=benches; config=Criterion::default().sample_size(20).warm_up_time(Duration::from_millis(200)).measurement_time(Duration::from_secs(1)); targets=benchmarks }
criterion_main!(benches);
