//! Five independent wall-clock batches per operation; stdout is machine-readable JSON.
use fontpixel::bitmap::{Alignment, Bitmap, Direction};
use fontpixel::model::Codepoint;
use fontpixel::{Font, Glyph};
use std::{hint::black_box, time::Instant};
fn pattern(w: u32, h: u32) -> Bitmap {
    let mut b = Bitmap::new(w, h).unwrap();
    for y in 0..h {
        for x in 0..w {
            b.set_pixel(x, y, u8::from((x * 13 + y * 7) % 11 < 4))
                .unwrap();
        }
    }
    b
}
fn measure(name: &str, count: u32, mut action: impl FnMut()) -> serde_json::Value {
    for _ in 0..count.min(100) {
        action();
    }
    let mut samples = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        for _ in 0..count {
            action();
        }
        samples.push(start.elapsed().as_nanos() as f64 / f64::from(count));
    }
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    serde_json::json!({"name":name,"iterations_per_batch":count,"samples_ns":samples,"median_ns":sorted[2]})
}
fn main() {
    let small = pattern(64, 64);
    let glyph = pattern(14, 14);
    let large = pattern(4096, 1024);
    let mut canvas = Bitmap::new(1120, 350).unwrap();
    let mut dst = large.clone();
    let mut results = vec![
        measure("crop_64x64_to_31x31", 100_000, || {
            black_box(small.crop(31, 31, 3, 5).unwrap());
        }),
        measure("concat_two_64x64", 20_000, || {
            black_box(
                small
                    .concat(&small, Direction::Right, Alignment::End, 0)
                    .unwrap(),
            );
        }),
        measure("concat_80_glyphs", 5_000, || {
            black_box(
                Bitmap::concat_all(
                    &[&glyph; 80],
                    Direction::Right,
                    Alignment::End,
                    &[],
                    glyph.limits(),
                )
                .unwrap(),
            );
        }),
        measure("paste_screen_80x25", 200, || {
            for y in 0..25 {
                for x in 0..80 {
                    canvas.paste(black_box(&glyph), x * 14, y * 14);
                }
            }
            black_box(canvas.as_bytes());
        }),
        measure("crop_4096x1024", 100, || {
            black_box(large.crop(4000, 1000, 3, 5).unwrap());
        }),
        measure("overlay_4096x1024_unaligned", 100, || {
            dst.paste(black_box(&large), 3, -2);
            black_box(dst.as_bytes());
        }),
        measure("bold_64x64_radius8", 10_000, || {
            black_box(small.bold(8).unwrap());
        }),
        measure("rotate_64x64", 20_000, || {
            black_box(small.rotate(90).unwrap());
        }),
    ];
    let mut font = Font::default();
    for cp in 0..38_653 {
        let mut g = Glyph::new(glyph.clone());
        g.codepoints.push(Codepoint::Unicode(cp));
        font.add_glyph(g).unwrap();
    }
    let mut cp = 0u32;
    results.push(measure("cmap_lookup_38653", 1_000_000, || {
        cp = (cp + 997) % 38_653;
        black_box(font.glyph_for_codepoint(Codepoint::Unicode(cp)));
    }));
    println!("{}", serde_json::to_string_pretty(&results).unwrap());
}
