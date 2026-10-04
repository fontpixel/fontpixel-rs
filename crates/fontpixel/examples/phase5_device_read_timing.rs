//! E1 memory-only reads with separately verified original fixture pixels.
#[cfg(all(
    feature = "rockbox",
    feature = "u8m",
    feature = "cybiko",
    feature = "playdate",
    feature = "hmzk",
    feature = "sbf"
))]
mod app {
    use fontpixel::formats::{cybiko, hmzk, playdate, rockbox, sbf, u8m};
    use fontpixel::model::{Codepoint, FontSet, GlyphBitmap};
    use fontpixel::resources::{Input, ReadContext, ResourcePath, ResourceResolver};
    use fontpixel::{Report, ResourceLimits, Result};
    use std::{
        collections::BTreeMap,
        hint::black_box,
        path::{Path, PathBuf},
        sync::Mutex,
        time::Instant,
    };
    struct Loader {
        root: PathBuf,
        files: Mutex<BTreeMap<String, Vec<u8>>>,
    }
    impl ResourceResolver for Loader {
        fn resolve(&self, p: &ResourcePath, max: usize) -> Result<Vec<u8>> {
            let path = self.root.join(p.as_str());
            let metadata = std::fs::symlink_metadata(&path)?;
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return Err(fontpixel::Error::InvalidInput(
                    "benchmark resource must be a regular file".into(),
                ));
            }
            if metadata.len() > max as u64 {
                return Err(fontpixel::Error::ResourceLimit {
                    resource: "benchmark resource bytes",
                    limit: max,
                });
            }
            let data = std::fs::read(path)?;
            self.files
                .lock()
                .expect("benchmark cache lock")
                .insert(p.to_string(), data.clone());
            Ok(data)
        }
    }
    struct Memory(BTreeMap<String, Vec<u8>>);
    impl ResourceResolver for Memory {
        fn resolve(&self, p: &ResourcePath, max: usize) -> Result<Vec<u8>> {
            let b = self
                .0
                .get(p.as_str())
                .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?;
            if b.len() > max {
                return Err(fontpixel::Error::ResourceLimit {
                    resource: "benchmark resource bytes",
                    limit: max,
                });
            }
            Ok(b.clone())
        }
    }
    fn read(
        format: &str,
        b: &[u8],
        name: &str,
        files: &dyn ResourceResolver,
    ) -> Result<(FontSet, Report)> {
        let input = Input::new(b).with_name(name).with_resources(files);
        let limits = ResourceLimits::default();
        let c = &mut ReadContext::new(&input, &limits)?;
        match format {
            "rockbox" | "rb11" | "rb12" | "gray" | "large" => rockbox::read(c, &Default::default()),
            "u8m" => u8m::read(c, &Default::default()),
            "cybiko" => cybiko::read(c, &Default::default()),
            "hmzk" => hmzk::read(c, &Default::default()),
            "sbf" => sbf::read(c, &Default::default()),
            "playdate" | "split" | "pft" => playdate::read(c, &Default::default()),
            _ => Err(fontpixel::Error::InvalidInput(
                "unknown device format".into(),
            )),
        }
    }
    type Ink = (i64, Vec<(i64, i64, u8)>);
    type Mappings = BTreeMap<(String, u32), Ink>;
    fn actual(set: &FontSet, format: &str) -> Mappings {
        let mut out = BTreeMap::new();
        let mut unencoded = 0;
        for f in &set.fonts {
            for (_, g) in f.glyphs() {
                let mut ink = Vec::new();
                for y in 0..g.bitmap.height() {
                    for x in 0..g.bitmap.width() {
                        let value = match &g.bitmap {
                            GlyphBitmap::Mono(b) => b.get_pixel(x, y).unwrap() * 255,
                            GlyphBitmap::Gray8(b) => b.get_pixel(x, y).unwrap(),
                        };
                        if value != 0 {
                            ink.push((
                                g.metrics.bounds.x + i64::from(x),
                                g.metrics.bounds.y + i64::from(g.bitmap.height() - 1 - y),
                                value,
                            ));
                        }
                    }
                }
                if g.codepoints.is_empty() {
                    // The original U8M fixture acquires the required empty
                    // glyph-zero slot. Other selected fixtures have none.
                    assert_eq!(format, "u8m", "unexpected unencoded fixture glyph");
                    assert_eq!((g.bitmap.width(), g.bitmap.height()), (0, 0));
                    assert_eq!(g.metrics.advance.map_or(0, |v| v.x), 0);
                    unencoded += 1;
                }
                for cp in &g.codepoints {
                    let value = (g.metrics.advance.map_or(0, |v| v.x), ink.clone());
                    let domain = match cp {
                        Codepoint::Unicode(_) => "unicode",
                        Codepoint::Legacy(_) => "legacy",
                    };
                    if let Some(prior) = out.insert((domain.into(), cp.value()), value.clone()) {
                        assert_eq!(prior, value, "conflicting fixture mapping aliases");
                    }
                }
            }
        }
        assert_eq!(unencoded, usize::from(format == "u8m"));
        out
    }
    fn expected(path: &Path) -> std::result::Result<Mappings, Box<dyn std::error::Error>> {
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
        let mut out = BTreeMap::new();
        for g in value["glyphs"].as_array().ok_or("expected glyph array")? {
            let w = g["width"].as_u64().ok_or("width")?;
            let h = g["height"].as_u64().ok_or("height")?;
            let x = g["x"].as_i64().ok_or("x")?;
            let y = g["y"].as_i64().ok_or("y")?;
            let pixels = g["pixels"].as_array().ok_or("pixels")?;
            if pixels.len() as u64 != w * h {
                return Err("expected raster length".into());
            }
            let mut ink = Vec::new();
            for row in 0..h {
                for col in 0..w {
                    let v = pixels[(row * w + col) as usize]
                        .as_u64()
                        .ok_or("coverage")?;
                    if v != 0 {
                        ink.push((x + col as i64, y + (h - 1 - row) as i64, u8::try_from(v)?));
                    }
                }
            }
            out.insert(
                (
                    g["domain"].as_str().ok_or("mapping domain")?.into(),
                    u32::try_from(g["cp"].as_u64().ok_or("code")?)?,
                ),
                (g["advance"].as_i64().ok_or("advance")?, ink),
            );
        }
        Ok(out)
    }
    pub fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
        let a: Vec<_> = std::env::args().skip(1).collect();
        if a.len() < 3 {
            return Err("usage: FORMAT INPUT ITERATIONS [EXPECTED_JSON]".into());
        }
        let path = Path::new(&a[1]);
        let n = a[2].parse::<usize>()?;
        if n == 0 {
            return Err("iterations must be positive".into());
        }
        let expected_path = a
            .get(3)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(format!("{}.expected.json", path.display())));
        let b = std::fs::read(path)?;
        let name = path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or("input filename")?;
        let loader = Loader {
            root: path.parent().ok_or("input parent")?.into(),
            files: Mutex::new(BTreeMap::new()),
        };
        let (font, report) = read(&a[0], &b, name, &loader)?;
        assert_eq!(
            actual(&font, &a[0]),
            expected(&expected_path)?,
            "independent original pixels, placement, advances and code mappings"
        );
        let memory = Memory(loader.files.into_inner().map_err(|_| "cache lock")?);
        let check = read(&a[0], &b, name, &memory)?;
        assert_eq!(check.0, font, "preloaded reader model");
        assert_eq!(check.1, report, "preloaded reports");
        let pixels: usize = font
            .fonts
            .iter()
            .flat_map(|f| f.glyphs())
            .map(|(_, g)| g.bitmap.width() as usize * g.bitmap.height() as usize)
            .sum();
        for _ in 0..5 {
            black_box(read(&a[0], &b, name, &memory)?);
        }
        let t = Instant::now();
        for _ in 0..n {
            black_box(read(&a[0], black_box(&b), name, &memory)?);
        }
        let read_ns = t.elapsed().as_nanos() / n as u128;
        println!(
            "{}",
            serde_json::json!({"format":a[0],"iterations":n,"warmup":5,"glyphs":font.fonts.iter().map(|f|f.len()).sum::<usize>(),"pixels":pixels,"input_bytes":b.len(),"resource_bytes":memory.0.values().map(Vec::len).sum::<usize>(),"resource_names":memory.0.keys().collect::<Vec<_>>(),"read_ns":read_ns,"write_ns":null,"correctness":"original expected mapping/advance/absolute coverage and cached model/report equality","report_codes":report.entries.iter().map(|v|&v.code).collect::<Vec<_>>()})
        );
        Ok(())
    }
}
#[cfg(all(
    feature = "rockbox",
    feature = "u8m",
    feature = "cybiko",
    feature = "playdate",
    feature = "hmzk",
    feature = "sbf"
))]
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    app::main()
}
#[cfg(not(all(
    feature = "rockbox",
    feature = "u8m",
    feature = "cybiko",
    feature = "playdate",
    feature = "hmzk",
    feature = "sbf"
)))]
fn main() {}
