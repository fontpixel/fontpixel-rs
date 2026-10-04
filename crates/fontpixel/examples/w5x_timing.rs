//! W5.X independent-oracle adapter; timing loops perform no filesystem I/O.
#[cfg(not(all(feature = "bmfont", feature = "raw")))]
fn main() {}
#[cfg(all(feature = "bmfont", feature = "raw"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    adapter::main()
}
#[cfg(all(feature = "bmfont", feature = "raw"))]
mod adapter {
    use fontpixel::bitmap::{BitOrder, RowOrder};
    use fontpixel::formats::{
        bmfont::{self, BmfontEncoding, BmfontWriteOptions},
        raw::{self, RawByteOrder, RawLayout, RawPacking, RawReadOptions, RawWriteOptions},
    };
    use fontpixel::resources::{Input, OutputBundle, ReadContext, ResourcePath, ResourceResolver};
    use fontpixel::{FontSet, ResourceLimits, Result};
    use std::{
        hint::black_box,
        path::{Path, PathBuf},
        time::Instant,
    };
    struct Pages(std::collections::BTreeMap<String, Vec<u8>>);
    impl ResourceResolver for Pages {
        fn resolve(&self, p: &ResourcePath, n: usize) -> Result<Vec<u8>> {
            let b = self
                .0
                .get(p.as_str())
                .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?;
            if b.len() > n {
                return Err(fontpixel::Error::ResourceLimit {
                    resource: "adapter page",
                    limit: n,
                });
            }
            Ok(b.clone())
        }
    }
    fn layout(v: &serde_json::Value) -> RawLayout {
        let mut l = RawLayout::new(
            v["width"].as_u64().unwrap_or(8) as u32,
            v["height"].as_u64().unwrap_or(8) as u32,
        );
        l.count = v["count"].as_u64().map(|n| n as usize);
        l.first_codepoint = v["first"].as_u64().unwrap_or(0) as u32;
        l.strike_count = v["strike_count"].as_u64().unwrap_or(1) as usize;
        l.offset = v["offset"].as_u64().unwrap_or(0) as usize;
        l.row_padding = v["padding"].as_u64().unwrap_or(0) as usize;
        l.packing = if v["packing"] == "bit" {
            RawPacking::BitAligned
        } else {
            RawPacking::ByteAlignedRows
        };
        l.byte_order = if v["order"] == "column" {
            RawByteOrder::ColumnMajor
        } else {
            RawByteOrder::RowMajor
        };
        l.bit_order = if v["lsb"] == true {
            BitOrder::LsbFirst
        } else {
            BitOrder::MsbFirst
        };
        l.row_order = if v["bottom"] == true {
            RowOrder::BottomUp
        } else {
            RowOrder::TopDown
        };
        l
    }
    fn encoding(v: &serde_json::Value) -> BmfontEncoding {
        match v["encoding"].as_str().unwrap_or("json") {
            "text" => BmfontEncoding::Text,
            "xml" => BmfontEncoding::Xml,
            "binary" => BmfontEncoding::Binary,
            _ => BmfontEncoding::Json,
        }
    }
    fn read(id: &str, b: &[u8], v: &serde_json::Value, p: &Pages) -> Result<FontSet> {
        let limits = ResourceLimits::default();
        if id == "raw" {
            return Ok(raw::read(
                b,
                &RawReadOptions {
                    layout: Some(layout(v)),
                },
                &limits,
            )?
            .0);
        }
        let input = Input::new(b).with_resources(p);
        Ok(
            bmfont::read(&mut ReadContext::new(&input, &limits)?, &Default::default())?
                .0
                .into(),
        )
    }
    fn write(id: &str, s: &FontSet, v: &serde_json::Value) -> Result<OutputBundle> {
        if id == "raw" {
            let b = raw::write(
                s,
                &RawWriteOptions {
                    layout: Some(layout(v)),
                    ..Default::default()
                },
            )?
            .0;
            return Ok(OutputBundle::single(ResourcePath::new("font.raw")?, b));
        }
        Ok(bmfont::write(
            s.single()?,
            &ResourcePath::new("font.fnt")?,
            &BmfontWriteOptions {
                encoding: encoding(v),
                page_width: v["page_width"].as_u64().unwrap_or(64) as u32,
                page_height: v["page_height"].as_u64().unwrap_or(64) as u32,
                ..Default::default()
            },
        )?
        .output)
    }
    fn dump(s: &FontSet) -> serde_json::Value {
        let f = &s.fonts[0];
        let glyphs:Vec<_>=f.glyphs().map(|(_,g)|{let (w,h)=(g.bitmap.width(),g.bitmap.height());let pixels:Vec<_>=(0..h).flat_map(|y|(0..w).map(move|x|match &g.bitmap{fontpixel::model::GlyphBitmap::Mono(b)=>b.get_pixel(x,y).unwrap()*255,fontpixel::model::GlyphBitmap::Gray8(b)=>b.get_pixel(x,y).unwrap()})).collect();serde_json::json!({"codes":g.codepoints.iter().map(|c|match c{fontpixel::model::Codepoint::Legacy(v)|fontpixel::model::Codepoint::Unicode(v)=>v}).collect::<Vec<_>>(),"width":w,"height":h,"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|v.x),"pixels":pixels})}).collect();
        serde_json::json!({"glyphs":glyphs,"ascent":f.metrics.ascent,"line_height":f.metrics.line_height})
    }
    pub fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let a: Vec<_> = std::env::args().collect();
        if a.len() < 5 {
            return Err(
                "usage: dump|rewrite|bench raw|bmfont INPUT CONFIG_JSON [OUTPUT_DIR|LOOPS]".into(),
            );
        }
        let id = &a[2];
        if id != "raw" && id != "bmfont" {
            return Err("unknown format".into());
        }
        let path = Path::new(&a[3]);
        let b = std::fs::read(path)?;
        let v: serde_json::Value = serde_json::from_str(&a[4])?;
        let mut pages = Pages(Default::default());
        if id == "bmfont" {
            let dir = path.parent().unwrap_or(Path::new("."));
            fn collect(root: &Path, directory: &Path, pages: &mut Pages) -> std::io::Result<()> {
                for entry in std::fs::read_dir(directory)? {
                    let p = entry?.path();
                    if p.is_dir() {
                        collect(root, &p, pages)?;
                    } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png")) {
                        pages.0.insert(
                            p.strip_prefix(root).unwrap().to_string_lossy().into_owned(),
                            std::fs::read(p)?,
                        );
                    }
                }
                Ok(())
            }
            collect(dir, dir, &mut pages)?;
        }
        let s = read(id, &b, &v, &pages)?;
        match a[1].as_str() {
            "dump" => println!("{}", dump(&s)),
            "rewrite" => {
                let dir = PathBuf::from(a.get(5).ok_or("output directory required")?);
                std::fs::create_dir_all(&dir)?;
                for asset in write(id, &s, &v)?.assets() {
                    std::fs::write(dir.join(asset.path.as_str()), &asset.bytes)?;
                }
            }
            "bench" => {
                let loops = a.get(5).map_or(Ok(1000usize), |x| x.parse())?;
                if loops == 0 {
                    return Err("loops must be positive".into());
                }
                let start = Instant::now();
                for _ in 0..loops {
                    black_box(read(id, &b, &v, &pages)?);
                }
                let read_ns = start.elapsed().as_nanos();
                let start = Instant::now();
                for _ in 0..loops {
                    black_box(write(id, &s, &v)?);
                }
                let write_ns = start.elapsed().as_nanos();
                println!(
                    "{}",
                    serde_json::json!({"format":id,"loops":loops,"glyphs":s.fonts[0].len(),"input_bytes":b.len(),"config":v,"read_ns":read_ns,"write_ns":write_ns})
                );
            }
            _ => return Err("unknown command".into()),
        };
        Ok(())
    }
}
