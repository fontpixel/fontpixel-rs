#![no_main]
use fontpixel::{ResourceLimits, formats::draw};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    let l = ResourceLimits {
        max_input_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 65536,
        max_glyphs: 512,
        max_pixels_per_glyph: 65536,
        ..Default::default()
    };
    if let Ok((font, _)) = draw::read(bytes, &l) {
        if let Ok((b, r)) = draw::write(
            &font,
            &draw::DrawWriteOptions {
                limits: l.clone(),
                max_output_bytes: 65536,
                ..Default::default()
            },
        ) {
            let back = draw::read(&b, &l)
                .expect("successful draw output must decode")
                .0;
            if !r.has_loss() {
                assert_eq!(font, back);
            }
        }
    }
});
