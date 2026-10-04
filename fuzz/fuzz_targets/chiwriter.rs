#![no_main]
use fontpixel::formats::chiwriter;
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let _ = chiwriter::detect(data);
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
        chiwriter::ChiwriterReadOptions {
            layout: chiwriter::ChiwriterLayout::Contiguous,
            stride: None,
            ..Default::default()
        },
        chiwriter::ChiwriterReadOptions {
            layout: chiwriter::ChiwriterLayout::Contiguous,
            stride: Some(3),
            ..Default::default()
        },
        chiwriter::ChiwriterReadOptions {
            layout: chiwriter::ChiwriterLayout::AbsoluteOffsets,
            stride: None,
            ..Default::default()
        },
        chiwriter::ChiwriterReadOptions {
            layout: chiwriter::ChiwriterLayout::AbsoluteOffsets,
            stride: Some(3),
            ..Default::default()
        },
        chiwriter::ChiwriterReadOptions {
            layout: chiwriter::ChiwriterLayout::RelativeOffsets,
            stride: None,
            ..Default::default()
        },
        chiwriter::ChiwriterReadOptions {
            layout: chiwriter::ChiwriterLayout::RelativeOffsets,
            stride: Some(3),
            ..Default::default()
        },
    ] {
        let input = Input::new(data);
        if let Ok(mut context) = ReadContext::new(&input, &limits) {
            if let Ok((set, _)) = chiwriter::read(&mut context, &ro) {
                let wo = chiwriter::ChiwriterWriteOptions {
                    version: if data.first() == Some(&16) {
                        chiwriter::ChiwriterVersion::V3
                    } else {
                        chiwriter::ChiwriterVersion::V4
                    },
                    layout: ro.layout,
                    stride: ro.stride,
                    limits: limits.clone(),
                    max_output_bytes: 262144,
                    ..Default::default()
                };
                if let Ok(written) = chiwriter::write(
                    &set,
                    &fontpixel::WriteOptions::new(ResourcePath::new("font").unwrap()),
                    &wo,
                ) {
                    let bytes = written.output.into_single().unwrap();
                    let input = Input::new(&bytes);
                    let mut context =
                        ReadContext::new(&input, &limits).expect("writer input budget");
                    let (back, _) = chiwriter::read(&mut context, &ro)
                        .expect("writer output readable with identical limits");
                    if !written.report.has_loss() {
                        assert_eq!(set, back);
                    }
                }
            }
        }
    }
});
