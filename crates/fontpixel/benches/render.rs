//! Criterion render hot paths, with all input preparation outside timed loops.
use criterion::{Criterion, criterion_group, criterion_main};
use fontpixel::bitmap::Bounds;
use fontpixel::model::{Codepoint, Vector};
use fontpixel::render::{GlyphOrder, RenderMode, RenderOptions};
use fontpixel::{Bitmap, Font, Glyph};
use std::hint::black_box;
fn bench(c: &mut Criterion) {
    let mut font = Font::default();
    font.metrics.bounds = Some(Bounds::new(8, 16, 0, 0));
    for cp in 32..128 {
        let mut b: Bitmap = Bitmap::new(8, 16).unwrap();
        for y in 0..16 {
            for x in 0..8 {
                if (x + y + cp) % 3 == 0 {
                    b.set_pixel(x, y, 1).unwrap();
                }
            }
        }
        let mut g = Glyph::new(b);
        g.codepoints.push(Codepoint::Unicode(cp));
        g.metrics.advance = Some(Vector { x: 8, y: 0 });
        font.add_glyph(g).unwrap();
    }
    let text = "ABCDEFGH".repeat(250);
    let opts = RenderOptions {
        line_limit: 640,
        ..Default::default()
    };
    c.bench_function("render/screen_80x25", |b| {
        b.iter(|| font.draw(black_box(&text), &opts).unwrap())
    });
    let opts = RenderOptions {
        mode: RenderMode::Cell,
        ..opts
    };
    c.bench_function("render/drawall_96", |b| {
        b.iter(|| font.draw_all(GlyphOrder::Ascending, &[], &opts).unwrap())
    });
}
criterion_group!(benches, bench);
criterion_main!(benches);
