//! W5.A1 independent-oracle adapter and quiet native timing runner.
use fontpixel::formats::{dosstart, drhalo, edwin, mkwinfont, pcr, plus3dos, psf2txt, writeon};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
fn read(id: &str, b: &[u8]) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let limits = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &limits)?;
    macro_rules! get {
        ($m:ident) => {
            $m::read(&mut c, &Default::default())?.0
        };
    }
    Ok(match id {
        "dosstart" => get!(dosstart),
        "edwin" => get!(edwin),
        "mkwinfont" => get!(mkwinfont),
        "psf2txt" => get!(psf2txt),
        "pcr" => get!(pcr),
        "plus3dos" => get!(plus3dos),
        "drhalo" => get!(drhalo),
        "writeon" => get!(writeon),
        _ => {
            return Err(fontpixel::Error::InvalidInput(
                "unknown W5.A1 format".into(),
            ));
        }
    })
}
fn write(id: &str, s: &FontSet) -> fontpixel::Result<Vec<u8>> {
    let o = WriteOptions::new(ResourcePath::new("font")?);
    macro_rules! put {
        ($m:ident) => {
            $m::write(s, &o, &Default::default())?
        };
    }
    let w = match id {
        "dosstart" => put!(dosstart),
        "edwin" => put!(edwin),
        "mkwinfont" => put!(mkwinfont),
        "psf2txt" => put!(psf2txt),
        "pcr" => put!(pcr),
        "plus3dos" => put!(plus3dos),
        "drhalo" => put!(drhalo),
        "writeon" => put!(writeon),
        _ => {
            return Err(fontpixel::Error::InvalidInput(
                "unknown W5.A1 format".into(),
            ));
        }
    };
    w.output.into_single()
}
fn dump(s: &FontSet) -> serde_json::Value {
    serde_json::Value::Array(s.fonts.iter().map(|f|{let glyphs:Vec<_>=f.glyphs().map(|(_,g)|serde_json::json!({"legacy":g.codepoints.iter().filter_map(|c|if let fontpixel::model::Codepoint::Legacy(v)=c{Some(v)}else{None}).collect::<Vec<_>>(),"unicode":g.codepoints.iter().filter_map(|c|if let fontpixel::model::Codepoint::Unicode(v)=c{Some(v)}else{None}).collect::<Vec<_>>(),"sequences":g.unicode_sequences,"name":g.name,"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|v.x),"bytes":g.bitmap.as_bytes()})).collect();serde_json::json!({"glyphs":glyphs,"ascent":f.metrics.ascent,"descent":f.metrics.descent,"family":f.names.family,"point_size":f.metrics.point_size.map(|v|[v.numerator(),i64::from(v.denominator())])})}).collect())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() < 4 {
        return Err(
            "usage: dump FORMAT FILE | rewrite FORMAT INPUT OUTPUT | bench FORMAT FILE [COUNT]"
                .into(),
        );
    }
    let b = std::fs::read(&a[3])?;
    match a[1].as_str() {
        "dump" => println!("{}", dump(&read(&a[2], &b)?)),
        "rewrite" => std::fs::write(
            a.get(4).ok_or("missing output path")?,
            write(&a[2], &read(&a[2], &b)?)?,
        )?,
        "bench" => {
            let count = a.get(4).map_or(Ok(1000), |s| s.parse::<usize>())?;
            let set = read(&a[2], &b)?;
            let started = std::time::Instant::now();
            for _ in 0..count {
                std::hint::black_box(read(&a[2], &b)?);
            }
            let read_ns = started.elapsed().as_nanos();
            let started = std::time::Instant::now();
            for _ in 0..count {
                std::hint::black_box(write(&a[2], &set)?);
            }
            let write_ns = started.elapsed().as_nanos();
            println!(
                "{}",
                serde_json::json!({"format":a[2],"count":count,"glyphs":set.fonts[0].len(),"input_bytes":b.len(),"read_ns":read_ns,"write_ns":write_ns})
            );
        }
        _ => return Err("unknown command".into()),
    }
    Ok(())
}
