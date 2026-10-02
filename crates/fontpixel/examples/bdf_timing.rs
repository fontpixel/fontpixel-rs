//! Five independent wall-clock BDF read/write samples; input I/O is excluded.
#[cfg(feature = "bdf")]
fn main() -> fontpixel::Result<()> {
    use fontpixel::{
        ResourceLimits,
        formats::bdf::{self, BdfWriteOptions},
    };
    use std::{hint::black_box, time::Instant};
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| fontpixel::Error::InvalidInput("usage: bdf_timing INPUT.bdf".into()))?;
    let bytes = std::fs::read(&path)?;
    let limits = ResourceLimits::default();
    let options = BdfWriteOptions::default();
    let (font, report) = bdf::read(&bytes, &limits)?;
    let _ = bdf::write(&font, &options)?;
    let mut reads = Vec::new();
    let mut writes = Vec::new();
    let mut outputs = 0;
    for _ in 0..5 {
        let start = Instant::now();
        let parsed = bdf::read(black_box(&bytes), &limits)?;
        reads.push(start.elapsed().as_secs_f64() * 1000.0);
        black_box(&parsed);
        drop(parsed);
    }
    for _ in 0..5 {
        let start = Instant::now();
        let written = bdf::write(black_box(&font), &options)?;
        writes.push(start.elapsed().as_secs_f64() * 1000.0);
        outputs = written.0.len();
        black_box(&written);
        drop(written);
    }
    println!(
        "{}",
        serde_json::json!({"path":path,"input_bytes":bytes.len(),"glyphs":font.len(),"report_entries":report.entries.len(),"output_bytes":outputs,"read_ms":reads,"write_ms":writes})
    );
    Ok(())
}
#[cfg(not(feature = "bdf"))]
fn main() {
    eprintln!("enable the bdf feature");
}
