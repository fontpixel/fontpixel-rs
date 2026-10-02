#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::{ReadOptions, ResourceLimits};
use fontpixel::formats::bdf::{self, BdfWriteOptions};
fuzz_target!(|data: &[u8]| {
    let limits = ResourceLimits {
        max_input_bytes: 65536, max_total_bytes: 4 * 1024 * 1024,
        max_decompressed_bytes: 65536, max_table_bytes: 65536,
        max_glyphs: 256, max_pixels_per_glyph: 65536,
        ..ResourceLimits::default()
    };
    let options = ReadOptions { format: Some("bdf".into()), limits };
    if let Ok(loaded) = fontpixel::read(data, &options) {
        for font in &loaded.fonts.fonts {
            for (_, glyph) in font.glyphs() { let _ = bdf::validate_bitmap(glyph); }
            let _ = bdf::write(font, &BdfWriteOptions { max_output_bytes: 1024 * 1024, ..BdfWriteOptions::default() });
        }
    }
});
