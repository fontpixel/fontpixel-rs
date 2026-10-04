#![no_main]
use fontpixel::formats::riscos;
use fontpixel::resources::{Input, OutputAsset, ReadContext, ResourcePath, ResourceResolver};
struct Side<'a>(&'a [u8]);
impl ResourceResolver for Side<'_> {
    fn resolve(&self, _: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        if self.0.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "fuzz sidecar",
                limit: max,
            });
        }
        Ok(self.0.to_vec())
    }
}
struct Assets(Vec<OutputAsset>);
impl ResourceResolver for Assets {
    fn resolve(&self, p: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let b = &self
            .0
            .iter()
            .find(|a| &a.path == p)
            .ok_or_else(|| fontpixel::Error::MissingResource(p.to_string()))?
            .bytes;
        if b.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "fuzz output resource",
                limit: max,
            });
        }
        Ok(b.clone())
    }
}
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let _ = riscos::detect(data);
    let limits = fontpixel::ResourceLimits {
        max_input_bytes: 262144,
        max_resource_bytes: 262144,
        max_total_bytes: 8 * 1024 * 1024,
        max_glyphs: 2048,
        max_pixels_per_glyph: 65536,
        max_table_bytes: 262144,
        max_resources: 8,
        ..Default::default()
    };
    let (data, side, framed) = if data.starts_with(b"METR") && data.len() >= 8 {
        let n = u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize;
        if n > data.len() - 8 {
            return;
        }
        (&data[8..8 + n], &data[8 + n..], true)
    } else {
        (data, data, false)
    };
    let sidecar = Side(side);
    for ro in [riscos::RiscosReadOptions {
        metrics_path: framed.then(|| ResourcePath::new("IntMetrics").unwrap()),
        ..Default::default()
    }] {
        let input = Input::new(data).with_resources(&sidecar);
        if let Ok(mut context) = ReadContext::new(&input, &limits) {
            if let Ok((set, _)) = riscos::read(&mut context, &ro) {
                let wo = riscos::RiscosWriteOptions {
                    version: if !data.starts_with(b"FONT") {
                        riscos::RiscosVersion::Xy
                    } else {
                        match data[5] {
                            4 => riscos::RiscosVersion::V4,
                            5 => riscos::RiscosVersion::V5,
                            7 => riscos::RiscosVersion::V7,
                            8 => riscos::RiscosVersion::V8,
                            _ => riscos::RiscosVersion::V6,
                        }
                    },
                    depth: if !data.starts_with(b"FONT") || data[4] == 4 {
                        riscos::RiscosDepth::Gray4
                    } else {
                        riscos::RiscosDepth::Mono
                    },
                    compression: if data.starts_with(b"FONT") && data[4] == 1 && data.len() % 2 == 0
                    {
                        riscos::RiscosCompression::Packed
                    } else {
                        riscos::RiscosCompression::Raw
                    },
                    metrics_path: ro.metrics_path.clone(),
                    limits: limits.clone(),
                    max_output_bytes: 262144,
                    ..Default::default()
                };
                if let Ok(written) = riscos::write(
                    &set,
                    &fontpixel::WriteOptions::new(ResourcePath::new("font").unwrap()),
                    &wo,
                ) {
                    let assets = Assets(written.output.into_assets());
                    let bytes = &assets
                        .0
                        .iter()
                        .find(|a| a.path.as_str() == "font")
                        .unwrap()
                        .bytes;
                    let input = Input::new(bytes).with_resources(&assets);
                    let mut context =
                        ReadContext::new(&input, &limits).expect("writer input budget");
                    let (back, _) = riscos::read(&mut context, &ro)
                        .expect("writer output readable with identical limits");
                    if !written.report.has_loss() {
                        assert_eq!(set, back);
                    }
                }
            }
        }
    }
});
