#![no_main]
use fontpixel::ResourceLimits;
use fontpixel::formats::hanfont::{self, HanfontReadOptions, HanfontWriteOptions, ReadMode};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = ResourceLimits {
        max_input_bytes: 1 << 16,
        max_resource_bytes: 1 << 16,
        max_total_bytes: 48 << 20,
        max_resources: 8,
        max_decompressed_bytes: 1 << 16,
        max_glyphs: 12000,
        max_pixels_per_glyph: 256,
        max_table_bytes: 1 << 16,
    };
    for mode in [ReadMode::Components, ReadMode::Syllables] {
        let read = HanfontReadOptions {
            mode,
            ..Default::default()
        };
        if let Ok((set, _)) = hanfont::read(data, &read, &limits) {
            let o = HanfontWriteOptions {
                limits: limits.clone(),
                max_output_bytes: 1 << 16,
                ..Default::default()
            };
            if let Ok((out, report)) = hanfont::write(&set, &o) {
                hanfont::read(&out, &Default::default(), &limits)
                    .expect("default readback under identical limits");
                let (again, _) = hanfont::read(&out, &read, &limits).unwrap();
                if !report.has_loss() {
                    assert_eq!(set, again);
                }
            }
        }
    }
});
