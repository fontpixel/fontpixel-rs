#![no_main]
use fontpixel::formats::sbf as codec;
use libfuzzer_sys::fuzz_target;
#[path = "rockbox_bitmap/common.rs"]
mod common;
fuzz_target!(|data: &[u8]| {
    let options = codec::SbfWriteOptions {
        limits: common::limits(),
        max_output_bytes: 256 * 1024,
        gray_threshold: Some(128),
        ..Default::default()
    };
    common::exercise(
        data,
        |c| codec::read(c, &Default::default()),
        |f, o| codec::write(f, o, &options),
    );
});
