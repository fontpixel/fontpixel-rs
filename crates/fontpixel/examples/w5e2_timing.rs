//! Original fixtures, native-consumer signatures and quiet E2 timing.
use fontpixel::bitmap::{Bitmap, Gray8};
use fontpixel::formats::{FormatReader, FormatWriter, image, pilfont, sfont, vlw};
use fontpixel::model::{Codepoint, Font, Glyph, GlyphBitmap, Vector};
use fontpixel::resources::{Input, ReadContext, ResourcePath, ResourceResolver, Written};
use fontpixel::{FontSet, ResourceLimits, WriteOptions};
use serde_json::json;
use std::path::{Path, PathBuf};
fn fixture(format: &str, n: u32) -> FontSet {
    let mut f = Font::default();
    f.metrics.ascent = Some(7);
    f.metrics.descent = Some(3);
    f.metrics.line_height = Some(11);
    f.metrics.line_gap = Some(1);
    f.metrics.pixel_size = Some(9);
    f.names.full = Some("Original\0🙂".into());
    f.names.postscript = Some("OriginalPS".into());
    f.names.family = Some("Original é".into());
    f.names.style = Some("Regular".into());
    for i in 0..n {
        let mut bm = Bitmap::<Gray8>::new(3, 4).unwrap();
        for y in 0..4 {
            for x in 0..3 {
                bm.set_pixel(x, y, ((x * 17 + y * 31 + i * 11) % 256) as u8)
                    .unwrap();
            }
        }
        bm.set_pixel(0, 0, 255).unwrap();
        bm.set_pixel(2, 3, 128).unwrap();
        let mut g = Glyph::new(bm);
        g.metrics.bounds.x = if i % 2 == 0 { -1 } else { 1 };
        g.metrics.bounds.y = 2;
        g.metrics.advance = Some(Vector::new(5, 0));
        let cp = if format == "rfont" {
            if i < 3 {
                [65, 256, 0x20001][i as usize]
            } else {
                0x20100 + i
            }
        } else {
            33 + i
        };
        g.codepoints
            .push(if matches!(format, "pilfont" | "imageset") {
                Codepoint::Legacy(cp)
            } else {
                Codepoint::Unicode(cp)
            });
        f.add_glyph(g).unwrap();
    }
    f.into()
}
fn signature(s: &FontSet) -> serde_json::Value {
    let f = s.single().unwrap();
    let mut glyphs = Vec::new();
    for (_, g) in f.glyphs() {
        let mut pixels = Vec::new();
        for y in 0..g.bitmap.height() {
            for x in 0..g.bitmap.width() {
                pixels.push(match &g.bitmap {
                    GlyphBitmap::Mono(b) => b.get_pixel(x, y).unwrap() * 255,
                    GlyphBitmap::Gray8(b) => b.get_pixel(x, y).unwrap(),
                });
            }
        }
        for cp in &g.codepoints {
            glyphs.push(json!({"cp":cp.value(),"domain":if matches!(cp,Codepoint::Unicode(_)){"unicode"}else{"legacy"},"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|v.x),"advance_y":g.metrics.advance.map(|v|v.y),"pixels":pixels}));
        }
    }
    glyphs.sort_by_key(|g| g["cp"].as_u64());
    json!({"ascent":f.metrics.ascent,"descent":f.metrics.descent,"pixel_size":f.metrics.pixel_size,"line_height":f.metrics.line_height,"line_gap":f.metrics.line_gap,"family":f.names.family,"style":f.names.style,"full":f.names.full,"postscript":f.names.postscript,"glyphs":glyphs})
}
struct Directory(PathBuf);
impl ResourceResolver for Directory {
    fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let path = self.0.join(p.as_str());
        let m = match std::fs::symlink_metadata(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(fontpixel::Error::MissingResource(p.to_string()));
            }
            other => other?,
        };
        if !m.is_file() || m.file_type().is_symlink() {
            return Err(fontpixel::Error::InvalidInput(
                "oracle requires regular files".into(),
            ));
        }
        if m.len() > max as u64 {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "oracle bytes",
                limit: max,
            });
        }
        Ok(std::fs::read(path)?)
    }
    fn list(
        &self,
        p: Option<&ResourcePath>,
        max: usize,
        bytes: usize,
    ) -> fontpixel::Result<Vec<ResourcePath>> {
        let root = p.map_or_else(|| self.0.clone(), |p| self.0.join(p.as_str()));
        let mut paths = Vec::new();
        let mut size = 0;
        for e in std::fs::read_dir(root)? {
            let e = e?;
            if !e.file_type()?.is_file() {
                return Err(fontpixel::Error::InvalidInput(
                    "oracle directory contains non-file".into(),
                ));
            }
            let name = e
                .file_name()
                .into_string()
                .map_err(|_| fontpixel::Error::InvalidInput("non UTF8 path".into()))?;
            size += name.len() * 2 + 64;
            if paths.len() == max || size > bytes {
                return Err(fontpixel::Error::ResourceLimit {
                    resource: "oracle listing",
                    limit: max,
                });
            }
            paths.push(ResourcePath::new(name)?);
        }
        Ok(paths)
    }
}
fn read(format: &str, path: &Path) -> fontpixel::Result<FontSet> {
    let data = if format == "imageset" {
        Vec::new()
    } else {
        std::fs::read(path)?
    };
    let root = Directory(if format == "imageset" {
        path.into()
    } else {
        path.parent().unwrap_or(Path::new(".")).into()
    });
    let name = path.file_name().unwrap().to_str().unwrap();
    let input = if format == "imageset" {
        Input::directory(name, &root)
    } else {
        Input::new(&data).with_name(name).with_resources(&root)
    };
    let limits = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &limits)?;
    decode(format, &mut c).map(|v| v.0)
}
fn decode(
    format: &str,
    c: &mut ReadContext<'_>,
) -> fontpixel::Result<(FontSet, fontpixel::Report)> {
    match format {
        "pilfont" => pilfont::read(c, &Default::default()),
        "vlw" | "vlw8" | "vlw9" | "vlw10" => vlw::read(c, &Default::default()),
        "sfont" | "rfont" => sfont::read(
            c,
            &sfont::SfontReadOptions {
                flavor: Some(if format == "rfont" {
                    sfont::SfontFlavor::Rfont
                } else {
                    sfont::SfontFlavor::Sfont
                }),
                ..Default::default()
            },
        ),
        "imageset" => image::ImageCodec::default().read(c, &Default::default()),
        _ => panic!("format"),
    }
}

