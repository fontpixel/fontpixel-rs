//! NFNT crosscheck driver and repeatable native read/write timing workload.
use fontpixel::formats::nfnt::{self, Container, NfntReadOptions, NfntWriteOptions, ResourceType};
use fontpixel::{FontSet, ResourceLimits};
use std::{hint::black_box, time::Instant};
fn container(s: &str) -> Container {
    match s {
        "bare" => Container::Bare,
        "macbinary" => Container::MacBinary,
        "applesingle" => Container::AppleSingle,
        "appledouble" => Container::AppleDouble,
        "font" | "dfont" => Container::Dfont,
        _ => panic!("unknown container"),
    }
}
fn snapshot(set: &FontSet) -> serde_json::Value {
    serde_json::json!({"fonts":set.fonts.iter().map(|f|serde_json::json!({"family":f.names.family,"style":f.names.style,"ascent":f.metrics.ascent,"descent":f.metrics.descent,"point":f.metrics.point_size.map(|v|v.numerator()/i64::from(v.denominator())),"glyphs":f.glyphs().map(|(_,g)|serde_json::json!({"codes":g.codepoints.iter().map(|c|c.value()).collect::<Vec<_>>(),"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|v.x),"pixels":g.bitmap.as_bytes().iter().map(|v|format!("{v:02x}")).collect::<String>()})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    let input = std::fs::read(&args[2])?;
    if args[1] == "detect" {
        println!("{:?}", nfnt::detect(&input));
        return Ok(());
    }
    let (set, r) = nfnt::read(
        &input,
        &NfntReadOptions::default(),
        &ResourceLimits::default(),
    )?;
    match args[1].as_str() {
        "inspect" => println!("{}", snapshot(&set)),
        "rewrite" => {
            let o = NfntWriteOptions {
                container: container(&args[4]),
                resource_type: if args[4] == "font" {
                    ResourceType::Font
                } else {
                    ResourceType::Nfnt
                },
                ..Default::default()
            };
            let (bytes, report) = nfnt::write(&set, &o)?;
            std::fs::write(&args[3], bytes)?;
            println!("{}", report.entries.len());
        }
        "time" => {
            let n = args
                .get(3)
                .map(|s| s.parse::<usize>())
                .transpose()?
                .unwrap_or(1000);
            let start = Instant::now();
            for _ in 0..n {
                black_box(nfnt::read(
                    black_box(&input),
                    &Default::default(),
                    &Default::default(),
                )?);
            }
            let read = start.elapsed().as_secs_f64();
            let start = Instant::now();
            for _ in 0..n {
                black_box(nfnt::write(black_box(&set), &Default::default())?);
            }
            let write = start.elapsed().as_secs_f64();
            println!(
                "{{\"iterations\":{n},\"input_bytes\":{},\"fonts\":{},\"glyphs\":{},\"read_seconds\":{read},\"write_seconds\":{write},\"read_loss\":{}}}",
                input.len(),
                set.fonts.len(),
                set.fonts.iter().map(|f| f.len()).sum::<usize>(),
                r.has_loss()
            );
        }
        _ => panic!("detect|inspect|rewrite|time required"),
    };
    Ok(())
}
