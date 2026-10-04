//! W5.4 independent-oracle adapter and packed bitmap timing probe.
use fontpixel::formats::{gdr, palm};
use fontpixel::model::{Codepoint, Glyph, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use fontpixel::{Bitmap, Font, FontSet, ResourceLimits, WriteOptions};
use std::path::Path;
fn read(format: &str, b: &[u8]) -> fontpixel::Result<FontSet> {
    let input = Input::new(b);
    let limits = ResourceLimits::default();
    let mut context = ReadContext::new(&input, &limits)?;
    Ok(if format == "gdr" {
        gdr::read(&mut context, &Default::default())?.0
    } else {
        palm::read(&mut context, &Default::default())?.0
    })
}
fn dump(s: &FontSet) -> serde_json::Value {
    serde_json::Value::Array(s.fonts.iter().map(|f|{let glyphs:Vec<_>=f.glyphs().map(|(_,g)|serde_json::json!({"codes":g.codepoints.iter().map(|c|c.value()).collect::<Vec<_>>(),"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|v.x),"gray":matches!(g.bitmap,fontpixel::model::GlyphBitmap::Gray8(_)),"bytes":g.bitmap.as_bytes()})).collect();serde_json::json!({"glyphs":glyphs,"ascent":f.metrics.ascent,"descent":f.metrics.descent,"family":f.names.family,"size":f.metrics.pixel_size,"dpi":f.metrics.resolution.map(|v|v.y)})}).collect())
}
fn sample(unicode: bool, scale: u32) -> fontpixel::Result<FontSet> {
    let mut f = Font::default();
    f.names.family = Some("Original".into());
    f.metrics.ascent = Some(i64::from(4 * scale));
    f.metrics.descent = Some(i64::from(2 * scale));
    f.metrics.pixel_size = Some(6 * scale);
    for (cp, text) in [
        (65, "##\n.#\n##\n..\n#.\n.."),
        (67, ".#\n##\n#.\n#.\n##\n.."),
    ] {
        let src = text.parse::<Bitmap>()?;
        let mut bm = Bitmap::<fontpixel::bitmap::Mono>::new(2 * scale, 6 * scale)?;
        for y in 0..6 * scale {
            for x in 0..2 * scale {
                bm.set_pixel(x, y, src.get_pixel(x / scale, y / scale)?)?;
            }
        }
        let mut g = Glyph::new(bm);
        g.codepoints.push(if unicode {
            Codepoint::Unicode(cp)
        } else {
            Codepoint::Legacy(cp)
        });
        g.metrics.bounds.y = -i64::from(2 * scale);
        g.metrics.advance = Some(Vector::new(i64::from(2 * scale), 0));
        f.add_glyph(g)?;
    }
    Ok(f.into())
}
fn large(unicode: bool) -> fontpixel::Result<FontSet> {
    let mut f = Font::default();
    f.metrics.ascent = Some(12);
    f.metrics.descent = Some(4);
    f.metrics.pixel_size = Some(16);
    for cp in 0..256u32 {
        let mut bm = Bitmap::<fontpixel::bitmap::Mono>::new(16, 16)?;
        for y in 0..16 {
            for x in 0..16 {
                bm.set_pixel(x, y, (((cp * 37 + y * 7) >> (x % 8)) & 1) as u8)?;
            }
        }
        let mut g = Glyph::new(bm);
        g.codepoints.push(if unicode {
            Codepoint::Unicode(cp)
        } else {
            Codepoint::Legacy(cp)
        });
        g.metrics.bounds.y = -4;
        g.metrics.advance = Some(Vector::new(16, 0));
        f.add_glyph(g)?;
    }
    Ok(f.into())
}
fn write(s: &FontSet, profile: &str) -> fontpixel::Result<Vec<u8>> {
    let w = WriteOptions::new(ResourcePath::new("font")?);
    let out = if profile.starts_with("gdr") {
        gdr::write(
            s,
            &w,
            &gdr::GdrWriteOptions {
                version: if profile == "gdr42" {
                    gdr::GdrVersion::V42
                } else if profile == "gdr40" {
                    gdr::GdrVersion::V40
                } else if profile == "gdr39" {
                    gdr::GdrVersion::V39
                } else {
                    gdr::GdrVersion::V41
                },
                ..Default::default()
            },
        )?
    } else {
        let version = match profile {
            "v2" | "double" => palm::PalmVersion::V2,
            "afnx" => palm::PalmVersion::Afnx,
            "fontsubst" => palm::PalmVersion::FontSubst,
            "gray1" => palm::PalmVersion::Gray1,
            "gray2" => palm::PalmVersion::Gray2,
            _ => palm::PalmVersion::V1,
        };
        let container = match profile {
            "pdb" => palm::PalmContainer::Pdb,
            "prc" | "gray1" | "gray2" => palm::PalmContainer::Prc,
            _ => palm::PalmContainer::Bare,
        };
        palm::write(
            s,
            &w,
            &palm::PalmWriteOptions {
                version,
                container,
                densities: if profile == "double" {
                    vec![72, 144]
                } else {
                    vec![]
                },
                ..Default::default()
            },
        )?
    };
    Ok(out.output.assets()[0].bytes.clone())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() < 3 {
        return Err("usage: emit DIR | dump FORMAT FILE | rewrite FORMAT INPUT PROFILE OUTPUT | bench FORMAT FILE".into());
    }
    match a[1].as_str() {
        "emit" => {
            let dir = Path::new(&a[2]);
            std::fs::create_dir_all(dir)?;
            for profile in [
                "v1",
                "v2",
                "afnx",
                "fontsubst",
                "pdb",
                "prc",
                "gray1",
                "gray2",
                "gdr39",
                "gdr40",
                "gdr41",
                "gdr42",
                "double",
                "large-palm",
                "gdr-large",
                "gdr-unicode",
            ] {
                let mut s = if profile.contains("large") {
                    large(profile.starts_with("gdr"))?
                } else {
                    sample(profile.starts_with("gdr"), 1)?
                };
                if profile == "gdr-unicode" {
                    s.fonts[0].names.family = Some("Å漢😀\u{f001}".into());
                }
                if profile == "double" {
                    s.fonts.extend(sample(false, 2)?.fonts);
                }
                if profile.starts_with("gray") {
                    for id in s.fonts[0].glyphs().map(|(id, _)| id).collect::<Vec<_>>() {
                        s.fonts[0].edit_glyph(id, |g| {
                            if let fontpixel::model::GlyphBitmap::Mono(b) = &g.bitmap {
                                let mut b = b.to_gray8()?;
                                b.set_pixel(0, 0, 85)?;
                                g.bitmap = b.into();
                            }
                            Ok(())
                        })?;
                    }
                }
                std::fs::write(dir.join(format!("{profile}.bin")), write(&s, profile)?)?;
                std::fs::write(
                    dir.join(format!("{profile}.source.json")),
                    serde_json::to_vec(&dump(&s))?,
                )?;
            }
        }
        "dump" => {
            let b = std::fs::read(&a[3])?;
            println!("{}", dump(&read(&a[2], &b)?));
        }
        "rewrite" => {
            let s = read(&a[2], &std::fs::read(&a[3])?)?;
            std::fs::write(&a[5], write(&s, &a[4])?)?;
        }
        "bench" => {
            let b = std::fs::read(&a[3])?;
            let set = read(&a[2], &b)?;
            let profile = if a[2] == "gdr" { "gdr41" } else { "prc" };
            for _ in 0..10 {
                std::hint::black_box(read(&a[2], &b)?);
                std::hint::black_box(write(&set, profile)?);
            }
            let start = std::time::Instant::now();
            for _ in 0..400 {
                std::hint::black_box(read(&a[2], &b)?);
            }
            let read_us = start.elapsed().as_secs_f64() * 1_000_000. / 400.;
            let start = std::time::Instant::now();
            let mut output = 0;
            for _ in 0..400 {
                output = std::hint::black_box(write(&set, profile)?).len();
            }
            let write_us = start.elapsed().as_secs_f64() * 1_000_000. / 400.;
            println!(
                "{}",
                serde_json::json!({"iterations":400,"input_bytes":b.len(),"output_bytes":output,"read_us":read_us,"write_us":write_us})
            );
        }
        _ => return Err("unknown operation".into()),
    }
    Ok(())
}
