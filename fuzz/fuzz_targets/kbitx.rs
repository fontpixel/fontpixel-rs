#![no_main]
use fontpixel::{
    ResourceLimits,
    formats::kbitx::{self, KbitxReadOptions, KbitxWriteOptions},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = ResourceLimits {
        max_input_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 32768,
        max_glyphs: 256,
        max_pixels_per_glyph: 65536,
        ..Default::default()
    };
    for gray_threshold in [None, Some(128), Some(0)] {
        if let Ok((font, _)) = kbitx::read(data, &KbitxReadOptions { gray_threshold }, &limits) {
            let options = KbitxWriteOptions {
                limits: limits.clone(),
                max_output_bytes: 65536,
            };
            if let Ok((out, report)) = kbitx::write(&font, &options) {
                let (back, _) = kbitx::read(&out, &KbitxReadOptions::default(), &limits)
                    .expect("KBITX writer output must decode under the same limits");
                if !report.has_loss() {
                    assert_eq!(font, back);
                }
                let (again, _) = kbitx::write(&back, &options)
                    .expect("Native KBITX output must remain writable");
                assert_eq!(out, again);
            }
        }
    }
});
