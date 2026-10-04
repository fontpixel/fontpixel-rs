#![no_main]
use fontpixel::{
    Error, ResourceLimits, ResourcePath, Result,
    formats::plan9::{self, Compression, Container, Plan9WriteOptions},
    resources::{Input, OutputBundle, ReadContext, ResourceResolver},
};
use libfuzzer_sys::fuzz_target;
struct Bytes<'a>(&'a [u8]);
impl ResourceResolver for Bytes<'_> {
    fn resolve(&self, _: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        if self.0.len() > max {
            return Err(Error::ResourceLimit {
                resource: "fuzz resource",
                limit: max,
            });
        }
        Ok(self.0.to_vec())
    }
}
struct Assets<'a>(&'a OutputBundle);
impl ResourceResolver for Assets<'_> {
    fn resolve(&self, p: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let a = self
            .0
            .assets()
            .iter()
            .find(|a| a.path == *p)
            .ok_or_else(|| Error::MissingResource(p.to_string()))?;
        Bytes(&a.bytes).resolve(p, max)
    }
}
fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let length = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
    let split = 4 + length.min(data.len() - 4);
    let (primary, sidecar) = (&data[4..split], &data[split..]);
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
    let input = Input::new(primary).with_resources(&resolver);
    let Ok(mut c) = ReadContext::new(&input, &limits) else {
        return;
    };
    if let Ok((font, _)) = plan9::read(&mut c) {
        for (container, compression) in [
            (Container::Font, Compression::Raw),
            (Container::Subfont, Compression::Raw),
            (Container::Subfont, Compression::Literal),
        ] {
            if let Ok(out) = plan9::write(
                &font.clone().into(),
                &ResourcePath::new("font").unwrap(),
                &Plan9WriteOptions {
                    container,
                    compression,
                    limits: limits.clone(),
                    max_output_bytes: 65536,
                    ..Default::default()
                },
            ) {
                let primary = out
                    .output
                    .assets()
                    .iter()
                    .find(|a| a.path == *out.output.primary())
                    .unwrap();
                let resolver = Assets(&out.output);
                let input = Input::new(&primary.bytes).with_resources(&resolver);
                let mut context = ReadContext::new(&input, &limits).unwrap();
                let (back, _) = plan9::read(&mut context)
                    .expect("successful Plan 9 output must reread under identical limits");
                if !out.report.has_loss() {
                    assert_eq!(font, back)
                }
            }
        }
    }
});
