//! Original W5.A2 fixtures, native consumer bridge and quiet timing.
#[cfg(all(
    feature = "bbc",
    feature = "wyse",
    feature = "grasp",
    feature = "hrcg",
    feature = "xbin",
    feature = "consoleet",
    feature = "zapfont",
    feature = "comfont"
))]
mod app {
    use fontpixel::bitmap::{Bitmap, BitmapLimits, Mono};
    use fontpixel::formats::{
        WriteOptions, bbc, comfont, consoleet, grasp, hrcg, wyse, xbin, zapfont,
    };
    use fontpixel::model::{Codepoint, Font, FontSet, Glyph, GlyphBitmap, Vector};
    use fontpixel::resources::{OutputBundle, ReadContext, ResourceResolver};
    use fontpixel::{Input, Report, ResourceLimits, ResourcePath, Result};
    use std::collections::BTreeMap;
    struct Files(BTreeMap<String, Vec<u8>>);
    impl ResourceResolver for Files {
        fn resolve(&self, p: &ResourcePath, max: usize) -> Result<Vec<u8>> {
            let b = self
                .0
                .get(p.as_str())
                .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?;
            if b.len() > max {
                return Err(fontpixel::Error::ResourceLimit {
                    resource: "bridge resource",
                    limit: max,
                });
            }
            Ok(b.clone())
        }
        fn list(
            &self,
            d: Option<&ResourcePath>,
            max: usize,
            bytes: usize,
        ) -> Result<Vec<ResourcePath>> {
            if d.is_some()
                || self.0.len() > max
                || self
                    .0
                    .keys()
                    .map(|x| 2 * (size_of::<ResourcePath>() + x.len()))
                    .sum::<usize>()
                    > bytes
            {
                return Err(fontpixel::Error::InvalidInput(
                    "bridge directory bounds".into(),
                ));
            }
            self.0.keys().map(ResourcePath::new).collect()
        }
    }
    fn family(k: &str) -> &str {
        k.split('-').next().unwrap_or(k)
    }
    fn comopts(k: &str) -> comfont::ComfontWriteOptions {
        use comfont::*;
        ComfontWriteOptions {
            variant: if k.contains("fontedit") {
                ComfontVariant::Fontedit
            } else if k.contains("tsr") {
                ComfontVariant::FraptTsr
            } else if k.contains("letafont") {
                ComfontVariant::Letafont
            } else if k.contains("mania") {
                ComfontVariant::Mania
            } else if k.contains("psfcom") {
                ComfontVariant::Psfcom
            } else if k.contains("udg") {
                ComfontVariant::Udg
            } else {
                ComfontVariant::Frapt
            },
            cpm_target: if k.contains("spectrum") {
                CpmTarget::Spectrum
            } else if k.contains("cpc") {
                CpmTarget::Cpc
            } else {
                CpmTarget::Pcw
            },
            ..Default::default()
        }
    }
    fn read(k: &str, b: &[u8], files: &Files) -> Result<(FontSet, Report)> {
        let l = ResourceLimits::default();
        let input = if family(k) == "consoleet" {
            Input::directory("font", files)
        } else {
            Input::new(b)
        };
        let c = &mut ReadContext::new(&input, &l)?;
        match family(k) {
            "bbc" => bbc::read(c, &Default::default()),
            "wyse" => wyse::read(c, &Default::default()),
            "grasp" => grasp::read(c, &Default::default()),
            "hrcg" => hrcg::read(c, &Default::default()),
            "xbin" => xbin::read(c, &Default::default()),
            "consoleet" => consoleet::read(c, &Default::default()),
            "zapfont" => zapfont::read(c, &Default::default()),
            "comfont" => comfont::read(c, &Default::default()),
            _ => Err(fontpixel::Error::InvalidInput("unknown family".into())),
        }
    }
    fn write(k: &str, s: &FontSet) -> Result<OutputBundle> {
        let o = WriteOptions::new(ResourcePath::new("font")?);
        let w = match family(k) {
            "bbc" => bbc::write(s, &o, &Default::default()),
            "wyse" => wyse::write(s, &o, &Default::default()),
            "grasp" => grasp::write(s, &o, &Default::default()),
            "hrcg" => hrcg::write(s, &o, &Default::default()),
            "xbin" => xbin::write(
                s,
                &o,
                &xbin::XbinWriteOptions {
                    count: if k.contains("512") {
                        xbin::CharacterCount::Chars512
                    } else {
                        xbin::CharacterCount::Auto
                    },
                    ..Default::default()
                },
            ),
            "consoleet" => consoleet::write(s, &o, &Default::default()),
            "zapfont" => zapfont::write(
                s,
                &o,
                &zapfont::ZapfontWriteOptions {
                    variant: if k.contains("old") {
                        zapfont::ZapVariant::Old
                    } else {
                        zapfont::ZapVariant::Redraw
                    },
                    ..Default::default()
                },
            ),
            "comfont" => comfont::write(s, &o, &comopts(k)),
            _ => Err(fontpixel::Error::InvalidInput("unknown family".into())),
        }?;
        Ok(w.output)
    }
    fn fixture(k: &str) -> Result<FontSet> {
        let mut f = Font::default();
        let h = if k.contains("32") {
            32
        } else if family(k) == "wyse" || k.contains("16") {
            16
        } else {
            8
        };
        let width: u32 = if family(k) == "hrcg" { 14 } else { 8 };
        let count = if k.contains("512") {
            512
        } else if ["xbin", "comfont"].contains(&family(k)) || k == "zapfont-redraw" {
            256
        } else if family(k) == "hrcg" {
            96
        } else {
            3
        };
        let first = if family(k) == "hrcg" {
            32
        } else if count >= 256 {
            0
        } else {
            65
        };
        for i in 0..count {
            let mut raw = vec![0u8; (width as usize).div_ceil(8) * h as usize];
            for y in 0..h {
                for x in 0..width {
                    if (x + y + i) % 3 == 0 && (width != 14 || x % 2 == y % 2) {
                        raw[(y * width.div_ceil(8) + x / 8) as usize] |= 128 >> (x % 8);
                    }
                }
            }
            let mut g = Glyph::new(Bitmap::<Mono>::from_raw(
                width,
                h,
                &raw,
                BitmapLimits::default(),
            )?);
            g.codepoints.push(Codepoint::Legacy(first + i));
            g.metrics.advance = Some(Vector::new(i64::from(width), 0));
            f.add_glyph(g)?;
        }
        if k == "zapfont-redraw" {
            let mut g = Glyph::new(Bitmap::<Mono>::from_raw(
                8,
                h,
                &vec![0xa5; h as usize],
                BitmapLimits::default(),
            )?);
            g.codepoints.push(Codepoint::Unicode(0x2603));
            g.metrics.advance = Some(Vector::new(8, 0));
            f.add_glyph(g)?;
        }
        f.update_bounds()?;
        Ok(f.into())
    }
    fn signature(s: &FontSet) -> serde_json::Value {
        serde_json::json!({"glyphs":s.fonts[0].glyphs().map(|(_,g)|{let mut pixels=Vec::new();for y in 0..g.bitmap.height(){for x in 0..g.bitmap.width(){pixels.push(match &g.bitmap{GlyphBitmap::Mono(v)=>if v.get_pixel(x,y).unwrap_or(0)!=0{255}else{0},GlyphBitmap::Gray8(v)=>v.get_pixel(x,y).unwrap_or(0)});}}serde_json::json!({"codes":g.codepoints.iter().map(|c|c.value()).collect::<Vec<_>>(),"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|v.x),"pixels":pixels})}).collect::<Vec<_>>()})
    }
    fn load(k: &str, path: &str) -> Result<(Vec<u8>, Files)> {
        if family(k) == "consoleet" {
            let mut f = BTreeMap::new();
            for entry in std::fs::read_dir(path)? {
                let e = entry?;
                f.insert(
                    e.file_name().to_string_lossy().into_owned(),
                    std::fs::read(e.path())?,
                );
            }
            Ok((Vec::new(), Files(f)))
        } else {
            Ok((std::fs::read(path)?, Files(BTreeMap::new())))
        }
    }
    fn save(out: OutputBundle, path: &str) -> Result<()> {
        if out.is_directory() {
            std::fs::create_dir_all(path)?;
            for a in out.assets() {
                let file =
                    a.path.as_str().strip_prefix("font/").ok_or_else(|| {
                        fontpixel::Error::InvalidInput("bridge bundle path".into())
                    })?;
                std::fs::write(std::path::Path::new(path).join(file), &a.bytes)?;
            }
        } else {
            std::fs::write(path, out.into_single()?)?;
        }
        Ok(())
    }
    pub fn main() -> Result<()> {
        let a: Vec<_> = std::env::args().skip(1).collect();
        if a.len() < 3 {
            return Err(fontpixel::Error::InvalidInput("fixture KIND PATH | inspect KIND PATH | convert KIND INPUT OUTPUT | time KIND PATH [N]".into()));
        }
        match a[0].as_str() {
            "fixture" => save(write(&a[1], &fixture(&a[1])?)?, &a[2])?,
            "inspect" => {
                let (b, f) = load(&a[1], &a[2])?;
                let (s, r) = read(&a[1], &b, &f)?;
                println!("{}", signature(&s));
                eprint!("{r}");
            }
            "convert" => {
                let (b, f) = load(&a[1], &a[2])?;
                save(write(&a[1], &read(&a[1], &b, &f)?.0)?, &a[3])?;
            }
            "time" => {
                let (b, f) = load(&a[1], &a[2])?;
                let s = read(&a[1], &b, &f)?.0;
                let n = a
                    .get(3)
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(100)
                    .max(1);
                let t = std::time::Instant::now();
                for _ in 0..n {
                    std::hint::black_box(read(&a[1], &b, &f)?);
                }
                let rn = t.elapsed().as_nanos() / n as u128;
                let t = std::time::Instant::now();
                for _ in 0..n {
                    std::hint::black_box(write(&a[1], &s)?);
                }
                println!(
                    "{}",
                    serde_json::json!({"kind":a[1],"iterations":n,"glyphs":s.fonts[0].len(),"bytes":b.len()+f.0.values().map(Vec::len).sum::<usize>(),"read_ns":rn,"write_ns":t.elapsed().as_nanos()/n as u128})
                );
            }
            _ => {
                return Err(fontpixel::Error::InvalidInput(
                    "unknown bridge command".into(),
                ));
            }
        }
        Ok(())
    }
}
#[cfg(all(
    feature = "bbc",
    feature = "wyse",
    feature = "grasp",
    feature = "hrcg",
    feature = "xbin",
    feature = "consoleet",
    feature = "zapfont",
    feature = "comfont"
))]
fn main() -> fontpixel::Result<()> {
    app::main()
}
#[cfg(not(all(
    feature = "bbc",
    feature = "wyse",
    feature = "grasp",
    feature = "hrcg",
    feature = "xbin",
    feature = "consoleet",
    feature = "zapfont",
    feature = "comfont"
)))]
fn main() {}