fn write(s: &FontSet, format: &str, name: &str) -> fontpixel::Result<Written> {
    let o = WriteOptions::new(ResourcePath::new(name)?);
    match format {
        "pilfont" => pilfont::write(s, &o, &Default::default()),
        "vlw" | "vlw8" | "vlw9" | "vlw10" => vlw::write(
            s,
            &o,
            &vlw::VlwWriteOptions {
                version: match format {
                    "vlw8" => vlw::VlwVersion::V8,
                    "vlw9" => vlw::VlwVersion::V9,
                    "vlw10" => vlw::VlwVersion::V10,
                    _ => vlw::VlwVersion::V11,
                },
                ..Default::default()
            },
        ),
        "sfont" | "rfont" => sfont::write(
            s,
            &o,
            &sfont::SfontWriteOptions {
                flavor: if format == "rfont" {
                    sfont::SfontFlavor::Rfont
                } else {
                    sfont::SfontFlavor::Sfont
                },
                ..Default::default()
            },
        ),
        "imageset" => image::ImageCodec {
            write_options: image::ImageWriteOptions {
                layout: image::ImageLayout::Imageset,
                ..Default::default()
            },
            ..Default::default()
        }
        .write(s, &o),
        _ => panic!("format"),
    }
}
fn save(w: Written, path: &Path) {
    let parent = path.parent().unwrap_or(Path::new("."));
    for a in w.output.assets() {
        let p = parent.join(a.path.as_str());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, &a.bytes).unwrap();
    }
}

