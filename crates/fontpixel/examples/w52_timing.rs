//! Native W5.2 snapshots, reproducible rewrites and isolated read/write timing.
use fontpixel::formats::{hanfont, iigs};
use fontpixel::{FontSet, Report, ResourceLimits};
use std::{hint::black_box, time::Instant};
fn read(format: &str, data: &[u8]) -> fontpixel::Result<(FontSet, Report)> {
    if format == "iigs" {
        iigs::read(data, &Default::default(), &ResourceLimits::default())
    } else {
        hanfont::read(
            data,
            &hanfont::HanfontReadOptions {
                mode: if format == "components" {
                    hanfont::ReadMode::Components
                } else {
                    hanfont::ReadMode::Syllables
                },
                ..Default::default()
            },
            &ResourceLimits::default(),
        )
    }
}
fn write(format: &str, s: &FontSet, choice: &str) -> fontpixel::Result<(Vec<u8>, Report)> {
    if format == "iigs" {
        iigs::write(
            s,
            &iigs::IigsWriteOptions {
                container: if choice == "binscii" {
                    iigs::Container::BinScii
                } else {
                    iigs::Container::Bare
                },
                version: match choice {
                    "v1.1" => iigs::Version::V1_1,
                    "v1.5" => iigs::Version::V1_5,
                    _ => iigs::Version::Auto,
                },
                ..Default::default()
            },
        )
    } else {
        hanfont::write(s, &Default::default())
    }
}
fn snapshot(s: &FontSet) -> serde_json::Value {
    serde_json::json!({"fonts":s.fonts.iter().map(|f|serde_json::json!({"family":f.names.family,"style":f.names.style,"ascent":f.metrics.ascent,"descent":f.metrics.descent,"point":f.metrics.point_size.map(|p|p.numerator()/i64::from(p.denominator())),"glyphs":f.glyphs().map(|(_,g)|serde_json::json!({"codes":g.codepoints.iter().map(|c|c.value()).collect::<Vec<_>>(),"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|a|a.x),"pixels":g.bitmap.as_bytes().iter().map(|v|format!("{v:02x}")).collect::<String>()})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).map(String::as_str) == Some("slots") {
        println!(
            "{}",
            serde_json::json!(
                (0xac00..=0xd7a3)
                    .map(|cp| hanfont::component_slots(cp).unwrap())
                    .collect::<Vec<_>>()
            )
        );
        return Ok(());
    }
    if args.len() < 4 {
        return Err("usage: w52_timing inspect|rewrite|canonical|time|time-canonical FORMAT INPUT [OUTPUT|ITERATIONS] [bare|v1.1|v1.5|binscii]".into());
    }
    let format = &args[2];
    let input = std::fs::read(&args[3])?;
    let (mut set, _) = read(format, &input)?;
    match args[1].as_str(){
        "inspect"=>println!("{}",snapshot(&set)),
        "rewrite"|"canonical"=>{
            if args[1]=="canonical" {set.fonts[0].properties.remove("iigs.source");}
            let (out,report)=write(format,&set,args.get(5).map(String::as_str).unwrap_or("bare"))?;
            std::fs::write(&args[4],out)?;println!("{}",serde_json::json!({"loss":report.has_loss(),"reports":report.entries.iter().map(|e|e.code.as_str()).collect::<Vec<_>>()}));
        },
        "time"|"time-canonical"=>{
            let count=args.get(4).map(|s|s.parse::<usize>()).transpose()?.unwrap_or(100);
            if count == 0 { return Err("iteration count must be positive".into()); }
            if args[1] == "time-canonical" {set.fonts[0].properties.remove("iigs.source");}
            let choice=args.get(5).map(String::as_str).unwrap_or("bare");
            let (sample, _)=write(format,&set,choice)?;
            read(format,&sample)?;
            for _ in 0..3 {black_box(read(format,&input)?); black_box(write(format,&set,choice)?);}
            let t=Instant::now();for _ in 0..count {black_box(read(format,black_box(&input))?);}let read_seconds=t.elapsed().as_secs_f64();
            let t=Instant::now();for _ in 0..count {black_box(write(format,black_box(&set),choice)?);}let write_seconds=t.elapsed().as_secs_f64();
            println!("{}",serde_json::json!({"format":format,"write_choice":choice,"canonical":args[1]=="time-canonical","iterations":count,"input_bytes":input.len(),"output_bytes":sample.len(),"glyphs":set.fonts[0].len(),"read_seconds":read_seconds,"write_seconds":write_seconds}));
        },
        _=>return Err("usage: w52_timing inspect|rewrite|canonical|time FORMAT INPUT [OUTPUT|ITERATIONS] [bare|v1.1|v1.5|binscii]".into()),
    }
    Ok(())
}
