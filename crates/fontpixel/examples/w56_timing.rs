//! Reproducible color writer fixtures, oracle bridge, and quiet release timings.
use fontpixel::{
    Bitmap, Font, FontSet, Glyph,
    bitmap::{BitmapLimits, Gray8, Mono},
    formats::embedded::{self, ColorFlavor, ColorWriteOptions},
    model::{Codepoint, GlyphBitmap, Vector},
};
use serde_json::{Value, json};
use std::{env, fs, path::Path, time::Instant};
fn coverage(g: &Glyph) -> Vec<u8> {
    match &g.bitmap {
        GlyphBitmap::Gray8(b) => b.as_bytes().to_vec(),
        GlyphBitmap::Mono(b) => (0..b.height())
            .flat_map(|y| {
                (0..b.width()).map(move |x| {
                    if b.get_pixel(x, y).unwrap() != 0 {
                        255
                    } else {
                        0
                    }
                })
            })
            .collect(),
    }
}
fn model() -> FontSet {
    let mut set = FontSet::default();
    for k in [2u32, 1, 3] {
        let mut f = Font::default();
        f.names.family = Some("Fontpixel color oracle".into());
        f.metrics.pixel_size = Some(12 * k);
        f.metrics.ascent = Some(9 * i64::from(k));
        f.metrics.descent = Some(3 * i64::from(k));
        f.metrics.resolution = Some(Vector::new(96, 96));
        for (name, codes, mono) in [
            ("A", vec![65, 0x10000], false),
            ("B", vec![66], true),
            ("unencoded", vec![], false),
            ("transparent", vec![67], false),
            ("empty", vec![32], false),
            ("sparse", vec![68], false),
        ] {
            if name == "sparse" && k != 1 {
                continue;
            }
            let (w, h) = if name == "empty" {
                (0, 0)
            } else {
                (5 * k, 3 * k)
            };
            let pixels: Vec<u8> = (0..w * h)
                .map(|i| {
                    if name == "transparent" {
                        0
                    } else {
                        [0, 1, 64, 128, 254, 255][i as usize % 6]
                    }
                })
                .collect();
            let gray = Bitmap::<Gray8>::from_raw(w, h, &pixels, BitmapLimits::default()).unwrap();
            let bitmap = if mono {
                GlyphBitmap::Mono(gray.to_mono(128).unwrap())
            } else {
                GlyphBitmap::Gray8(gray)
            };
            let mut g = Glyph::new(bitmap);
            g.codepoints = codes.into_iter().map(Codepoint::Unicode).collect();
            g.name = Some(name.into());
            g.metrics.advance = Some(Vector::new(7 * i64::from(k), 0));
            g.metrics.bounds.x = -i64::from(k);
            g.metrics.bounds.y = -i64::from(k);
            f.add_glyph(g).unwrap();
        }
        set.fonts.push(f);
    }
    set
}
fn describe(set: &FontSet) -> Value {
    json!(set.fonts.iter().map(|f| json!({
        "ppem": f.metrics.pixel_size,
        "ascent": f.metrics.ascent, "descent": f.metrics.descent,
        "glyphs": f.glyphs().map(|(_, g)| json!({
            "name": g.name, "codes": g.codepoints.iter().map(|c| c.value()).collect::<Vec<_>>(),
            "width": g.bitmap.width(), "height": g.bitmap.height(),
            "x": g.metrics.bounds.x, "y": g.metrics.bounds.y,
            "advance": g.metrics.advance.map(|v| v.x), "pixels": coverage(g)
        })).collect::<Vec<_>>()
    })).collect::<Vec<_>>())
}
fn save(
    set: &FontSet,
    flavor: ColorFlavor,
    name: &str,
    dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let options = ColorWriteOptions {
        flavor,
        ..Default::default()
    };
    let (bytes, report) = embedded::write_color(set, &options)?;
    assert_eq!(bytes, embedded::write_color(set, &options)?.0);
    fs::write(dir.join(format!("{name}.ttf")), &bytes)?;
    let (read, _) = embedded::read(
        &bytes,
        fontpixel::sfnt::bitmap::Kind::Color,
        &Default::default(),
    )?;
    fs::write(
        dir.join(format!("{name}-expected.json")),
        serde_json::to_vec_pretty(&json!({
            "source": describe(set), "read": describe(&read),
            "losses": report.entries.iter().map(|e| &e.code).collect::<Vec<_>>()
        }))?,
    )?;
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    match args.get(1).map(String::as_str).unwrap_or("timing") {
        "fixtures" => {
            let dir = Path::new(&args[2]);
            fs::create_dir_all(dir)?;
            let set = model();
            save(&set, ColorFlavor::Cbdt, "cbdt", dir)?;
            save(&set, ColorFlavor::Sbix, "sbix", dir)?;
            let mut vertical = set.clone();
            for f in &mut vertical.fonts {
                let ids: Vec<_> = f.glyphs().map(|(id, _)| id).collect();
                for id in ids {
                    f.edit_glyph(id, |g| {
                        g.metrics.vertical_advance = Some(Vector::new(0, -20));
                        g.metrics.vertical_origin = Some(Vector::new(1, 3));
                        Ok(())
                    })?;
                }
            }
            save(&vertical, ColorFlavor::Cbdt, "cbdt-vertical", dir)?;
            let mut f = Font::default();
            f.metrics.pixel_size = Some(512);
            f.metrics.ascent = Some(300);
            f.metrics.descent = Some(200);
            let p: Vec<u8> = (0..300 * 256).map(|i| (i % 256) as u8).collect();
            let mut g = Glyph::new(Bitmap::<Gray8>::from_raw(
                300,
                256,
                &p,
                BitmapLimits::default(),
            )?);
            g.name = Some("wide".into());
            g.codepoints.push(Codepoint::Unicode(65));
            g.metrics.bounds.x = -150;
            g.metrics.bounds.y = -200;
            g.metrics.advance = Some(Vector::new(320, 0));
            f.add_glyph(g)?;
            save(&f.into(), ColorFlavor::Sbix, "sbix-wide", dir)?;
        }
        "read" => {
            let (set, _) = embedded::read(
                &fs::read(&args[2])?,
                fontpixel::sfnt::bitmap::Kind::Color,
                &Default::default(),
            )?;
            println!("{}", describe(&set));
        }
        "timing" => {
            let n: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(4096);
            let mut f = Font::default();
            f.metrics.pixel_size = Some(14);
            f.metrics.ascent = Some(11);
            f.metrics.descent = Some(3);
            for i in 0..n {
                let mut b = Bitmap::<Mono>::new(14, 14)?;
                for y in 0..14 {
                    for x in 0..14 {
                        b.set_pixel(x, y, u8::from((x + y + i as u32).is_multiple_of(3)))?;
                    }
                }
                let mut g = Glyph::new(b);
                g.codepoints.push(Codepoint::Unicode(0x10000 + i as u32));
                g.metrics.advance = Some(Vector::new(14, 0));
                f.add_glyph(g)?;
            }
            let set: FontSet = f.into();
            for flavor in [ColorFlavor::Cbdt, ColorFlavor::Sbix] {
                let options = ColorWriteOptions {
                    flavor,
                    ..Default::default()
                };
                let start = Instant::now();
                let (bytes, _) = embedded::write_color(std::hint::black_box(&set), &options)?;
                let write = start.elapsed().as_secs_f64();
                let start = Instant::now();
                let (loaded, _) = embedded::read(
                    &bytes,
                    fontpixel::sfnt::bitmap::Kind::Color,
                    &Default::default(),
                )?;
                std::hint::black_box(loaded);
                println!(
                    "{}",
                    json!({"flavor":format!("{flavor:?}"),"glyphs":n,"bytes":bytes.len(),"write_seconds":write,"read_seconds":start.elapsed().as_secs_f64()})
                );
            }
        }
        _ => return Err("usage: w56_timing fixtures DIR | read FONT | timing [GLYPHS]".into()),
    }
    Ok(())
}
