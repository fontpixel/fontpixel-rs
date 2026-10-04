#![no_main]
use fontpixel::{
    ResourceLimits,
    formats::hzk::{self, CodeSpace, Geometry, HzkReadOptions, HzkWriteOptions, Storage},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let Some((&selector, bytes)) = data.split_first() else {
        return;
    };
    let limits = ResourceLimits {
        max_input_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 65536,
        max_glyphs: 512,
        max_pixels_per_glyph: 65536,
        ..Default::default()
    };
    let geometry = Geometry {
        width: u16::from(selector % 31) + 1,
        height: u16::from(selector / 8) + 1,
        first_code: if selector & 64 == 0 { 0xa1a1 } else { 0 },
        last_code: if selector & 64 == 0 { 0xfefe } else { 255 },
        code_space: if selector & 64 == 0 {
            CodeSpace::Gb2312
        } else {
            CodeSpace::Ascii
        },
        storage: if selector & 128 == 0 {
            Storage::Rows
        } else {
            Storage::Columns
        },
    };
    let read = HzkReadOptions {
        geometry: Some(geometry),
        ..Default::default()
    };
    if let Ok((font, _)) = hzk::read(bytes, &read, &limits) {
        if let Ok((out, report)) = hzk::write(
            &font,
            &HzkWriteOptions {
                geometry: Some(geometry),
                limits: limits.clone(),
                max_output_bytes: 65536,
                ..Default::default()
            },
        ) {
            let back = hzk::read(&out, &read, &limits)
                .expect("HZK writer output must decode under the same limits")
                .0;
            if !report.has_loss() {
                assert_eq!(font, back);
            }
        }
    }
});