struct Cached {
    name: String,
    primary: Vec<u8>,
    directory: bool,
    assets: std::collections::BTreeMap<ResourcePath, Vec<u8>>,
}
impl ResourceResolver for Cached {
    fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let b = self
            .assets
            .get(p)
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?;
        if b.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "timing cached asset",
                limit: max,
            });
        }
        Ok(b.clone())
    }
    fn list(
        &self,
        p: Option<&ResourcePath>,
        max: usize,
        bytes: usize,
    ) -> fontpixel::Result<Vec<ResourcePath>> {
        assert!(p.is_none());
        if self.assets.len() > max
            || self
                .assets
                .keys()
                .map(|p| 2 * (std::mem::size_of::<ResourcePath>() + p.as_str().len()))
                .sum::<usize>()
                > bytes
        {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "timing cached listing",
                limit: bytes,
            });
        }
        Ok(self.assets.keys().cloned().collect())
    }
}
impl Cached {
    fn load(format: &str, path: &Path) -> fontpixel::Result<Self> {
        let limits = ResourceLimits::default();
        let directory = format == "imageset";
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        let root = Directory(if directory {
            path.into()
        } else {
            path.parent().unwrap_or(Path::new(".")).into()
        });
        let primary = if directory {
            Vec::new()
        } else {
            root.resolve(&ResourcePath::new(&name)?, limits.max_input_bytes)?
        };
        let paths = if directory {
            root.list(None, limits.max_resources, limits.max_total_bytes)?
        } else if format == "pilfont" {
            ["png", "gif", "pbm"]
                .into_iter()
                .map(|ext| {
                    ResourcePath::new(Path::new(&name).with_extension(ext).to_str().unwrap())
                })
                .collect::<fontpixel::Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        let mut assets = std::collections::BTreeMap::new();
        let mut total = primary.len();
        for p in paths {
            match root.resolve(
                &p,
                limits
                    .max_resource_bytes
                    .min(limits.max_total_bytes - total),
            ) {
                Ok(b) => {
                    total += b.len();
                    assets.insert(p, b);
                }
                Err(fontpixel::Error::MissingResource(_)) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(Self {
            name,
            primary,
            directory,
            assets,
        })
    }
    fn read(&self, format: &str) -> fontpixel::Result<(FontSet, fontpixel::Report)> {
        let input = if self.directory {
            Input::directory(&self.name, self)
        } else {
            Input::new(&self.primary)
                .with_name(&self.name)
                .with_resources(self)
        };
        let l = ResourceLimits::default();
        let mut c = ReadContext::new(std::hint::black_box(&input), &l)?;
        decode(format, &mut c)
    }
}
fn bench_read(format: &str, path: &Path, iterations: u64) {
    assert!(iterations > 0, "positive iterations required");
    let cached = Cached::load(format, path).unwrap();
    let expected = cached.read(format).unwrap();
    for _ in 0..5 {
        assert_eq!(cached.read(format).unwrap(), expected);
    }
    let mut total = 0u128;
    for _ in 0..iterations {
        let start = std::time::Instant::now();
        let result = cached.read(format).unwrap();
        total += start.elapsed().as_nanos();
        assert_eq!(result, expected);
        std::hint::black_box(result);
    }
    println!(
        "{}",
        json!({"format":format,"mode":"memory_read","total_ns":total,"actual_iterations":iterations,"warmup_iterations":5,"glyphs":expected.0.fonts.iter().map(Font::len).sum::<usize>(),"primary_bytes":cached.primary.len(),"resource_bytes":cached.assets.values().map(Vec::len).sum::<usize>(),"resource_files":cached.assets.len(),"read_report_entries":expected.1.entries.len(),"full_model_and_report_verified":true})
    );
}
fn main() {
    let a: Vec<_> = std::env::args().collect();
    match a[1].as_str() {
        "emit" => {
            let format = &a[2];
            let path = Path::new(&a[3]);
            let count = a.get(4).map_or(if format == "rfont" { 3 } else { 5 }, |n| {
                n.parse::<u32>().unwrap()
            });
            let s = fixture(format, count);
            let w = write(&s, format, path.file_name().unwrap().to_str().unwrap()).unwrap();
            save(w, path);
            println!(
                "{}",
                json!({"source":signature(&s),"native":signature(&read(format,path).unwrap())})
            );
        }
        "inspect" => println!("{}", signature(&read(&a[2], Path::new(&a[3])).unwrap())),
        "rewrite" => {
            let path = Path::new(&a[4]);
            let s = read(&a[2], Path::new(&a[3])).unwrap();
            save(
                write(&s, &a[2], path.file_name().unwrap().to_str().unwrap()).unwrap(),
                path,
            );
            println!("{}", signature(&read(&a[2], path).unwrap()));
        }
        "bench-read" => bench_read(
            &a[2],
            Path::new(&a[3]),
            a.get(4).map_or(100, |n| n.parse().unwrap()),
        ),
        "bench" => {
            let n = a.get(2).map_or(100, |n| n.parse::<u32>().unwrap());
            for format in ["pilfont", "vlw", "sfont", "rfont", "imageset"] {
                let count = if format == "pilfont" {
                    n.min(200)
                } else if format == "sfont" {
                    n.min(94)
                } else {
                    n
                };
                let s = fixture(format, count);
                let name = match format {
                    "pilfont" => "font.pil",
                    "vlw" => "font.vlw",
                    "imageset" => "glyphs",
                    _ => "font.png",
                };
                let start = std::time::Instant::now();
                let w = write(&s, format, name).unwrap();
                println!(
                    "{format}\t{count}\t{}\t{}",
                    w.output
                        .assets()
                        .iter()
                        .map(|a| a.bytes.len())
                        .sum::<usize>(),
                    start.elapsed().as_nanos()
                );
            }
        }
        _ => panic!(
            "emit FORMAT PATH [N] | inspect FORMAT PATH | rewrite FORMAT IN OUT | bench [N] | bench-read FORMAT PATH [ITERATIONS]"
        ),
    }
}
