#![no_main]
use fontpixel::formats::nfnt::{self, Container, NfntCodec, NfntReadOptions, NfntWriteOptions};
use fontpixel::formats::{FormatReader, ReadOptions};
use fontpixel::resources::{Input, OutputBundle, ReadContext, ResourcePath, ResourceResolver};
use fontpixel::{Error, ResourceLimits};
use libfuzzer_sys::fuzz_target;
struct Assets<'a>(&'a OutputBundle);
impl ResourceResolver for Assets<'_> {
    fn resolve(&self, path: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        let asset = self
            .0
            .assets()
            .iter()
            .find(|a| a.path == *path)
            .ok_or_else(|| Error::MissingResource(path.to_string()))?;
        if asset.bytes.len() > max {
            return Err(Error::ResourceLimit {
                resource: "fuzz companion",
                limit: max,
            });
        }
        Ok(asset.bytes.clone())
    }
}
fuzz_target!(|data: &[u8]| {
    std::hint::black_box(nfnt::detect(data));
    let limits = ResourceLimits {
        max_input_bytes: 1 << 20,
        max_resource_bytes: 1 << 20,
        max_total_bytes: 8 << 20,
        max_resources: 128,
        max_decompressed_bytes: 1 << 20,
        max_glyphs: 4096,
        max_pixels_per_glyph: 1 << 16,
        max_table_bytes: 1 << 20,
    };
    if let Ok((set, _)) = nfnt::read(data, &Default::default(), &limits) {
        for container in [
            Container::Bare,
            Container::Dfont,
            Container::MacBinary,
            Container::AppleSingle,
        ] {
            let options = NfntWriteOptions {
                container,
                limits: limits.clone(),
                max_output_bytes: 1 << 20,
                ..Default::default()
            };
            if let Ok((bytes, report)) = nfnt::write(&set, &options) {
                let (roundtrip, _) = nfnt::read(&bytes, &Default::default(), &limits)
                    .expect("successful NFNT output must reread under the same limits");
                if !report.has_loss() {
                    assert_eq!(set, roundtrip);
                }
            }
        }
        let options = NfntWriteOptions {
            container: Container::AppleDouble,
            limits: limits.clone(),
            max_output_bytes: 1 << 20,
            ..Default::default()
        };
        let path = ResourcePath::new("font").unwrap();
        if let Ok(written) = nfnt::write_bundle(&set, &options, &path) {
            let primary = written
                .output
                .assets()
                .iter()
                .find(|a| a.path == path)
                .unwrap();
            let assets = Assets(&written.output);
            let input = Input::new(&primary.bytes).with_resources(&assets);
            let mut context = ReadContext::new(&input, &limits).unwrap();
            let codec = NfntCodec {
                read_options: NfntReadOptions {
                    apple_double_resource: Some(ResourcePath::new("._font").unwrap()),
                    ..Default::default()
                },
                ..Default::default()
            };
            let (roundtrip, _) = codec
                .read(&mut context, &ReadOptions::default())
                .expect("successful AppleDouble bundle must reread under the same limits");
            if !written.report.has_loss() {
                assert_eq!(set, roundtrip);
            }
        }
    }
});
