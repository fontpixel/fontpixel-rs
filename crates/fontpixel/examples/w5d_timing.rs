//! Apple/Qt group native snapshots, canonical rewrites, and quiet timing.
use fontpixel::formats::{fbit, lisa, mgtk, nfnt, qpf, qpf2};
use fontpixel::model::GlyphBitmap;
use fontpixel::{FontSet, Report, ResourceLimits};
use std::{hint::black_box, time::Instant};
fn read(format: &str, data: &[u8]) -> fontpixel::Result<(FontSet, Report)> {
    let l = ResourceLimits::default();
    match format {
        "fbit" | "hfnt" => fbit::read(
            data,
            &fbit::FbitReadOptions {
                kind: if format == "hfnt" {
                    fbit::ResourceKind::Hfnt
                } else {
                    fbit::ResourceKind::Fbit
                },
                ..Default::default()
            },
            &l,
        ),
        "lisa" => lisa::read(data, &Default::default(), &l),
        "mgtk" => mgtk::read(data, &Default::default(), &l),
        "qpf" => qpf::read(data, &Default::default(), &l),
        "qpf2" => qpf2::read(data, &Default::default(), &l),
        "binhex" | "nfnt" => nfnt::read(data, &Default::default(), &l),
        _ => Err(fontpixel::Error::InvalidInput("unknown W5.D format".into())),
    }
}
fn write(format: &str, set: &FontSet, choice: &str) -> fontpixel::Result<(Vec<u8>, Report)> {
    match format {
        "fbit" | "hfnt" => fbit::write(
            set,
            &fbit::FbitWriteOptions {
                kind: if format == "hfnt" {
                    fbit::ResourceKind::Hfnt
                } else {
                    fbit::ResourceKind::Fbit
                },
                container: match choice {
                    "macbinary" => nfnt::Container::MacBinary,
                    "binhex" => nfnt::Container::BinHex,
                    "dfont" => nfnt::Container::Dfont,
                    "applesingle" => nfnt::Container::AppleSingle,
                    _ => nfnt::Container::Bare,
                },
                ..Default::default()
            },
        ),
        "lisa" => lisa::write(
            set,
            &lisa::LisaWriteOptions {
                compression: match choice {
                    "compressed" => lisa::Compression::Compressed,
                    "uncompressed" => lisa::Compression::Uncompressed,
                    _ => lisa::Compression::Preserve,
                },
                ..Default::default()
            },
        ),
        "mgtk" => mgtk::write(
            set,
            &mgtk::MgtkWriteOptions {
                columns: match choice {
                    "single" => mgtk::Columns::Single,
                    "double" => mgtk::Columns::Double,
                    _ => mgtk::Columns::Auto,
                },
                ..Default::default()
            },
        ),
        "qpf" => qpf::write(set, &Default::default()),
        "qpf2" => qpf2::write(set, &Default::default()),
        "binhex" | "nfnt" => nfnt::write(
            set,
            &nfnt::NfntWriteOptions {
                container: if format == "binhex" {
                    nfnt::Container::BinHex
                } else {
                    nfnt::Container::Bare
                },
                ..Default::default()
            },
        ),
        _ => Err(fontpixel::Error::InvalidInput("unknown W5.D format".into())),
    }
}
fn snapshot(set: &FontSet, report: &Report) -> serde_json::Value {
    serde_json::json!({"loss":report.has_loss(),"reports":report.entries.iter().map(|e|e.code.as_str()).collect::<Vec<_>>(),"fonts":set.fonts.iter().map(|f|serde_json::json!({"family":f.names.family,"name":f.names.full,"ascent":f.metrics.ascent,"descent":f.metrics.descent,"leading":f.metrics.line_gap,"slots":f.slot_count(),"glyphs":f.glyphs().map(|(id,g)|serde_json::json!({"id":id.index(),"codes":g.codepoints.iter().map(|c|c.value()).collect::<Vec<_>>(),"gray":matches!(g.bitmap,GlyphBitmap::Gray8(_)),"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|a|a.x),"pixels":g.bitmap.as_bytes().iter().map(|v|format!("{v:02x}")).collect::<String>()})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 4 {
        return Err("usage: w5d_timing inspect|rewrite|canonical|time|time-canonical FORMAT INPUT [OUTPUT|ITERATIONS] [CHOICE]".into());
    }
    let format = &args[2];
    let data = std::fs::read(&args[3])?;
    let (mut set, report) = read(format, &data)?;
    let choice = args.get(5).map(String::as_str).unwrap_or("default");
    if matches!(args[1].as_str(), "canonical" | "time-canonical") {
        for prefix in ["fbit", "lisa", "mgtk", "qpf", "qpf2", "nfnt"] {
            let key = format!("{prefix}.source");
            set.properties.remove(&key);
            for f in &mut set.fonts {
                f.properties.remove(&key);
            }
        }
    }
    match args[1].as_str() {
        "inspect" => println!("{}", snapshot(&set, &report)),
        "rewrite" | "canonical" => {
            let (out, report) = write(format, &set, choice)?;
            std::fs::write(&args[4], out)?;
            println!(
                "{}",
                serde_json::json!({"loss":report.has_loss(),"reports":report.entries.iter().map(|e|e.code.as_str()).collect::<Vec<_>>()})
            );
        }
        "time" | "time-canonical" => {
            let count = args
                .get(4)
                .map(|v| v.parse::<usize>())
                .transpose()?
                .unwrap_or(100);
            if count == 0 {
                return Err("positive iteration count required".into());
            }
            let (sample, _) = write(format, &set, choice)?;
            read(format, &sample)?;
            for _ in 0..3 {
                black_box(read(format, &data)?);
                black_box(write(format, &set, choice)?);
            }
            let t = Instant::now();
            for _ in 0..count {
                black_box(read(format, black_box(&data))?);
            }
            let read_seconds = t.elapsed().as_secs_f64();
            let t = Instant::now();
            for _ in 0..count {
                black_box(write(format, black_box(&set), choice)?);
            }
            let write_seconds = t.elapsed().as_secs_f64();
            println!(
                "{}",
                serde_json::json!({"format":format,"choice":choice,"canonical":args[1]=="time-canonical","iterations":count,"input_bytes":data.len(),"output_bytes":sample.len(),"fonts":set.fonts.len(),"glyphs":set.fonts.iter().map(|f|f.len()).sum::<usize>(),"read_seconds":read_seconds,"write_seconds":write_seconds})
            );
        }
        _ => return Err("unknown operation".into()),
    }
    Ok(())
}
