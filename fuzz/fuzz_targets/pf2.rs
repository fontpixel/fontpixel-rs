#![no_main]
use fontpixel::{
    ResourceLimits,
    formats::pf2::{self, Pf2WriteOptions},
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
    let _ = pf2::detect(data);
    if let Ok((font, _)) = pf2::read(data, &Default::default(), &limits) {
        if let Ok((out, report)) = pf2::write(
            &font,
            &Pf2WriteOptions {
                limits: limits.clone(),
                max_output_bytes: 65536,
                ..Default::default()
            },
        ) {
            let back = pf2::read(&out, &Default::default(), &limits)
                .expect("PF2 writer output must decode under the same limits")
                .0;
            if !report.has_loss() {
                assert_eq!(font, back);
            }
        }
    }
});
