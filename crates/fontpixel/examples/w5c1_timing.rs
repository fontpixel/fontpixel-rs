//! Original C1 oracle adapter and quiet in-memory benchmark harness.
use fontpixel::formats::{daisy, polyprint, printshop, pxl, signum, tasprint};
use fontpixel::model::FontSet;
use fontpixel::resources::{Input, OutputBundle, ReadContext, ResourcePath, ResourceResolver};
use fontpixel::{ResourceLimits, Result, WriteOptions};
use std::{collections::BTreeMap, hint::black_box, path::Path};
struct Files(BTreeMap<String, Vec<u8>>);
impl ResourceResolver for Files {
    fn resolve(&self, p: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let b = self
            .0
            .get(p.as_str())
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?;
        if b.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "oracle sidecar",
                limit: max,
            });
        }
        Ok(b.clone())
    }
}
fn read(fmt: &str, b: &[u8], name: &str, files: &Files) -> Result<FontSet> {
    let input = Input::new(b).with_name(name).with_resources(files);
    let limits = ResourceLimits::default();
    let mut c = ReadContext::new(&input, &limits)?;
    Ok(match fmt {
        "daisy" => daisy::read(&mut c, &Default::default())?,
        "polyprint" => polyprint::read(&mut c, &Default::default())?,
        "printshop" => printshop::read(&mut c, &Default::default())?,
        "tasprint" => tasprint::read(&mut c, &Default::default())?,
        "pxl" => pxl::read(
            &mut c,
            &pxl::PxlReadOptions {
                tfm: files
                    .0
                    .contains_key("font.tfm")
                    .then(|| ResourcePath::new("font.tfm").unwrap()),
                ..Default::default()
            },
        )?,
        "signum" => signum::read(&mut c, &Default::default())?,
        _ => return Err(fontpixel::Error::InvalidInput("unknown C1 format".into())),
    }
    .0)
}
fn write(fmt: &str, b: &[u8], s: &FontSet, name: &str) -> Result<OutputBundle> {
    let o = WriteOptions::new(ResourcePath::new(name)?);
    Ok(match fmt {
        "daisy" => daisy::write(
            s,
            &o,
            &daisy::DaisyWriteOptions {
                version: if b.starts_with(b"B") {
                    daisy::DaisyVersion::Magnified
                } else if b.starts_with(b"3") {
                    daisy::DaisyVersion::V3
                } else {
                    daisy::DaisyVersion::V2
                },
                allow_control_bytes: true,
                ..Default::default()
            },
        )?,
        "polyprint" => polyprint::write(s, &o, &Default::default())?,
        "printshop" => printshop::write(s, &o, &Default::default())?,
        "tasprint" => tasprint::write(
            s,
            &o,
            &tasprint::TasprintWriteOptions {
                version: if b.starts_with(b"PLUS3") {
                    tasprint::TasprintVersion::Plus3
                } else if b.len().is_multiple_of(5248) {
                    tasprint::TasprintVersion::Cpc
                } else if b.len() == 4096 || b.len() == 4064 {
                    tasprint::TasprintVersion::Pcw
                } else {
                    tasprint::TasprintVersion::Spectrum48k
                },
                pcw_count: if b.len() == 4064 { 127 } else { 128 },
                ..Default::default()
            },
        )?,
        "pxl" => pxl::write(
            s,
            &o,
            &pxl::PxlWriteOptions {
                version: if b[3] == 0xea {
                    pxl::PxlVersion::V1002
                } else {
                    pxl::PxlVersion::V1001
                },
                ..Default::default()
            },
        )?,
        "signum" => signum::write(
            s,
            &o,
            &signum::SignumWriteOptions {
                version: if b.starts_with(b"ps09") {
                    signum::SignumVersion::P9
                } else if b.starts_with(b"ps24") {
                    signum::SignumVersion::P24
                } else if b.starts_with(b"ls30") {
                    signum::SignumVersion::L30
                } else {
                    signum::SignumVersion::E24
                },
                ..Default::default()
            },
        )?,
        _ => return Err(fontpixel::Error::InvalidInput("unknown C1 format".into())),
    }
    .output)
}
fn dump(s: &FontSet) {
    let out=s.fonts.iter().map(|f|serde_json::json!({"ascent":f.metrics.ascent,"descent":f.metrics.descent,"line_height":f.metrics.line_height,"space_width":f.metrics.space_width,"glyphs":f.glyphs().map(|(_,g)|serde_json::json!({"codes":g.codepoints.iter().map(|c|c.value()).collect::<Vec<_>>(),"width":g.bitmap.width(),"height":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|v|v.x),"scalable_advance":g.metrics.scalable_advance.map(|v|v.x),"bytes":g.bitmap.as_bytes()})).collect::<Vec<_>>()})).collect::<Vec<_>>();
    println!("{}", serde_json::to_string(&out).unwrap());
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let a = std::env::args().skip(1).collect::<Vec<_>>();
    if a.len() < 3 {
        return Err("usage: dump|write|bench FORMAT INPUT [OUTPUT]".into());
    }
    let path = Path::new(&a[2]);
    let name = path
        .file_name()
        .ok_or("input filename")?
        .to_str()
        .ok_or("UTF8 filename")?;
    let b = std::fs::read(path)?;
    let mut files = Files(BTreeMap::new());
    if a[1] == "daisy" && b.starts_with(b"B") {
        for i in 2..=4 {
            let key = format!(
                "{}{i}",
                &name[..name.char_indices().last().ok_or("empty filename")?.0]
            );
            let p = path.with_file_name(&key);
            if p.exists() {
                files.0.insert(key, std::fs::read(p)?);
            }
        }
    }
    if path.with_file_name("font.tfm").exists() {
        files.0.insert(
            "font.tfm".into(),
            std::fs::read(path.with_file_name("font.tfm"))?,
        );
    }
    let s = read(&a[1], &b, name, &files)?;
    match a[0].as_str() {
        "dump" => dump(&s),
        "write" => {
            let p = Path::new(a.get(3).ok_or("output path")?);
            let n = p
                .file_name()
                .ok_or("output filename")?
                .to_str()
                .ok_or("UTF8 output")?;
            for asset in write(&a[1], &b, &s, n)?.assets() {
                std::fs::write(p.with_file_name(asset.path.as_str()), &asset.bytes)?;
            }
        }
        "tfm" => {
            if a[1] != "pxl" {
                return Err("TFM output requires PXL".into());
            }
            let p = Path::new(a.get(3).ok_or("output path")?);
            let name = p
                .file_name()
                .ok_or("filename")?
                .to_str()
                .ok_or("UTF8 filename")?;
            let result = pxl::write(
                &s,
                &WriteOptions::new(ResourcePath::new(name)?),
                &pxl::PxlWriteOptions {
                    tfm: Some(ResourcePath::new("font.tfm")?),
                    ..Default::default()
                },
            )?;
            for asset in result.output.assets() {
                std::fs::write(p.with_file_name(asset.path.as_str()), &asset.bytes)?;
            }
        }
        "bench" => {
            let output = write(&a[1], &b, &s, name)?;
            let backfiles = Files(
                output
                    .assets()
                    .iter()
                    .map(|a| (a.path.to_string(), a.bytes.clone()))
                    .collect(),
            );
            let primary = backfiles.0.get(name).ok_or("primary output")?;
            let back = read(&a[1], primary, name, &backfiles)?;
            assert_eq!(
                write(&a[1], primary, &back, name)?,
                output,
                "stable canonical bundle"
            );
            for _ in 0..10 {
                black_box(read(&a[1], &b, name, &files)?);
                black_box(write(&a[1], &b, &s, name)?);
            }
            let iterations = 300;
            let start = std::time::Instant::now();
            for _ in 0..iterations {
                black_box(read(&a[1], &b, name, &files)?);
            }
            let read_us = start.elapsed().as_secs_f64() * 1e6 / f64::from(iterations);
            let start = std::time::Instant::now();
            for _ in 0..iterations {
                black_box(write(&a[1], &b, &s, name)?);
            }
            let write_us = start.elapsed().as_secs_f64() * 1e6 / f64::from(iterations);
            println!(
                "{}",
                serde_json::json!({"format":a[1],"glyphs":s.fonts.iter().map(|f|f.len()).sum::<usize>(),"input_bytes":b.len(),"output_bytes":output.assets().iter().map(|a|a.bytes.len()).sum::<usize>(),"iterations":iterations,"read_us":read_us,"write_us":write_us})
            );
        }
        _ => return Err("unknown command".into()),
    }
    Ok(())
}
