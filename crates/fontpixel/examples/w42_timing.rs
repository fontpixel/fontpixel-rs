//! W4.2 reproducible native conversion/signature bridge and in-memory timing.
use fontpixel::formats::{bdf, fontx2, hzk, pf2};
use fontpixel::model::{Codepoint, Font};
use fontpixel::{Report, ResourceLimits, Result};
use std::path::Path;
fn decode(id: &str, bytes: &[u8], preset: Option<hzk::Preset>) -> Result<(Font, Report)> {
    match id {
        "bdf" => bdf::read(bytes, &ResourceLimits::default()),
        "pf2" => pf2::read(bytes, &Default::default(), &Default::default()),
        "fontx2" => fontx2::read(bytes, &Default::default(), &Default::default()),
        "hzk" => hzk::read(
            bytes,
            &hzk::HzkReadOptions {
                preset,
                ..Default::default()
            },
            &Default::default(),
        ),
        _ => Err(fontpixel::Error::InvalidInput(
            "unknown bridge format".into(),
        )),
    }
}
fn encode(id: &str, f: &Font, preset: Option<hzk::Preset>) -> Result<(Vec<u8>, Report)> {
    match id {
        "bdf" => bdf::write(f, &Default::default()),
        "pf2" => pf2::write(f, &Default::default()),
        "fontx2" => fontx2::write(f, &Default::default()),
        "hzk" => hzk::write(
            f,
            &hzk::HzkWriteOptions {
                preset,
                ..Default::default()
            },
        ),
        _ => Err(fontpixel::Error::InvalidInput(
            "unknown bridge format".into(),
        )),
    }
}
fn signature(f: &Font) -> serde_json::Value {
    serde_json::json!({"charset":f.charset.canonical_name(),"count":f.len(),"glyphs":f.glyphs().map(|(id,g)|serde_json::json!({"id":id.index(),"codes":g.codepoints.iter().map(|cp|serde_json::json!([if matches!(cp,Codepoint::Unicode(_)){"unicode"}else{"legacy"},cp.value()])).collect::<Vec<_>>(),"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|[v.x,v.y]),"rows":g.bitmap.as_bytes()})).collect::<Vec<_>>()})
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 3 {
        return Err(fontpixel::Error::InvalidInput("usage: w42_timing inspect FORMAT INPUT [PRESET] | convert FROM TO INPUT OUTPUT [PRESET] | time FORMAT INPUT [PRESET]".into()));
    }
    match args[0].as_str() {
        "inspect" => {
            let preset = args.get(3).map(|s| s.parse()).transpose()?;
            let (f, r) = decode(&args[1], &std::fs::read(&args[2])?, preset)?;
            println!("{}", signature(&f));
            eprint!("{r}");
        }
        "convert" => {
            if args.len() < 5 {
                return Err(fontpixel::Error::InvalidInput(
                    "convert requires FROM TO INPUT OUTPUT".into(),
                ));
            }
            let preset = args.get(5).map(|s| s.parse()).transpose()?;
            let (f, mut report) = decode(&args[1], &std::fs::read(&args[3])?, preset)?;
            let (b, r) = encode(&args[2], &f, preset)?;
            report.extend(r);
            std::fs::write(Path::new(&args[4]), b)?;
            eprint!("{report}");
        }
        "time" => {
            let preset = args.get(3).map(|s| s.parse()).transpose()?;
            let bytes = std::fs::read(&args[2])?;
            let (f, _) = decode(&args[1], &bytes, preset)?;
            let iterations = 100;
            let start = std::time::Instant::now();
            for _ in 0..iterations {
                std::hint::black_box(decode(&args[1], std::hint::black_box(&bytes), preset)?);
            }
            let read_us = start.elapsed().as_secs_f64() * 1e6 / f64::from(iterations);
            let start = std::time::Instant::now();
            for _ in 0..iterations {
                std::hint::black_box(encode(&args[1], std::hint::black_box(&f), preset)?);
            }
            println!(
                "{}",
                serde_json::json!({"format":args[1],"input_bytes":bytes.len(),"glyphs":f.len(),"iterations":iterations,"read_us":read_us,"write_us":start.elapsed().as_secs_f64()*1e6/f64::from(iterations)})
            );
        }
        _ => {
            return Err(fontpixel::Error::InvalidInput(
                "unknown bridge operation".into(),
            ));
        }
    }
    Ok(())
}
