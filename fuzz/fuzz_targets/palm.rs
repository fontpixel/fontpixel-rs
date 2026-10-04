#![no_main]
use fontpixel::formats::palm;
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use libfuzzer_sys::fuzz_target;
const VERSIONS: &[palm::PalmVersion] = &[
    palm::PalmVersion::V1,
    palm::PalmVersion::V2,
    palm::PalmVersion::Afnx,
    palm::PalmVersion::FontSubst,
    palm::PalmVersion::Gray1,
    palm::PalmVersion::Gray2,
];
fuzz_target!(|data: &[u8]| {
    let limits = fontpixel::ResourceLimits {
        max_input_bytes: 262144,
        max_total_bytes: 4 * 1024 * 1024,
        max_glyphs: 1024,
        max_pixels_per_glyph: 65536,
        max_table_bytes: 262144,
        max_resources: 32,
        ..Default::default()
    };
    let input = Input::new(data);
    if let Ok(mut c) = ReadContext::new(&input, &limits) {
        if let Ok((set, _)) = palm::read(&mut c, &Default::default()) {
            for version in VERSIONS {
                if let Ok(written) = palm::write(
                    &set,
                    &fontpixel::WriteOptions::new(ResourcePath::new("font").unwrap()),
                    &palm::PalmWriteOptions {
                        version: *version,
                        limits: limits.clone(),
                        max_output_bytes: 262144,
                        ..Default::default()
                    },
                ) {
                    let bytes = written.output.into_single().unwrap();
                    let input = Input::new(&bytes);
                    let mut c = ReadContext::new(&input, &limits).expect("bounded output");
                    let (back, _) = palm::read(&mut c, &Default::default())
                        .expect("successful writer output reads with same limits");
                    if !written.report.has_loss() {
                        assert_eq!(set, back);
                    }
                }
            }
        }
    }
});
