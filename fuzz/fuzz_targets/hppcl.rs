#![no_main]
use fontpixel::formats::hppcl;
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let _ = hppcl::detect(data);
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
    for ro in [hppcl::HppclReadOptions {
        allow_legacy_descriptor_size: true,
    }] {
        let input = Input::new(data);
        if let Ok(mut context) = ReadContext::new(&input, &limits) {
            if let Ok((set, _)) = hppcl::read(&mut context, &ro) {
                let wo = hppcl::HppclWriteOptions {
                    orientation: match data.len() % 4 {
                        1 => hppcl::HppclOrientation::Landscape,
                        2 => hppcl::HppclOrientation::ReversePortrait,
                        3 => hppcl::HppclOrientation::ReverseLandscape,
                        _ => hppcl::HppclOrientation::Portrait,
                    },
                    compression: if data.len() % 2 == 0 {
                        hppcl::HppclCompression::Rle
                    } else {
                        hppcl::HppclCompression::Raw
                    },
                    continuation_bytes: 17,
                    limits: limits.clone(),
                    max_output_bytes: 262144,
                    ..Default::default()
                };
                if let Ok(written) = hppcl::write(
                    &set,
                    &fontpixel::WriteOptions::new(ResourcePath::new("font").unwrap()),
                    &wo,
                ) {
                    let bytes = written.output.into_single().unwrap();
                    let input = Input::new(&bytes);
                    let mut context =
                        ReadContext::new(&input, &limits).expect("writer input budget");
                    let (back, _) = hppcl::read(&mut context, &ro)
                        .expect("writer output readable with identical limits");
                    if !written.report.has_loss() {
                        assert_eq!(set, back);
                    }
                }
            }
        }
    }
});
