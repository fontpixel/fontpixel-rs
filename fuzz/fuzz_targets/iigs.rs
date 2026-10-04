#![no_main]
use fontpixel::ResourceLimits;
use fontpixel::formats::iigs::{self, Container, IigsWriteOptions, Version};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    std::hint::black_box(iigs::detect(data));
    let limits = ResourceLimits {
        max_input_bytes: 1 << 20,
        max_resource_bytes: 1 << 20,
        max_total_bytes: 24 << 20,
        max_resources: 128,
        max_decompressed_bytes: 1 << 20,
        max_glyphs: 512,
        max_pixels_per_glyph: 1 << 20,
        max_table_bytes: 1 << 20,
    };
    if let Ok((set, _)) = iigs::read(data, &Default::default(), &limits) {
        for (container, version) in [
            (Container::Bare, Version::Auto),
            (Container::Bare, Version::V1_1),
            (Container::Bare, Version::V1_5),
            (Container::BinScii, Version::Auto),
        ] {
            let options = IigsWriteOptions {
                container,
                version,
                limits: limits.clone(),
                max_output_bytes: 1 << 20,
                ..Default::default()
            };
            if let Ok((out, report)) = iigs::write(&set, &options) {
                let (mut again, _) = iigs::read(&out, &Default::default(), &limits)
                    .expect("successful IIgs output must read with identical limits");
                if !report.has_loss() {
                    // A newly requested wrapper adds provenance without losing
                    // source data. Existing wrapper provenance must still match.
                    if container == Container::BinScii
                        && set.properties.get("iigs.binscii-source").is_none()
                    {
                        assert_eq!(again.properties.remove("iigs.binscii-source"), 1);
                    }
                    assert_eq!(set, again);
                }
            }
        }
    }
});
