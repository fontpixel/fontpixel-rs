//! W4.1 independent-oracle signature bridge and in-memory measurements.
use fontpixel::formats::{bdf, cpi, draw, raw};
use fontpixel::{Error, FontSet, Report, ResourceLimits, Result};
fn invalid(s: &str) -> Error {
    Error::InvalidInput(s.into())
}
fn layout(preset: Option<&str>) -> Result<Option<raw::RawLayout>> {
    preset
        .map(|p| p.parse::<raw::RawPreset>().map(|p| p.layout()))
        .transpose()
}
fn decode(id: &str, b: &[u8], preset: Option<&str>) -> Result<(FontSet, Report)> {
    match id {
        "cpi" | "cp" | "cpx" => cpi::read(b, &Default::default(), &Default::default()),
        "draw" => draw::read(b, &Default::default()),
        "raw" => raw::read(
            b,
            &raw::RawReadOptions {
                layout: layout(preset)?,
            },
            &Default::default(),
        ),
        "bdf" => {
            let (f, r) = bdf::read(b, &ResourceLimits::default())?;
            Ok((f.into(), r))
        }
        _ => Err(invalid("unknown bridge input format")),
    }
}
fn encode(id: &str, f: &FontSet, preset: Option<&str>) -> Result<(Vec<u8>, Report)> {
    match id {
        "cpi" | "fontnt" | "drfont" | "cp" | "cpx" => cpi::write(
            f,
            &cpi::CpiWriteOptions {
                version: match id {
                    "fontnt" => cpi::Version::FontNt,
                    "drfont" => cpi::Version::DrFont,
                    "cp" => cpi::Version::Cp,
                    "cpx" => cpi::Version::Cpx,
                    _ => cpi::Version::Font,
                },
                ..Default::default()
            },
        ),
        "draw" => draw::write(f, &Default::default()),
        "raw" => raw::write(
            f,
            &raw::RawWriteOptions {
                layout: layout(preset)?,
                ..Default::default()
            },
        ),
        "bdf" => bdf::write(f.single()?, &Default::default()),
        _ => Err(invalid("unknown bridge output format")),
    }
}
fn signature(s: &FontSet) -> serde_json::Value {
    serde_json::json!(s.fonts.iter().map(|f|serde_json::json!({"charset":f.charset.canonical_name(),"codepage":f.charset.code_page,"glyphs":f.glyphs().map(|(id,g)|serde_json::json!({"id":id.index(),"codes":g.codepoints.iter().map(|cp|serde_json::json!([if matches!(cp,fontpixel::model::Codepoint::Unicode(_)){"unicode"}else{"legacy"},cp.value()])).collect::<Vec<_>>(),"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|[v.x,v.y]),"rows":g.bitmap.as_bytes()})).collect::<Vec<_>>()})).collect::<Vec<_>>())
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 3 {
        return Err(invalid(
            "usage: w41_timing inspect FORMAT INPUT [PRESET] | convert FROM TO INPUT OUTPUT [PRESET] | time FORMAT INPUT [PRESET]",
        ));
    }
    match args[0].as_str() {
        "inspect" => {
            let (f, r) = decode(
                &args[1],
                &std::fs::read(&args[2])?,
                args.get(3).map(String::as_str),
            )?;
            println!("{}", signature(&f));
            eprint!("{r}");
        }
        "convert" => {
            if args.len() < 5 {
                return Err(invalid("convert requires FROM TO INPUT OUTPUT [PRESET]"));
            }
            let p = args.get(5).map(String::as_str);
            let (f, r) = decode(&args[1], &std::fs::read(&args[3])?, p)?;
            eprint!("{r}");
            let (b, r) = encode(&args[2], &f, p)?;
            eprint!("{r}");
            std::fs::write(&args[4], b)?;
        }
        "time" => {
            let b = std::fs::read(&args[2])?;
            let p = args.get(3).map(String::as_str);
            let (f, _) = decode(&args[1], &b, p)?;
            let mut reads = Vec::new();
            let mut writes = Vec::new();
            for _ in 0..5 {
                let start = std::time::Instant::now();
                std::hint::black_box(decode(&args[1], &b, p)?);
                reads.push(start.elapsed().as_secs_f64() * 1000.0);
                let start = std::time::Instant::now();
                std::hint::black_box(encode(
                    if args[1] == "cpx" { "cpi" } else { &args[1] },
                    &f,
                    p,
                )?);
                writes.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            reads.sort_by(f64::total_cmp);
            writes.sort_by(f64::total_cmp);
            println!(
                "{}",
                serde_json::json!({"format":args[1],"bytes":b.len(),"fonts":f.fonts.len(),"glyphs":f.fonts.iter().map(|f|f.len()).sum::<usize>(),"read_ms":reads,"write_ms":writes,"read_median_ms":reads[2],"write_median_ms":writes[2]})
            );
        }
        _ => return Err(invalid("unknown bridge operation")),
    };
    Ok(())
}
