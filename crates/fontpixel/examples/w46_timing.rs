//! W4.6 interoperability adapter and short packed-row timing probe.
use fontpixel::formats::{amiga, gem, snf};
use fontpixel::resources::{Input, ReadContext, ResourcePath, ResourceResolver};
use fontpixel::{FontSet, ReadOptions, ResourceLimits, WriteOptions};
use std::path::{Path, PathBuf};
struct Resources(PathBuf);
impl ResourceResolver for Resources {
    fn resolve(&self, path: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        use std::io::Read;
        let mut file = std::fs::File::open(self.0.join(path.as_str()))?;
        let mut data = Vec::new();
        file.by_ref().take(max as u64 + 1).read_to_end(&mut data)?;
        if data.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "example sidecar",
                limit: max,
            });
        }
        Ok(data)
    }
}
fn read(format: &str, path: &Path) -> fontpixel::Result<FontSet> {
    let bytes = std::fs::read(path)?;
    let resources = Resources(path.parent().unwrap_or(Path::new(".")).to_owned());
    let input = Input::new(&bytes).with_resources(&resources);
    let limits = ResourceLimits::default();
    if format == "snf1" {
        return Ok(snf::read(
            &mut ReadContext::new(&input, &limits)?,
            &snf::SnfReadOptions {
                bitmap: Some(Default::default()),
                ..Default::default()
            },
        )?
        .0);
    }
    Ok(fontpixel::registry()
        .read(
            &input,
            &ReadOptions {
                format: Some(format.into()),
                ..Default::default()
            },
        )?
        .fonts)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() < 4 {
        return Err("usage: w46_timing dump FORMAT INPUT | write INPUT_FORMAT INPUT OUTPUT_FORMAT OUTPUT | bench FORMAT INPUT".into());
    }
    let s = read(&a[2], Path::new(&a[3]))?;
    match a[1].as_str() {
        "dump" => {
            let fonts:Vec<_>=s.fonts.iter().map(|f|{let glyphs:Vec<_>=f.glyphs().map(|(_,g)|{let cp:Vec<_>=g.codepoints.iter().map(|c|match c{fontpixel::model::Codepoint::Unicode(v)|fontpixel::model::Codepoint::Legacy(v)=>*v}).collect();serde_json::json!({"codes":cp,"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|v.x),"bytes":g.bitmap.as_bytes()})}).collect();serde_json::json!({"glyphs":glyphs,"ascent":f.metrics.ascent,"descent":f.metrics.descent,"name":f.names.full})}).collect();
            println!("{}", serde_json::to_string(&fonts)?);
        }
        "write" => {
            if a.len() < 6 {
                return Err("write requires output format and output path".into());
            }
            let dest = Path::new(&a[5]);
            let w = WriteOptions::new(ResourcePath::new(
                dest.file_name()
                    .ok_or("output name missing")?
                    .to_str()
                    .ok_or("output name is not UTF-8")?,
            )?);
            let result = match a[4].as_str() {
                "gem" => gem::write(
                    &s,
                    &w,
                    &gem::GemWriteOptions {
                        unicode_as_legacy: true,
                        ..Default::default()
                    },
                )?,
                "amiga" => amiga::write(&s, &w, &Default::default())?,
                "snf" => snf::write(&s, &w, &Default::default())?,
                _ => fontpixel::write(&s, &a[4], &w)?,
            };
            for asset in result.output.assets() {
                let path = dest
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join(asset.path.as_str());
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(path, &asset.bytes)?;
            }
            eprintln!(
                "{} diagnostics; loss={}",
                result.report.entries.len(),
                result.report.has_loss()
            );
        }
        "bench" => {
            let bytes = std::fs::read(&a[3])?;
            let mut registry = fontpixel::registry();
            if a[2] == "snf1" {
                registry.set_snf_read_options(snf::SnfReadOptions {
                    bitmap: Some(Default::default()),
                    ..Default::default()
                });
            }
            let options = ReadOptions {
                format: Some(a[2].trim_end_matches('1').into()),
                ..Default::default()
            };
            let now = std::time::Instant::now();
            let mut glyphs = 0;
            for _ in 0..100 {
                let value = registry.read(&Input::new(&bytes), &options)?;
                glyphs += value.fonts.fonts.iter().map(|f| f.len()).sum::<usize>();
                std::hint::black_box(value);
            }
            println!("100 reads: {:?}, {} glyphs", now.elapsed(), glyphs);
        }
        _ => return Err("unknown operation".into()),
    }
    Ok(())
}
