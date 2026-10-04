#![no_main]
use fontpixel::{
    ResourceLimits,
    formats::pk::{self, PkWriteOptions},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = ResourceLimits {
        max_input_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 65536,
        max_glyphs: 512,
        max_pixels_per_glyph: 65536,
        ..Default::default()
    };
    if let Ok((font, _)) = pk::read(data, &limits) {
        if let Ok((bytes, report)) = pk::write(
            &font,
            &PkWriteOptions {
                limits: limits.clone(),
                max_output_bytes: 65536,
                ..Default::default()
            },
        ) {
            let back = pk::read(&bytes, &limits)
                .expect("successful PK output must reread under identical limits")
                .0;
            if !report.has_loss() {
                assert_eq!(font, back)
            }
        }
    }
});
