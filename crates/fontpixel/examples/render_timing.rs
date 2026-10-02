//! Five wall-clock samples of single rendering calls, after parse/warmup.
#[cfg(feature = "bdf")]
fn main() -> fontpixel::Result<()> {
    use fontpixel::render::{GlyphOrder, RenderMode, RenderOptions};
    use std::{hint::black_box, time::Instant};
    let path = std::env::args().nth(1).expect("INPUT.bdf");
    let (font, _) = fontpixel::formats::bdf::read(&std::fs::read(&path)?, &Default::default())?;
    let text = "永".repeat(2000);
    let options = RenderOptions {
        line_limit: 80 * i64::from(font.metrics.bounds.unwrap().width),
        mode: RenderMode::Cell,
        ..Default::default()
    };
    let mut all_options = RenderOptions {
        mode: RenderMode::Cell,
        ..Default::default()
    };
    all_options.limits.bitmap.max_pixels = 64 * 1024 * 1024;
    let screen = font.draw(&text, &options)?;
    if screen.report.has_loss() {
        return Err(fontpixel::Error::StrictLoss(screen.report));
    }
    let all = font.draw_all(GlyphOrder::Ascending, &[], &all_options)?;
    let (mut screens, mut drawall) = (Vec::new(), Vec::new());
    for _ in 0..5 {
        let start = Instant::now();
        let out = font.draw(black_box(&text), &options)?;
        screens.push(start.elapsed().as_secs_f64() * 1000.);
        black_box(&out);
    }
    for _ in 0..5 {
        let start = Instant::now();
        let out = font.draw_all(GlyphOrder::Ascending, &[], &all_options)?;
        drawall.push(start.elapsed().as_secs_f64() * 1000.);
        black_box(&out);
    }
    println!(
        "{}",
        serde_json::json!({"path":path,"glyphs":font.len(),"screen_characters":2000,"screen_width":screen.bitmap.width(),"screen_height":screen.bitmap.height(),"drawall_width":all.bitmap.width(),"drawall_height":all.bitmap.height(),"screen_ms":screens,"drawall_ms":drawall})
    );
    Ok(())
}
#[cfg(not(feature = "bdf"))]
fn main() {
    eprintln!("enable bdf");
}
