#![no_main]
use fontpixel::{
    ResourceLimits,
    formats::kbits::{self, KbitsWriteOptions},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = ResourceLimits {
        max_input_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 32768,
        max_glyphs: 128,
        max_pixels_per_glyph: 65536,
        ..Default::default()
    };
    if let Ok((font, _)) = kbits::read(data, &limits) {
        let options = KbitsWriteOptions {
            limits: limits.clone(),
        };
        if let Ok((bytes, report)) = kbits::write(&font, &options) {
            let (back, _) =
                kbits::read(&bytes, &limits).expect("Kbits output readable under identical limits");
            if !report.has_loss() {
                assert_eq!(font, back);
            }
            let (again, _) =
                kbits::write(&back, &options).expect("canonical Kbits remains writable");
            assert_eq!(bytes, again);
        }
    }
});
