//! Quiet native adapters for independent SSFN/PFR consumers and later formal timing.
#[cfg(all(feature = "ssfn", feature = "pfr"))]
mod enabled {
    use fontpixel::formats::{pfr, ssfn};
    use fontpixel::resources::{Input, ReadContext, ResourcePath};
    use fontpixel::{FontSet, ResourceLimits, WriteOptions};
    fn read(id: &str, b: &[u8]) -> fontpixel::Result<(FontSet, fontpixel::Report)> {
        let input = Input::new(b);
        let l = ResourceLimits::default();
        let mut c = ReadContext::new(&input, &l)?;
        match id {
            "pfr" => pfr::read(&mut c, &Default::default()),
            "ssfn" => ssfn::read(&mut c, &Default::default()),
            _ => Err(fontpixel::Error::InvalidInput("unknown format".into())),
        }
    }
    fn write(
        id: &str,
        s: &FontSet,
        variant: &str,
    ) -> fontpixel::Result<fontpixel::resources::Written> {
        let o = WriteOptions::new(ResourcePath::new("font")?);
        match id {
            "pfr" => {
                let p = pfr::PfrWriteOptions {
                    dialect: if variant.starts_with("ft") {
                        Some(pfr::PfrDialect::FreeType)
                    } else {
                        None
                    },
                    compression: match variant {
                        "rle4" | "ftrle4" => pfr::PfrCompression::Rle4,
                        "rle8" | "ftrle8" => pfr::PfrCompression::Rle8,
                        _ => pfr::PfrCompression::Raw,
                    },
                    ..Default::default()
                };
                pfr::write(s, &o, &p)
            }
            "ssfn" => {
                let p = ssfn::SsfnWriteOptions {
                    encoding: if variant == "ascii" {
                        ssfn::SsfnEncoding::Ascii
                    } else {
                        ssfn::SsfnEncoding::Binary
                    },
                    ..Default::default()
                };
                ssfn::write(s, &o, &p)
            }
            _ => Err(fontpixel::Error::InvalidInput("unknown format".into())),
        }
    }
    fn dump(s: &FontSet) -> serde_json::Value {
        serde_json::Value::Array(s.fonts.iter().map(|f|serde_json::json!({"family":f.names.family,"ppem":f.metrics.pixel_size,"ascent":f.metrics.ascent,"descent":f.metrics.descent,"glyphs":f.glyphs().map(|(_,g)|{let mut pixels=Vec::new();for y in 0..g.bitmap.height(){for x in 0..g.bitmap.width(){pixels.push(match &g.bitmap{fontpixel::model::GlyphBitmap::Mono(b)=>if b.get_pixel(x,y).unwrap()==0{0}else{255},fontpixel::model::GlyphBitmap::Gray8(b)=>b.get_pixel(x,y).unwrap()});}}serde_json::json!({"codes":g.codepoints.iter().map(|c|c.value()).collect::<Vec<_>>(),"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|[v.x,v.y]),"sequences":g.unicode_sequences,"pixels":pixels})}).collect::<Vec<_>>()})).collect())
    }
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let a: Vec<_> = std::env::args().collect();
        if a.len() < 4 {
            return Err(
                "dump ID INPUT | rewrite ID INPUT OUTPUT [VARIANT] | bench ID INPUT [COUNT]".into(),
            );
        }
        let b = std::fs::read(&a[3])?;
        let (s, r) = read(&a[2], &b)?;
        match a[1].as_str() {
            "dump" => println!(
                "{}",
                serde_json::json!({"fonts":dump(&s),"read_loss":r.has_loss()})
            ),
            "rewrite" => {
                let w = write(&a[2], &s, a.get(5).map_or("", String::as_str))?;
                println!(
                    "{}",
                    serde_json::json!({"read_loss":r.has_loss(),"write_loss":w.report.has_loss(),"report":w.report.to_string()})
                );
                std::fs::write(a.get(4).ok_or("missing output")?, w.output.into_single()?)?;
            }
            "bench" => {
                let n: usize = a.get(4).map_or(Ok(1000), |s| s.parse())?;
                let start = std::time::Instant::now();
                for _ in 0..n {
                    std::hint::black_box(read(&a[2], &b)?);
                }
                let rn = start.elapsed().as_nanos();
                let start = std::time::Instant::now();
                for _ in 0..n {
                    std::hint::black_box(write(&a[2], &s, "")?);
                }
                println!(
                    "{}",
                    serde_json::json!({"count":n,"read_ns":rn,"write_ns":start.elapsed().as_nanos(),"input_bytes":b.len()})
                );
            }
            _ => return Err("unknown command".into()),
        }
        Ok(())
    }
}
#[cfg(all(feature = "ssfn", feature = "pfr"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    enabled::run()
}
#[cfg(not(all(feature = "ssfn", feature = "pfr")))]
fn main() {}
