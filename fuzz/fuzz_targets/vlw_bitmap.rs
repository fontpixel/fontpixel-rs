#![no_main]
use fontpixel::formats::vlw as codec;
use libfuzzer_sys::fuzz_target;
#[path = "pilfont_bitmap/common.rs"]
mod common;
fuzz_target!(|data: &[u8]| {
    let o = codec::VlwWriteOptions {
        limits: common::limits(),
        max_output_bytes: 256 * 1024,
        version: match data.last().copied().unwrap_or(0) % 4 {
            0 => codec::VlwVersion::V8,
            1 => codec::VlwVersion::V9,
            2 => codec::VlwVersion::V10,
            _ => codec::VlwVersion::V11,
        },
        ..Default::default()
    };
    common::exercise(
        data,
        "font.vlw",
        false,
        |c| codec::read(c, &Default::default()),
        |s, v| codec::write(s, v, &o),
    );
});
