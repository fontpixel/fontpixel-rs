//! Codec timings with deterministic glyphs and a bounded in-memory sidecar resolver.
use fontpixel::formats::c::{ArrayOptions, CCodec, CReadOptions, CWriteOptions, Family};
use fontpixel::formats::image::{ImageCodec, ImageWriteOptions};
use fontpixel::formats::{FormatReader, FormatWriter};
use fontpixel::resources::{OutputBundle, ReadContext, ResourceResolver};
use fontpixel::{Input, ReadOptions, ResourcePath, WriteOptions};
use std::{hint::black_box, time::Instant};
struct Assets(OutputBundle);
impl ResourceResolver for Assets {
    fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let a = self
            .0
            .assets()
            .iter()
            .find(|a| &a.path == p)
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?;
        if a.bytes.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "timing resource",
                limit: max,
            });
        }
        Ok(a.bytes.clone())
    }
}
fn timed(mut f: impl FnMut(), n: usize) -> f64 {
    for _ in 0..5 {
        f();
    }
    let start = Instant::now();
    for _ in 0..n {
        f();
    }
    start.elapsed().as_secs_f64() * 1e6 / n as f64
}
fn main() -> fontpixel::Result<()> {
    let n = 200;
    let mut source = String::from("const uint8_t Test[256][32] = {\n");
    for cp in 0u32..256 {
        source.push('{');
        for row in 0..16 {
            if row > 0 {
                source.push(',');
            }
            source.push_str(&format!(
                "{},{}",
                (cp.wrapping_mul(17) + row * 13) & 255,
                if (cp + row) % 3 == 0 { 128 } else { 0 }
            ));
        }
        source.push_str("},\n");
    }
    source.push_str("};\n");
    let opts = ReadOptions::default();
    let c = CCodec {
        read_options: CReadOptions {
            generic: Some(ArrayOptions {
                width: 9,
                height: 16,
                first_code: 32,
                charset: "unicode".into(),
                baseline: 13,
                lsb_first: false,
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let input = Input::new(source.as_bytes());
    let (font, _) = c.read(&mut ReadContext::new(&input, &opts.limits)?, &opts)?;
    let mut records = serde_json::Map::new();
    for ext in ["png", "pgm"] {
        let codec = ImageCodec {
            write_options: ImageWriteOptions::default(),
            ..Default::default()
        };
        let options = WriteOptions::new(ResourcePath::new(format!("font.{ext}"))?);
        let assets = Assets(codec.write(&font, &options)?.output);
        let data = assets
            .0
            .assets()
            .iter()
            .find(|a| a.path == options.primary_name)
            .unwrap();
        let input = Input::new(&data.bytes)
            .with_name(options.primary_name.as_str())
            .with_resources(&assets);
        let read = timed(
            || {
                black_box(
                    codec
                        .read(&mut ReadContext::new(&input, &opts.limits).unwrap(), &opts)
                        .unwrap(),
                );
            },
            n,
        );
        let write = timed(
            || {
                black_box(codec.write(&font, &options).unwrap());
            },
            n,
        );
        records.insert(ext.into(),serde_json::json!({"read_us":read,"write_us":write,"bytes":assets.0.assets().iter().map(|a|a.bytes.len()).sum::<usize>()}));
    }
    for (name, family) in [("generic", Family::Generic), ("gfx", Family::Gfx)] {
        let codec = CCodec {
            write_options: CWriteOptions {
                family,
                ..Default::default()
            },
            ..Default::default()
        };
        let options = WriteOptions::new(ResourcePath::new("font.h")?);
        let bytes = codec.write(&font, &options)?.output.into_single()?;
        let input = Input::new(&bytes);
        let read = timed(
            || {
                black_box(
                    codec
                        .read(&mut ReadContext::new(&input, &opts.limits).unwrap(), &opts)
                        .unwrap(),
                );
            },
            n,
        );
        let write = timed(
            || {
                black_box(codec.write(&font, &options).unwrap());
            },
            n,
        );
        records.insert(
            name.into(),
            serde_json::json!({"read_us":read,"write_us":write,"bytes":bytes.len()}),
        );
        for jobs in [1, 2, 4] {
            let start = Instant::now();
            std::thread::scope(|s| {
                for _ in 0..jobs {
                    let bytes = &bytes;
                    let opts = &opts;
                    s.spawn(move || {
                        let c = CCodec::default();
                        for _ in 0..1000 / jobs {
                            let input = Input::new(bytes);
                            black_box(
                                c.read(&mut ReadContext::new(&input, &opts.limits).unwrap(), opts)
                                    .unwrap(),
                            );
                        }
                    });
                }
            });
            records.insert(
                format!("{name}_1000_{jobs}_threads_ms"),
                serde_json::json!(start.elapsed().as_secs_f64() * 1000.),
            );
        }
    }
    println!(
        "{}",
        serde_json::json!({"glyphs":256,"iterations":n,"timings":records})
    );
    Ok(())
}
