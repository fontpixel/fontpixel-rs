//! Five-sample resident-byte BDF->PCF timings, including end-to-end conversion.
#[cfg(all(feature = "bdf", feature = "pcf"))]
fn main() -> fontpixel::Result<()> {
    use fontpixel::{
        ResourceLimits,
        formats::{
            bdf,
            pcf::{self, PcfWriteOptions},
        },
    };
    use std::{hint::black_box, time::Instant};
    let path = std::env::args().nth(1).ok_or_else(|| {
        fontpixel::Error::InvalidInput("usage: pcf_timing INPUT.bdf [OUTPUT.pcf]".into())
    })?;
    let bytes = std::fs::read(&path)?;
    let limits = ResourceLimits::default();
    let options = PcfWriteOptions::default();
    let (font, _) = bdf::read(&bytes, &limits)?;
    let (wire, _) = pcf::write(&font, &options)?;
    if let Some(path) = std::env::args().nth(2) {
        std::fs::write(path, &wire)?;
    }
    let mut parse = Vec::new();
    let mut write = Vec::new();
    let mut read = Vec::new();
    let mut convert = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        let f = bdf::read(black_box(&bytes), &limits)?;
        parse.push(start.elapsed().as_secs_f64() * 1000.0);
        black_box(&f);
        drop(f);
    }
    for _ in 0..5 {
        let start = Instant::now();
        let b = pcf::write(black_box(&font), &options)?;
        write.push(start.elapsed().as_secs_f64() * 1000.0);
        black_box(&b);
        drop(b);
    }
    for _ in 0..5 {
        let start = Instant::now();
        let f = pcf::read(black_box(&wire), &limits)?;
        read.push(start.elapsed().as_secs_f64() * 1000.0);
        black_box(&f);
        drop(f);
    }
    for _ in 0..5 {
        let start = Instant::now();
        let (f, _) = bdf::read(black_box(&bytes), &limits)?;
        let b = pcf::write(&f, &options)?;
        convert.push(start.elapsed().as_secs_f64() * 1000.0);
        black_box(&b);
        drop(b);
        drop(f);
    }
    println!(
        "{}",
        serde_json::json!({"path":path,"glyphs":font.len(),"input_bytes":bytes.len(),"pcf_bytes":wire.len(),"bdf_parse_ms":parse,"pcf_write_ms":write,"pcf_read_ms":read,"bdf_to_pcf_ms":convert})
    );
    Ok(())
}
#[cfg(not(all(feature = "bdf", feature = "pcf")))]
fn main() {
    eprintln!("enable bdf and pcf features");
}
