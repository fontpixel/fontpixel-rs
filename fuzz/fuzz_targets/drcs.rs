//! DEC DRCS malformed controls and successful-write bounded readback.
#![no_main]
use fontpixel::ResourceLimits;
use fontpixel::formats::drcs::*;
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let l = ResourceLimits {
        max_input_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 65536,
        max_glyphs: 96,
        max_pixels_per_glyph: 4096,
        ..Default::default()
    };
    for terminal in [Terminal::Vt320, Terminal::Vt510] {
        if let Ok((set, _)) = read(
            data,
            &DrcsReadOptions {
                terminal,
                ..Default::default()
            },
            &l,
        ) {
            for charset in [DrcsCharset::NinetyFour, DrcsCharset::NinetySix] {
                if let Ok((bytes, report)) = write(
                    &set,
                    &DrcsWriteOptions {
                        terminal,
                        charset,
                        gray_threshold: Some(128),
                        limits: l.clone(),
                        ..Default::default()
                    },
                ) {
                    let (back, _) = read(
                        &bytes,
                        &DrcsReadOptions {
                            terminal,
                            ..Default::default()
                        },
                        &l,
                    )
                    .expect("same-limit DRCS writer output readable");
                    if !report.has_loss() {
                        assert_eq!(set, back);
                    }
                }
            }
        }
    }
});
