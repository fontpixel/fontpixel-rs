#![no_main]
use fontpixel::formats::rockbox as codec;
use libfuzzer_sys::fuzz_target;
#[path = "rockbox_bitmap/common.rs"]
mod common;
fuzz_target!(|data: &[u8]| {
    let options = codec::RockboxWriteOptions {
        limits: common::limits(),
        max_output_bytes: 256 * 1024,
        gray_threshold: Some(128),
        flavor: if data.last().copied().unwrap_or(0) & 1 == 0 {
            codec::RockboxFlavor::Rb12
        } else {
            codec::RockboxFlavor::Rb11
        },
        depth: Some(if data.last().copied().unwrap_or(0) & 3 == 0 {
            codec::RockboxDepth::Gray4
        } else {
            codec::RockboxDepth::Mono
        }),
        ..Default::default()
    };
    common::exercise(
        data,
        |c| codec::read(c, &Default::default()),
        |f, o| codec::write(f, o, &options),
    );
});
