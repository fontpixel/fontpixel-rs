//! Original MIT C2 oracle adapter and quiet in-memory timing harness.
use fontpixel::formats::{catalyst, chiwriter, dashen, hppcl, prebuilt, riscos, xerox};
use fontpixel::model::{Codepoint, FontSet};
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
                resource: "oracle resource",
                limit: max,
            });
        }
        Ok(b.clone())
    }
}
fn has(fmt: &str, key: &str) -> bool {
    fmt.split(':').any(|s| s == key)
}
fn chi_layout(fmt: &str) -> chiwriter::ChiwriterLayout {
    if has(fmt, "absolute") {
        chiwriter::ChiwriterLayout::AbsoluteOffsets
    } else if has(fmt, "relative") {
        chiwriter::ChiwriterLayout::RelativeOffsets
    } else {
        chiwriter::ChiwriterLayout::Contiguous
    }
}
fn x_variant(fmt: &str, b: &[u8]) -> xerox::XeroxVariant {
    if has(fmt, "alto") {
        xerox::XeroxVariant::Alto
    } else if has(fmt, "prepress") || b.first() == Some(&0x10) {
        xerox::XeroxVariant::Prepress
    } else {
        xerox::XeroxVariant::Bitblt
    }
}
fn read(fmt: &str, b: &[u8], name: &str, files: &Files) -> Result<FontSet> {
    let i = Input::new(b).with_name(name).with_resources(files);
    let l = ResourceLimits::default();
    let mut c = ReadContext::new(&i, &l)?;
    Ok(match fmt.split(':').next().unwrap_or("") {
        "catalyst" => catalyst::read(
            &mut c,
            &catalyst::CatalystReadOptions {
                offset_base: if has(fmt, "data") {
                    catalyst::CatalystOffsetBase::GlyphData
                } else {
                    catalyst::CatalystOffsetBase::ResourceBody
                },
            },
        )?,
        "chiwriter" => chiwriter::read(
            &mut c,
            &chiwriter::ChiwriterReadOptions {
                layout: chi_layout(fmt),
                stride: has(fmt, "stride3").then_some(3),
                ..Default::default()
            },
        )?,
        "dashen" => dashen::read(&mut c, &Default::default())?,
        "hppcl" => hppcl::read(
            &mut c,
            &hppcl::HppclReadOptions {
                allow_legacy_descriptor_size: has(fmt, "legacy"),
            },
        )?,
        "prebuilt" => prebuilt::read(&mut c, &Default::default())?,
        "riscos" => riscos::read(
            &mut c,
            &riscos::RiscosReadOptions {
                metrics_path: files
                    .0
                    .contains_key("IntMetrics")
                    .then(|| ResourcePath::new("IntMetrics").unwrap()),
                ..Default::default()
            },
        )?,
        "xerox" => xerox::read(
            &mut c,
            &xerox::XeroxReadOptions {
                variant: Some(x_variant(fmt, b)),
                ..Default::default()
            },
        )?,
        _ => return Err(fontpixel::Error::InvalidInput("unknown C2 format".into())),
    }
    .0)
}
fn write(fmt: &str, b: &[u8], s: &FontSet, name: &str, files: &Files) -> Result<OutputBundle> {
    let o = WriteOptions::new(ResourcePath::new(name)?);
    Ok(match fmt.split(':').next().unwrap_or("") {
        "catalyst" => catalyst::write(
            s,
            &o,
            &catalyst::CatalystWriteOptions {
                offset_base: if has(fmt, "data") {
                    catalyst::CatalystOffsetBase::GlyphData
                } else {
                    catalyst::CatalystOffsetBase::ResourceBody
                },
                ..Default::default()
            },
        )?,
        "chiwriter" => chiwriter::write(
            s,
            &o,
            &chiwriter::ChiwriterWriteOptions {
                version: if b[0] == 16 {
                    chiwriter::ChiwriterVersion::V3
                } else {
                    chiwriter::ChiwriterVersion::V4
                },
                layout: chi_layout(fmt),
                stride: has(fmt, "stride3").then_some(3),
                ..Default::default()
            },
        )?,
        "dashen" => dashen::write(
            s,
            &o,
            &dashen::DashenWriteOptions {
                version: if s.fonts.len() > 1 {
                    dashen::DashenVersion::V4
                } else {
                    dashen::DashenVersion::V3
                },
                ..Default::default()
            },
        )?,
        "hppcl" => hppcl::write(
            s,
            &o,
            &hppcl::HppclWriteOptions {
                orientation: if has(fmt, "o1") {
                    hppcl::HppclOrientation::Landscape
                } else if has(fmt, "o2") {
                    hppcl::HppclOrientation::ReversePortrait
                } else if has(fmt, "o3") {
                    hppcl::HppclOrientation::ReverseLandscape
                } else {
                    hppcl::HppclOrientation::Portrait
                },
                compression: if has(fmt, "rle") {
                    hppcl::HppclCompression::Rle
                } else {
                    hppcl::HppclCompression::Raw
                },
                continuation_bytes: if has(fmt, "chunks") { 17 } else { 32767 },
                ..Default::default()
            },
        )?,
        "prebuilt" => prebuilt::write(
            s,
            &o,
            &prebuilt::PrebuiltWriteOptions {
                byte_order: if b[..2] == [0, 0] {
                    prebuilt::PrebuiltByteOrder::Little
                } else {
                    prebuilt::PrebuiltByteOrder::Big
                },
                ..Default::default()
            },
        )?,
        "riscos" => riscos::write(
            s,
            &o,
            &riscos::RiscosWriteOptions {
                version: if !b.starts_with(b"FONT") {
                    riscos::RiscosVersion::Xy
                } else {
                    match b[5] {
                        4 => riscos::RiscosVersion::V4,
                        5 => riscos::RiscosVersion::V5,
                        7 => riscos::RiscosVersion::V7,
                        8 => riscos::RiscosVersion::V8,
                        _ => riscos::RiscosVersion::V6,
                    }
                },
                depth: if !b.starts_with(b"FONT") || b[4] == 4 {
                    riscos::RiscosDepth::Gray4
                } else {
                    riscos::RiscosDepth::Mono
                },
                compression: if has(fmt, "packed") {
                    riscos::RiscosCompression::Packed
                } else {
                    riscos::RiscosCompression::Raw
                },
                metrics_path: files
                    .0
                    .contains_key("IntMetrics")
                    .then(|| ResourcePath::new("IntMetrics").unwrap()),
                ..Default::default()
            },
        )?,
        "xerox" => xerox::write(
            s,
            &o,
            &xerox::XeroxWriteOptions {
                variant: x_variant(fmt, b),
                bitblt_layout: if has(fmt, "plain-index") {
                    xerox::XeroxBitbltLayout::PlainIndex
                } else if has(fmt, "kerned-index") {
                    xerox::XeroxBitbltLayout::KernedIndex
                } else if has(fmt, "plain") {
                    xerox::XeroxBitbltLayout::Plain
                } else {
                    xerox::XeroxBitbltLayout::Kerned
                },
                ..Default::default()
            },
        )?,
        _ => return Err(fontpixel::Error::InvalidInput("unknown C2 format".into())),
    }
    .output)
}
fn dump(s: &FontSet) -> serde_json::Value {
    serde_json::json!({"fonts":s.fonts.iter().map(|f|serde_json::json!({"family":f.names.family,"ascent":f.metrics.ascent,"descent":f.metrics.descent,"line_height":f.metrics.line_height,"point":f.metrics.point_size.map(|p|[p.numerator(),i64::from(p.denominator())]),"dpi":f.metrics.resolution.map(|a|[a.x,a.y]),"glyphs":f.glyphs().map(|(_,g)|serde_json::json!({"name":g.name,"codes":g.codepoints.iter().map(|cp|match cp{Codepoint::Legacy(v)=>*v,Codepoint::Unicode(v)=>*v}).collect::<Vec<_>>(),"w":g.bitmap.width(),"h":g.bitmap.height(),"x":g.metrics.bounds.x,"y":g.metrics.bounds.y,"advance":g.metrics.advance.map(|a|[a.x,a.y]),"scalable":g.metrics.scalable_advance.map(|a|[a.x,a.y]),"depth":if matches!(g.bitmap,fontpixel::model::GlyphBitmap::Mono(_)){1}else{8},"bytes":g.bitmap.as_bytes()})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    if a.len() < 3 {
        return Err("usage: dump|write|bench FORMAT INPUT [OUTPUT]".into());
    }
    let path = Path::new(&a[2]);
    let name = path
        .file_name()
        .ok_or("filename")?
        .to_str()
        .ok_or("UTF8 filename")?;
    let b = std::fs::read(path)?;
    let mut files = Files(BTreeMap::new());
    let side = path.with_file_name("IntMetrics");
    if side.exists() {
        files.0.insert("IntMetrics".into(), std::fs::read(side)?);
    }
    let s = read(&a[1], &b, name, &files)?;
    match a[0].as_str() {
        "dump" => println!("{}", dump(&s)),
        "write" => {
            let p = Path::new(a.get(3).ok_or("output")?);
            let n = p
                .file_name()
                .ok_or("output name")?
                .to_str()
                .ok_or("UTF8 output")?;
            for asset in write(&a[1], &b, &s, n, &files)?.assets() {
                std::fs::write(p.with_file_name(asset.path.as_str()), &asset.bytes)?;
            }
        }
        "bench" => {
            let output = write(&a[1], &b, &s, name, &files)?;
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
                write(&a[1], primary, &back, name, &backfiles)?,
                output,
                "canonical bundle"
            );
            for _ in 0..10 {
                black_box(read(&a[1], &b, name, &files)?);
                black_box(write(&a[1], &b, &s, name, &files)?);
            }
            let iterations = 300;
            let start = std::time::Instant::now();
            for _ in 0..iterations {
                black_box(read(&a[1], &b, name, &files)?);
            }
            let read_us = start.elapsed().as_secs_f64() * 1e6 / f64::from(iterations);
            let start = std::time::Instant::now();
            for _ in 0..iterations {
                black_box(write(&a[1], &b, &s, name, &files)?);
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
