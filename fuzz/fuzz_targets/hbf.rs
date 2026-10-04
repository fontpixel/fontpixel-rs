#![no_main]
use fontpixel::formats::hbf::{self, HbfWriteOptions};
use fontpixel::resources::{Input, ReadContext, ResourceResolver};
use fontpixel::{ResourceLimits, ResourcePath};
use libfuzzer_sys::fuzz_target;
struct Bytes<'a>(&'a [u8]);
impl ResourceResolver for Bytes<'_> {
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
struct Assets<'a>(&'a fontpixel::resources::OutputBundle);
impl ResourceResolver for Assets<'_> {
    fn resolve(&self, path: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let bytes = self
            .0
            .assets()
            .iter()
            .find(|a| &a.path == path)
            .ok_or_else(|| fontpixel::Error::MissingResource(path.to_string()))?;
        Bytes(&bytes.bytes).resolve(path, max)
    }
}
fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let length = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
    let split = 4 + length.min(data.len() - 4);
    let (bytes, sidecar) = (&data[4..split], &data[split..]);
    let limits = ResourceLimits {
        max_input_bytes: 65536,
        max_resource_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_resources: 8,
        max_decompressed_bytes: 65536,
        max_glyphs: 512,
        max_pixels_per_glyph: 65536,
        max_table_bytes: 65536,
    };
    let resolver = Bytes(sidecar);
    let input = Input::new(bytes).with_resources(&resolver);
    let Ok(mut context) = ReadContext::new(&input, &limits) else {
        return;
    };
    if let Ok((font, _)) = hbf::read(&mut context) {
        if let Ok(out) = hbf::write(
            &font,
            &ResourcePath::new("font.hbf").unwrap(),
            &HbfWriteOptions {
                limits: limits.clone(),
                ..Default::default()
            },
        ) {
            let primary = out
                .output
                .assets()
                .iter()
                .find(|a| &a.path == out.output.primary())
                .unwrap();
            let assets = Assets(&out.output);
            let input = Input::new(&primary.bytes).with_resources(&assets);
            let mut context = ReadContext::new(&input, &limits).unwrap();
            let (back, _) = hbf::read(&mut context)
                .expect("successful HBF output must read within the same limits");
            assert_eq!(font.len(), back.len());
            for ((_, a), (_, b)) in font.glyphs().zip(back.glyphs()) {
                assert_eq!(a, b);
            }
        }
    }
});
