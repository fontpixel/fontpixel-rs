//! Reproducible original fixtures, external-oracle inspection and quiet timing.
use fontpixel::bitmap::{Bitmap, Gray8, Mono};
use fontpixel::formats::{cybiko, hmzk, playdate, rockbox, sbf, u8m};
use fontpixel::model::{Codepoint, Font, Glyph, GlyphBitmap, Property, PropertyValue, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath, ResourceResolver, Written};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Instant;
fn fixture(w: u32, h: u32, n: u32, legacy: bool, gray: bool, wide: bool) -> FontSet {
    let mut f = Font::default();
    f.names.family = Some("Original E1".into());
    f.names.copyright = Some("MIT original test pixels".into());
    f.metrics.ascent = Some(i64::from(h));
    f.metrics.descent = Some(0);
    f.metrics.line_height = Some(i64::from(h));
    f.metrics.pixel_size = Some(h);
    for i in 0..n {
        let mut g = if gray {
            let mut b = Bitmap::<Gray8>::new(w, h).unwrap();
            for y in 0..h {
                for x in 0..w {
                    b.set_pixel(x, y, (((x * 3 + y * 5 + i) % 16) * 17) as u8)
                        .unwrap();
                }
            }
            Glyph::new(b)
        } else {
            let mut b = Bitmap::<Mono>::new(w, h).unwrap();
            for y in 0..h {
                for x in 0..w {
                    if (x * 3 + y * 5 + i) % 7 < 2 {
                        b.set_pixel(x, y, 1).unwrap();
                    }
                }
            }
            Glyph::new(b)
        };
        let cp = if wide && i == 3 {
            0x100
        } else if wide && i == 4 {
            0x20000
        } else {
            65 + i
        };
        g.codepoints.push(if legacy {
            Codepoint::Legacy(cp)
        } else {
            Codepoint::Unicode(cp)
        });
        g.metrics.advance = Some(Vector::new(i64::from(w), 0));
        f.add_glyph(g).unwrap();
    }
    f.into()
}
fn signature(s: &FontSet) -> serde_json::Value {
    let f = s.single().unwrap();
    let mut glyphs = Vec::new();
    let mut unencoded = 0;
    let mut unencoded_glyphs = Vec::new();
    for (id, g) in f.glyphs() {
        let mut pixels = Vec::new();
        for y in 0..g.bitmap.height() {
            for x in 0..g.bitmap.width() {
                pixels.push(match &g.bitmap {
                    GlyphBitmap::Mono(b) => b.get_pixel(x, y).unwrap() * 255,
                    GlyphBitmap::Gray8(b) => b.get_pixel(x, y).unwrap(),
                });
            }
        }
        if g.codepoints.is_empty() {
            unencoded += 1;
            unencoded_glyphs.push(json!({"glyph_id":id.index(),"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|a|a.x),"pixels":pixels}));
        }
        for cp in &g.codepoints {
            let (domain, cp) = match cp {
                Codepoint::Unicode(c) => ("unicode", *c),
                Codepoint::Legacy(c) => ("legacy", *c),
            };
            glyphs.push(json!({"glyph_id":id.index(),"cp":cp,"domain":domain,"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|a|a.x),"pixels":pixels}));
        }
    }
    glyphs.sort_by_key(|g| g["cp"].as_u64().unwrap());
    json!({"ascent":f.metrics.ascent,"descent":f.metrics.descent,"line_height":f.metrics.line_height,"family":f.names.family,"unencoded":unencoded,"unencoded_glyphs":unencoded_glyphs,"glyphs":glyphs})
}
fn write(s: &FontSet, format: &str, name: &str) -> fontpixel::Result<Written> {
    let o = WriteOptions::new(ResourcePath::new(name)?);
    match format {
        "rb11" => rockbox::write(
            s,
            &o,
            &rockbox::RockboxWriteOptions {
                flavor: rockbox::RockboxFlavor::Rb11,
                ..Default::default()
            },
        ),
        "rb12" | "large" => rockbox::write(s, &o, &Default::default()),
        "gray" => rockbox::write(
            s,
            &o,
            &rockbox::RockboxWriteOptions {
                depth: Some(rockbox::RockboxDepth::Gray4),
                ..Default::default()
            },
        ),
        "u8m" => u8m::write(s, &o, &Default::default()),
        "cybiko" => cybiko::write(
            s,
            &o,
            &cybiko::CybikoWriteOptions {
                unicode_as_legacy: true,
                ..Default::default()
            },
        ),
        "sbf" => sbf::write(
            s,
            &o,
            &sbf::SbfWriteOptions {
                unicode_as_legacy: true,
                ..Default::default()
            },
        ),
        "hmzk" => hmzk::write(s, &o, &Default::default()),
        "playdate" | "split" | "pft" => playdate::write(
            s,
            &o,
            &playdate::PlaydateWriteOptions {
                flavor: match format {
                    "split" => playdate::PlaydateFlavor::FntPng,
                    "pft" => playdate::PlaydateFlavor::Pft,
                    _ => playdate::PlaydateFlavor::FntEmbedded,
                },
                ..Default::default()
            },
        ),
        _ => Err(fontpixel::Error::InvalidInput("unknown E1 output".into())),
    }
}
struct Directory(PathBuf);
impl ResourceResolver for Directory {
    fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let path = self.0.join(p.as_str());
        let meta = std::fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            return Err(fontpixel::Error::InvalidInput(
                "oracle resolver refuses symlinks".into(),
            ));
        }
        if meta.len() > max as u64 {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "oracle resource bytes",
                limit: max,
            });
        }
        Ok(std::fs::read(path)?)
    }
    fn list(
        &self,
        d: Option<&ResourcePath>,
        max: usize,
        bytes: usize,
    ) -> fontpixel::Result<Vec<ResourcePath>> {
        let path = d.map_or_else(|| self.0.clone(), |p| self.0.join(p.as_str()));
        let mut out = Vec::new();
        let mut total = 0;
        for entry in std::fs::read_dir(path)? {
            let e = entry?;
            if !e.file_type()?.is_file() {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let name = d.map_or(name.clone(), |p| format!("{p}/{name}"));
            total += name.len() + 128;
            if out.len() >= max || total > bytes {
                return Err(fontpixel::Error::ResourceLimit {
                    resource: "oracle listing bytes",
                    limit: bytes,
                });
            }
            out.push(ResourcePath::new(name)?);
        }
        Ok(out)
    }
}
fn read(format: &str, path: &Path) -> fontpixel::Result<FontSet> {
    let bytes = std::fs::read(path)?;
    let name = path.file_name().unwrap().to_str().unwrap();
    let resources = Directory(path.parent().unwrap().to_path_buf());
    let input = Input::new(&bytes)
        .with_name(name)
        .with_resources(&resources);
    let limits = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &limits)?;
    let (s, _) = match format {
        "rb11" | "rb12" | "gray" | "large" | "rockbox" => {
            rockbox::read(&mut c, &Default::default())
        }
        "u8m" => u8m::read(&mut c, &Default::default()),
        "cybiko" => cybiko::read(&mut c, &Default::default()),
        "sbf" => sbf::read(&mut c, &Default::default()),
        "hmzk" => hmzk::read(&mut c, &Default::default()),
        "playdate" | "split" | "pft" => playdate::read(&mut c, &Default::default()),
        _ => Err(fontpixel::Error::InvalidInput("unknown E1 input".into())),
    }?;
    Ok(s)
}
fn save(s: &FontSet, format: &str, path: &Path) -> fontpixel::Result<()> {
    let w = write(s, format, path.file_name().unwrap().to_str().unwrap())?;
    let root = path.parent().unwrap();
    for asset in w.output.assets() {
        std::fs::write(root.join(asset.path.as_str()), &asset.bytes)?;
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str){
    Some("inspect")=>{let s=read(&args[2],Path::new(&args[3]))?;println!("{}",signature(&s));},
    Some("rewrite")=>{let s=read(&args[2],Path::new(&args[3]))?;save(&s,&args[4],Path::new(&args[5]))?;},
    Some("emit")=>{let dir=Path::new(&args[2]);std::fs::create_dir_all(dir)?;for(format,name)in[("rb11","original.rb11"),("rb12","original.rb12"),("u8m","original.u8m"),("cybiko","original.cyf"),("sbf","original.sbf"),("hmzk","original.hmzk"),("playdate","original-playdate.fnt"),("split","original-split.fnt"),("pft","original.pft"),("gray","original-gray.rb12"),("large","original-large.rb12")]{
        let(w,h,n)=if format=="hmzk"{(16,16,3)}else if format=="large"{(255,64,33)}else if ["playdate","split","pft"].contains(&format){(9,7,5)}else{(9,7,3)};
        let mut s=fixture(w,h,n,["cybiko","sbf"].contains(&format),format=="gray",["playdate","split","pft"].contains(&format));
        if format=="u8m"{let id=s.fonts[0].glyph_id(Codepoint::Unicode(65)).unwrap();s.fonts[0].edit_glyph(id,|g|{g.codepoints.extend([Codepoint::Unicode(0x20001),Codepoint::Legacy(65)]);Ok(())})?;}
        if ["playdate","split","pft"].contains(&format){s.fonts[0].properties.push(Property::new("playdate.tracking",2i64));s.fonts[0].properties.push(Property::new("playdate.kerning",PropertyValue::List([(65,66,-2),(65,256,-3),(0x20000,65,1)].into_iter().map(|(a,b,k)|PropertyValue::List(vec![PropertyValue::Integer(a),PropertyValue::Integer(b),PropertyValue::Integer(k)])).collect())));}
        save(&s,format,&dir.join(name))?;std::fs::write(dir.join(format!("{name}.expected.json")),signature(&s).to_string())?;
    }},
    Some("bench")=>{let n=args.get(2).and_then(|s|s.parse::<u32>().ok()).unwrap_or(512);let rounds=args.get(3).and_then(|s|s.parse::<u32>().ok()).unwrap_or(20);let s=fixture(9,7,n,false,false,false);for format in["rb12","u8m","hmzk","cybiko","sbf","playdate","pft"]{let count=if ["cybiko","sbf"].contains(&format){n.min(190)}else{n};let small;let source=if count!=n{small=fixture(9,7,count,false,false,false);&small}else{&s};let start=Instant::now();let mut bytes=0;for _ in 0..rounds{let out=write(std::hint::black_box(source),format,"bench.fnt")?;bytes+=out.output.assets().iter().map(|a|a.bytes.len()).sum::<usize>();std::hint::black_box(out);}println!("{format}\t{count}\t{rounds}\t{}\t{bytes}",start.elapsed().as_nanos());}},
    _=>return Err("usage: w5e1_timing emit DIR | inspect FORMAT FILE | rewrite FROM FILE TO FILE | bench [GLYPHS] [ROUNDS]".into())
}
    Ok(())
}
