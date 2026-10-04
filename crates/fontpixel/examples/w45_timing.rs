//! W4.5 oracle bridge and repeatable bounded codec timing; not a production CLI.
use fontpixel::formats::{gf, pk, plan9};
use fontpixel::model::{Codepoint, Font, FontSet, Glyph, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{Bitmap, ResourceLimits, Result};
use std::time::Instant;
fn load(format: &str, b: &[u8]) -> Result<Font> {
    let l = ResourceLimits::default();
    Ok(match format {
        "pk" => pk::read(b, &l)?.0,
        "gf" => gf::read(b, &l)?.0,
        "plan9" | "plan9c" | "subfont" => {
            let input = Input::new(b);
            let mut c = ReadContext::new(&input, &l)?;
            plan9::read(&mut c)?.0
        }
        _ => return Err(fontpixel::Error::InvalidInput("unknown format".into())),
    })
}
fn encode(format: &str, f: &Font) -> Result<Vec<u8>> {
    Ok(match format {
        "pk" => pk::write(f, &Default::default())?.0,
        "gf" => gf::write(f, &Default::default())?.0,
        "plan9" | "plan9c" | "subfont" => {
            let o = plan9::Plan9WriteOptions {
                container: plan9::Container::Subfont,
                compression: if format == "plan9c" {
                    plan9::Compression::Literal
                } else {
                    plan9::Compression::Raw
                },
                ..Default::default()
            };
            plan9::write(
                &FontSet::from(f.clone()),
                &ResourcePath::new("font.subfont")?,
                &o,
            )?
            .output
            .into_single()?
        }
        _ => return Err(fontpixel::Error::InvalidInput("unknown format".into())),
    })
}
fn dump(f: &Font) {
    print!("{{\"glyphs\":[");
    for (n, (id, g)) in f.glyphs().enumerate() {
        if n > 0 {
            print!(",")
        }
        print!("{{\"slot\":{},\"codes\":[", id.index());
        for (i, c) in g.codepoints.iter().enumerate() {
            if i > 0 {
                print!(",")
            }
            print!("{}", c.value())
        }
        let b = g.metrics.bounds;
        let advance = g.metrics.advance.unwrap_or_default();
        print!(
            "],\"bounds\":[{},{},{},{}],\"advance\":[{},{}],\"pixels\":\"",
            b.width, b.height, b.x, b.y, advance.x, advance.y
        );
        for byte in g.bitmap.as_bytes() {
            print!("{byte:02x}")
        }
        print!("\"}}");
    }
    let point = f.metrics.point_size;
    let dpi = f.metrics.resolution;
    println!(
        "],\"point\":[{},{}],\"dpi\":[{},{}]}}",
        point.map_or(0, |p| p.numerator()),
        point.map_or(1, |p| p.denominator()),
        dpi.map_or(0, |p| p.x),
        dpi.map_or(0, |p| p.y)
    );
}
fn synthetic(n: usize) -> Result<Font> {
    let mut f = Font::default();
    f.metrics.ascent = Some(8);
    f.metrics.descent = Some(0);
    f.metrics.line_height = Some(8);
    for code in 0..n {
        let bitmap: Bitmap =
            "#..##..#\n.##..##.\n#.#..#.#\n..####..\n..####..\n#.#..#.#\n.##..##.\n#..##..#"
                .parse()?;
        let mut g = Glyph::new(bitmap);
        g.codepoints.push(Codepoint::Legacy(code as u32));
        g.metrics.advance = Some(Vector::new(8, 0));
        f.add_glyph(g)?;
    }
    Ok(f)
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str){Some("dump")if args.len()==4=>dump(&load(&args[2],&std::fs::read(&args[3])?)?),Some("convert")if args.len()==6=>{let f=load(&args[2],&std::fs::read(&args[4])?)?;std::fs::write(&args[5],encode(&args[3],&f)?)?},Some("sample")if args.len()==4=>{std::fs::write(&args[3],encode(&args[2],&synthetic(128)?)?)?},Some("bench")=>{let n=args.get(2).and_then(|v|v.parse().ok()).unwrap_or(4096);let f=synthetic(n)?;for format in ["pk","gf","plan9"]{let bytes=encode(format,&f)?;let native=load(format,&bytes)?;let at=Instant::now();let parsed=load(format,std::hint::black_box(&bytes))?;let read=at.elapsed();let at=Instant::now();let output=encode(format,std::hint::black_box(&native))?;let write=at.elapsed();println!("{{\"format\":\"{format}\",\"glyphs\":{},\"bytes\":{},\"read_ns\":{},\"write_ns\":{}}}",parsed.len(),output.len(),read.as_nanos(),write.as_nanos());}},_=>return Err(fontpixel::Error::InvalidInput("usage: w45_timing dump FORMAT INPUT | convert FROM TO INPUT OUTPUT | sample FORMAT OUTPUT | bench [GLYPHS]".into()))}
    Ok(())
}
