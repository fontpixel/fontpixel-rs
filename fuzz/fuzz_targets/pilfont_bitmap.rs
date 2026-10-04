#![no_main]
use fontpixel::formats::pilfont as codec;
use libfuzzer_sys::fuzz_target;
#[path = "pilfont_bitmap/common.rs"]
mod common;
fuzz_target!(|data: &[u8]| {
    let o = codec::PilfontWriteOptions {
        limits: common::limits(),
        max_output_bytes: 256 * 1024,
        ..Default::default()
    };
    common::exercise(
        data,
        "font.pil",
        false,
        |c| codec::read(c, &Default::default()),
        |s, v| codec::write(s, v, &o),
    );
});
