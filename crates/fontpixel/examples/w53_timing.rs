//! W5.3 oracle adapter and in-memory timing harness. Run release builds for timings.
use fontpixel::formats::{figlet, fzx, vfont};
use fontpixel::model::{FontSet, GlyphBitmap};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{ResourceLimits, Result, WriteOptions};
use std::hint::black_box;
fn read(fmt: &str, bytes: &[u8]) -> Result<FontSet> {
    let input = Input::new(bytes);
    let limits = ResourceLimits::default();
    let mut context = ReadContext::new(&input, &limits)?;
    Ok(match fmt {
        "vfont" => vfont::read(&mut context, &Default::default())?,
        "fzx" => fzx::read(&mut context, &Default::default())?,
        "figlet" => figlet::read(&mut context, &Default::default())?,
        _ => return Err(fontpixel::Error::InvalidInput("unknown W5.3 format".into())),
    }
    .0)
}
fn write(fmt: &str, set: &FontSet) -> Result<Vec<u8>> {
    let o = WriteOptions::new(ResourcePath::new(format!("font.{fmt}"))?);
    match fmt {
        "vfont" => vfont::write(set, &o, &Default::default())?,
        "fzx" => fzx::write(set, &o, &Default::default())?,
        "figlet" => figlet::write(set, &o, &Default::default())?,
        _ => return Err(fontpixel::Error::InvalidInput("unknown W5.3 format".into())),
    }
    .output
    .into_single()
}
fn dump(s: &FontSet) {
    let out=s.fonts.iter().map(|f|serde_json::json!({
 "ascent":f.metrics.ascent,"descent":f.metrics.descent,"line_height":f.metrics.line_height,"space_width":f.metrics.space_width,
 "glyphs":f.glyphs().map(|(_,g)|{assert!(matches!(g.bitmap,GlyphBitmap::Mono(_)));serde_json::json!({"codes":g.codepoints.iter().map(|c|c.value()).collect::<Vec<_>>(),"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|v.x),"left":g.metrics.left_bearing,"right":g.metrics.right_bearing,"bytes":g.bitmap.as_bytes()})}).collect::<Vec<_>>()
})).collect::<Vec<_>>();
    println!("{}", serde_json::to_string(&out).unwrap());
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let a = std::env::args().skip(1).collect::<Vec<_>>();
    if a.len() < 3 {
        return Err(
            "usage: w53_timing dump FORMAT INPUT | write FORMAT INPUT OUTPUT | bench FORMAT INPUT"
                .into(),
        );
    }
    let bytes = std::fs::read(&a[2])?;
    let set = read(&a[1], &bytes)?;
    match a[0].as_str() {
        "dump" => dump(&set),
        "write" => std::fs::write(a.get(3).ok_or("missing output path")?, write(&a[1], &set)?)?,
        "bench" => {
            let output = write(&a[1], &set)?;
            let round = read(&a[1], &output)?;
            assert_eq!(
                write(&a[1], &round)?,
                output,
                "canonical output must be stable"
            );
            for _ in 0..10 {
                black_box(read(&a[1], &bytes)?);
                black_box(write(&a[1], &set)?);
            }
            let iterations = 1000;
            let start = std::time::Instant::now();
            for _ in 0..iterations {
                black_box(read(&a[1], &bytes)?);
            }
            let read_us = start.elapsed().as_secs_f64() * 1e6 / f64::from(iterations);
            let start = std::time::Instant::now();
            for _ in 0..iterations {
                black_box(write(&a[1], &set)?);
            }
            let write_us = start.elapsed().as_secs_f64() * 1e6 / f64::from(iterations);
            println!(
                "{}",
                serde_json::json!({"format":a[1],"glyphs":set.single()?.len(),"input_bytes":bytes.len(),"output_bytes":output.len(),"iterations":iterations,"read_us":read_us,"write_us":write_us})
            );
        }
        _ => return Err("unknown command".into()),
    };
    Ok(())
}
