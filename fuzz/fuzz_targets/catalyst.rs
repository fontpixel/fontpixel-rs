#![no_main]
use fontpixel::formats::catalyst;
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let _ = catalyst::detect(data);
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
    for ro in [
        catalyst::CatalystReadOptions {
            offset_base: catalyst::CatalystOffsetBase::ResourceBody,
        },
        catalyst::CatalystReadOptions {
            offset_base: catalyst::CatalystOffsetBase::GlyphData,
        },
    ] {
        let input = Input::new(data);
        if let Ok(mut context) = ReadContext::new(&input, &limits) {
            if let Ok((set, _)) = catalyst::read(&mut context, &ro) {
                let wo = catalyst::CatalystWriteOptions {
                    offset_base: ro.offset_base,
                    limits: limits.clone(),
                    max_output_bytes: 262144,
                    ..Default::default()
                };
                if let Ok(written) = catalyst::write(
                    &set,
                    &fontpixel::WriteOptions::new(ResourcePath::new("font").unwrap()),
                    &wo,
                ) {
                    let bytes = written.output.into_single().unwrap();
                    let input = Input::new(&bytes);
                    let mut context =
                        ReadContext::new(&input, &limits).expect("writer input budget");
                    let (back, _) = catalyst::read(&mut context, &ro)
                        .expect("writer output readable with identical limits");
                    if !written.report.has_loss() {
                        assert_eq!(set, back);
                    }
                }
            }
        }
    }
});
