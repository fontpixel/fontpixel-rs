#![no_main]
use fontpixel::formats::amiga;
use fontpixel::resources::{Input, ReadContext, ResourcePath, ResourceResolver};
struct Sidecar<'a>(&'a [u8]);
impl ResourceResolver for Sidecar<'_> {
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
struct Bundle<'a>(&'a fontpixel::resources::OutputBundle);
impl ResourceResolver for Bundle<'_> {
    fn resolve(&self, path: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let asset = self
            .0
            .assets()
            .iter()
            .find(|a| a.path == *path)
            .ok_or_else(|| fontpixel::Error::MissingResource(path.to_string()))?;
        if asset.bytes.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "fuzz output sidecar",
                limit: max,
            });
        }
        Ok(asset.bytes.clone())
    }
}
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = fontpixel::ResourceLimits {
        max_input_bytes: 262144,
        max_resource_bytes: 262144,
        max_total_bytes: 2 * 1024 * 1024,
        max_glyphs: 4096,
        max_pixels_per_glyph: 65536,
        max_table_bytes: 262144,
        max_resources: 32,
        ..Default::default()
    };
    let sidecar = Sidecar(data.get(264..).unwrap_or_default());
    let input = Input::new(data).with_resources(&sidecar);
    if let Ok(mut c) = ReadContext::new(&input, &limits) {
        if let Ok((s, _)) = amiga::read(&mut c, &Default::default()) {
            if let Ok(written) = amiga::write(
                &s,
                &fontpixel::WriteOptions::new(ResourcePath::new("font.font").unwrap()),
                &amiga::AmigaWriteOptions {
                    limits: limits.clone(),
                    gray_threshold: data.first().filter(|&&b| b & 1 != 0).map(|_| 128),
                    max_output_bytes: 262144,
                    ..Default::default()
                },
            ) {
                let resolver = Bundle(&written.output);
                let primary = written
                    .output
                    .assets()
                    .iter()
                    .find(|a| &a.path == written.output.primary())
                    .unwrap();
                let input = Input::new(&primary.bytes).with_resources(&resolver);
                let mut context =
                    ReadContext::new(&input, &limits).expect("Amiga output meets input limits");
                let (back, _) = amiga::read(&mut context, &Default::default())
                    .expect("Amiga output readable under identical limits");
                if !written.report.has_loss() {
                    assert_eq!(s, back);
                }
            }
        }
    }
});
