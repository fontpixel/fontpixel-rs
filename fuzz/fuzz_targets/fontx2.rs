#![no_main]
use fontpixel::{
    ResourceLimits,
    formats::fontx2::{self, Fontx2WriteOptions},
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
    let _ = fontx2::detect(data);
    if let Ok((font, _)) = fontx2::read(data, &Default::default(), &limits) {
        if let Ok((out, report)) = fontx2::write(
            &font,
            &Fontx2WriteOptions {
                limits: limits.clone(),
                max_output_bytes: 65536,
                ..Default::default()
            },
        ) {
            let back = fontx2::read(&out, &Default::default(), &limits)
                .expect("FONTX2 writer output must decode under the same limits")
                .0;
            if !report.has_loss() {
                assert_eq!(font, back);
            }
        }
    }
});
