//! W5.5 original fixtures, native signatures, conversion and quiet in-memory timing.
#[cfg(all(feature = "c", feature = "drcs"))]
mod app {
    use fontpixel::bitmap::{Bitmap, BitmapLimits, Mono};
    use fontpixel::formats::{FormatReader, FormatWriter, ReadOptions, WriteOptions, c::*, drcs};
    use fontpixel::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap, Vector};
    use fontpixel::resources::ReadContext;
    use fontpixel::{Input, Report, ResourceLimits, ResourcePath, Result};
    fn options(kind: &str) -> CWriteOptions {
        let family = match kind {
            v if v.starts_with("u8glib") => Family::U8glib,
            v if v.starts_with("u8x8") => Family::U8x8,
            "u8g2" => Family::U8g2,
            "netbsd" => Family::Netbsd,
            _ => Family::Lvgl,
        };
        CWriteOptions {
            family,
            container: if kind.contains("binary") {
                CContainer::LvglBinary
            } else {
                CContainer::Source
            },
            u8x8_legacy: kind.contains("legacy"),
            u8glib_format: if kind.contains("compact") {
                U8glibFormat::Compact
            } else if kind.contains("gray") {
                U8glibFormat::Gray2
            } else {
                U8glibFormat::Full
            },
            lvgl_bpp: if kind.starts_with("lvgl2") {
                LvglBpp::Two
            } else if kind.starts_with("lvgl4") {
                LvglBpp::Four
            } else if kind.starts_with("lvgl8") {
                LvglBpp::Eight
            } else {
                LvglBpp::One
            },
            lvgl_compression: if kind.contains("xor") {
                LvglCompression::RleXor
            } else if kind.contains("rle") {
                LvglCompression::Rle
            } else {
                LvglCompression::Plain
            },
            gray_threshold: Some(128),
            ..Default::default()
        }
    }
    fn read(kind: &str, bytes: &[u8]) -> Result<(FontSet, Report)> {
        if kind == "drcs" {
            return drcs::read(bytes, &Default::default(), &Default::default());
        }
        let opts = options(kind);
        let codec = CCodec {
            read_options: CReadOptions {
                family: if kind == "auto" {
                    Family::Auto
                } else {
                    opts.family
                },
                u8x8_legacy: opts.u8x8_legacy,
                ..Default::default()
            },
            ..Default::default()
        };
        let input = Input::new(bytes);
        let ro = ReadOptions::default();
        codec.read(&mut ReadContext::new(&input, &ro.limits)?, &ro)
    }
    fn write(kind: &str, set: &FontSet) -> Result<(Vec<u8>, Report)> {
        if kind == "drcs" {
            return drcs::write(set, &Default::default());
        }
        let w = CCodec {
            write_options: options(kind),
            ..Default::default()
        }
        .write(set, &WriteOptions::new(ResourcePath::new("font.h")?))?;
        Ok((w.output.into_single()?, w.report))
    }
    fn fixture(kind: &str) -> Result<FontSet> {
        let mut f = Font::default();
        for i in 0..if kind == "u8g2" { 4u32 } else { 3u32 } {
            let mut bytes = vec![0u8; 8];
            for y in 0..8 {
                for x in 0..8 {
                    if (x + y + i) % 3 == 0 || x == i {
                        bytes[y as usize] |= 128 >> x;
                    }
                }
            }
            let mut g = Glyph::new(Bitmap::<Mono>::from_raw(
                8,
                8,
                &bytes,
                BitmapLimits::default(),
            )?);
            if kind == "u8glib-gray"
                || kind.starts_with("lvgl2")
                || kind.starts_with("lvgl4")
                || kind.starts_with("lvgl8")
            {
                let pixels = (0..64u32)
                    .map(|p| ((p % 8 * 17 + p / 8 * 31 + i * 43) % 256) as u8)
                    .collect::<Vec<_>>();
                g = Glyph::new(Bitmap::<fontpixel::bitmap::Gray8>::from_raw(
                    8,
                    8,
                    &pixels,
                    BitmapLimits::default(),
                )?);
            }
            g.codepoints.push(if kind == "drcs" {
                Codepoint::Legacy(65 + i)
            } else {
                Codepoint::Unicode(if kind == "u8g2" && i == 3 {
                    0x4e00
                } else {
                    65 + i
                })
            });
            g.metrics.advance = Some(Vector::new(8, 0));
            f.add_glyph(g)?;
        }
        f.update_bounds()?;
        f.metrics.ascent = Some(8);
        f.metrics.descent = Some(0);
        f.metrics.line_height = Some(8);
        f.charset.name = Some(
            if kind == "drcs" {
                "dec-drcs"
            } else {
                "unicode"
            }
            .into(),
        );
        Ok(f.into())
    }
    fn signature(set: &FontSet) -> serde_json::Value {
        serde_json::json!({"glyphs":set.fonts[0].glyphs().map(|(_,g)|{let mut pixels=Vec::new();for y in 0..g.bitmap.height(){for x in 0..g.bitmap.width(){pixels.push(match &g.bitmap{GlyphBitmap::Mono(v)=>if v.get_pixel(x,y).unwrap_or(0)!=0{255}else{0},GlyphBitmap::Gray8(v)=>v.get_pixel(x,y).unwrap_or(0)});}}serde_json::json!({"codes":g.codepoints.iter().map(|v|v.value()).collect::<Vec<_>>(),"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|v.x),"pixels":pixels})}).collect::<Vec<_>>()})
    }
    pub fn main() -> Result<()> {
        let a: Vec<_> = std::env::args().skip(1).collect();
        if a.len() < 3 {
            return Err(fontpixel::Error::InvalidInput("usage: fixture KIND OUT | inspect KIND IN | convert FROM TO IN OUT | time KIND IN [ITERATIONS]".into()));
        }
        match a[0].as_str() {
            "fixture" => {
                let (bytes, _) = write(&a[1], &fixture(&a[1])?)?;
                std::fs::write(&a[2], bytes)?;
            }
            "inspect" => {
                let (s, r) = read(&a[1], &std::fs::read(&a[2])?)?;
                println!("{}", signature(&s));
                eprint!("{r}");
            }
            "convert" => {
                if a.len() != 5 {
                    return Err(fontpixel::Error::InvalidInput(
                        "convert requires FROM TO IN OUT".into(),
                    ));
                }
                let (s, _) = read(&a[1], &std::fs::read(&a[3])?)?;
                let (bytes, r) = write(&a[2], &s)?;
                std::fs::write(&a[4], bytes)?;
                eprint!("{r}");
            }
            "time" => {
                let bytes = std::fs::read(&a[2])?;
                let (s, _) = read(&a[1], &bytes)?;
                let n = a
                    .get(3)
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(1000)
                    .max(1);
                let start = std::time::Instant::now();
                for _ in 0..n {
                    std::hint::black_box(read(&a[1], std::hint::black_box(&bytes))?);
                }
                let read_ns = start.elapsed().as_nanos() / n as u128;
                let start = std::time::Instant::now();
                for _ in 0..n {
                    std::hint::black_box(write(&a[1], std::hint::black_box(&s))?);
                }
                println!(
                    "{}",
                    serde_json::json!({"kind":a[1],"iterations":n,"bytes":bytes.len(),"glyphs":s.fonts[0].len(),"read_ns":read_ns,"write_ns":start.elapsed().as_nanos()/n as u128,"max_total_bytes":ResourceLimits::default().max_total_bytes})
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
}
#[cfg(all(feature = "c", feature = "drcs"))]
fn main() -> fontpixel::Result<()> {
    app::main()
}
#[cfg(not(all(feature = "c", feature = "drcs")))]
fn main() {}
