#![no_main]
use fontpixel::{
    Font, FontSet, Glyph,
    bitmap::{Bitmap, BitmapLimits, Gray8},
    formats::embedded::{self, ColorFlavor, ColorWriteOptions},
    model::{Codepoint, Vector},
    sfnt::bitmap::{self, Kind, Limits},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let mut limits = Limits::default();
    limits.max_strikes = 8;
    limits.max_depth = 8;
    limits.max_pixel_work = 262144;
    limits.max_visits = 4096;
    limits.sfnt.resources.max_glyphs = 256;
    limits.sfnt.resources.max_pixels_per_glyph = 4096;
    limits.sfnt.resources.max_total_bytes = 524288;
    limits.sfnt.resources.max_table_bytes = 131072;
    limits.sfnt.resources.max_decompressed_bytes = 131072;
    limits.sfnt.resources.max_input_bytes = 524288;
    limits.sfnt.max_cmap_mappings = 1024;
    limits.sfnt.max_name_records = 256;
    let read = embedded::EmbeddedReadOptions {
        limits: limits.clone(),
        color_darkness: data.first().is_some_and(|b| b & 1 != 0),
        ..Default::default()
    };
    if let Ok((fonts, _)) = embedded::read(data, Kind::Color, &read) {
        for flavor in [ColorFlavor::Cbdt, ColorFlavor::Sbix] {
            if let Ok((bytes, _)) = embedded::write_color(
                &fonts,
                &ColorWriteOptions {
                    limits: limits.clone(),
                    flavor,
                },
            ) {
                embedded::read(&bytes, Kind::Color, &read)
                    .expect("successful color output must read with the same limits");
            }
        }
    }
    if data.len() >= 4 {
        let split = u16::from_be_bytes([data[0], data[1]]) as usize;
        let count = u16::from_be_bytes([data[2], data[3]]) % 256 + 1;
        if split <= data.len() - 4 {
            let _ = bitmap::decode_with_color(
                &data[4..4 + split],
                &data[4 + split..],
                Kind::Color,
                count,
                &limits,
                true,
            );
        }
        let _ = bitmap::color::decode_sbix(&data[4..], count, &limits, false);
        let mut font = Font::default();
        font.metrics.pixel_size = Some(u32::from(data[0]) + 1);
        for (i, bytes) in data[4..].chunks(16).take(8).enumerate() {
            let width = u32::from(bytes[0] & 7);
            let height = u32::from(bytes[0] >> 4);
            let pixels: Vec<_> = (0..width * height)
                .map(|n| bytes[n as usize % bytes.len()])
                .collect();
            let gray =
                Bitmap::<Gray8>::from_raw(width, height, &pixels, BitmapLimits::default()).unwrap();
            let mut g = Glyph::new(gray);
            g.codepoints = vec![Codepoint::Unicode(65 + i as u32)];
            g.metrics.advance = Some(Vector::new(i64::from(data[1]), 0));
            g.metrics.bounds.x = i64::from(data[2] as i8);
            g.metrics.bounds.y = i64::from(data[3] as i8);
            font.add_glyph(g).unwrap();
        }
        let fonts: FontSet = font.into();
        for flavor in [ColorFlavor::Cbdt, ColorFlavor::Sbix] {
            if let Ok((bytes, _)) = embedded::write_color(
                &fonts,
                &ColorWriteOptions {
                    limits: limits.clone(),
                    flavor,
                },
            ) {
                embedded::read(&bytes, Kind::Color, &read).expect("generated writer contract");
            }
        }
    }
});
