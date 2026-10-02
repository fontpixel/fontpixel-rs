//! Five-run wall-clock YAFF write/read timing without filesystem I/O.
#[cfg(all(feature = "yaff", feature = "bdf"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use fontpixel::{
        FontSet, ResourceLimits,
        formats::{bdf, yaff},
    };
    use std::{hint::black_box, time::Instant};
    let path = std::env::args().nth(1).ok_or("supply BDF path")?;
    let input = std::fs::read(&path)?;
    let font = bdf::read(&input, &ResourceLimits::default())?.0;
    let set = FontSet::from(font);
    let options = yaff::YaffWriteOptions::default();
    let mut write = Vec::new();
    let mut read = Vec::new();
    let mut size = 0;
    for _ in 0..5 {
        let t = Instant::now();
        let bytes = black_box(yaff::write(black_box(&set), &options)?.0);
        write.push(t.elapsed().as_secs_f64() * 1000.);
        size = bytes.len();
        let t = Instant::now();
        let back = black_box(yaff::read(black_box(&bytes), &ResourceLimits::default())?.0);
        read.push(t.elapsed().as_secs_f64() * 1000.);
        assert_eq!(back, set);
    }
    let mut w = write.clone();
    w.sort_by(f64::total_cmp);
    let mut r = read.clone();
    r.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({"source":path,"glyphs":set.fonts[0].len(),"bytes":size,"write_ms":write,"read_ms":read,"write_median_ms":w[2],"read_median_ms":r[2],"exact_model":"pass"})
    );
    Ok(())
}
#[cfg(not(all(feature = "yaff", feature = "bdf")))]
fn main() {}
