#![no_main]
use fontpixel::formats::pxl;
use fontpixel::resources::{Input, OutputAsset, ReadContext, ResourcePath, ResourceResolver};
use libfuzzer_sys::fuzz_target;
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
                resource: "fuzz output sidecar",
                limit: max,
            });
        }
        Ok(b.clone())
    }
}
fuzz_target!(|data: &[u8]| {
    let limits = fontpixel::ResourceLimits {
        max_input_bytes: 262144,
        max_resource_bytes: 262144,
        max_total_bytes: 4 * 1024 * 1024,
        max_glyphs: 1024,
        max_pixels_per_glyph: 65536,
        max_table_bytes: 262144,
        max_resources: 8,
        ..Default::default()
    };
    let (primary, side) = if data.starts_with(b"TFM!") && data.len() >= 8 {
        let n = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
        if n > data.len() - 8 {
            return;
        }
        (&data[8..8 + n], &data[8 + n..])
    } else {
        (data, &[][..])
    };
    let sidecar = Side(side);
    let input = Input::new(primary)
        .with_name("font.nlq")
        .with_resources(&sidecar);
    if let Ok(mut context) = ReadContext::new(&input, &limits) {
        if let Ok((set, _)) = pxl::read(
            &mut context,
            &pxl::PxlReadOptions {
                tfm: (!side.is_empty()).then(|| ResourcePath::new("font.tfm").unwrap()),
                ..Default::default()
            },
        ) {
            let opts = pxl::PxlWriteOptions {
                version: if primary.get(3) == Some(&0xea) {
                    pxl::PxlVersion::V1002
                } else {
                    pxl::PxlVersion::V1001
                },
                tfm: Some(ResourcePath::new("font.tfm").unwrap()),
                limits: limits.clone(),
                max_output_bytes: 262144,
                ..Default::default()
            };
            if let Ok(written) = pxl::write(
                &set,
                &fontpixel::WriteOptions::new(ResourcePath::new("font.nlq").unwrap()),
                &opts,
            ) {
                let assets = Assets(written.output.into_assets());
                let b = &assets
                    .0
                    .iter()
                    .find(|a| a.path.as_str() == "font.nlq")
                    .unwrap()
                    .bytes;
                let input = Input::new(b).with_name("font.nlq").with_resources(&assets);
                let mut context = ReadContext::new(&input, &limits).expect("writer input limits");
                let (back, _) = pxl::read(
                    &mut context,
                    &pxl::PxlReadOptions {
                        tfm: opts.tfm.clone(),
                        ..Default::default()
                    },
                )
                .expect("writer output readable with same limits");
                if !written.report.has_loss() {
                    assert_eq!(set, back);
                }
            }
        }
    }
});
