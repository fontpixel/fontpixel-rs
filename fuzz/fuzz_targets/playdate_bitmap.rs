#![no_main]
use fontpixel::formats::playdate as codec;
use libfuzzer_sys::fuzz_target;
#[path = "rockbox_bitmap/common.rs"]
mod common;
fuzz_target!(|data: &[u8]| {
    let options = codec::PlaydateWriteOptions {
        limits: common::limits(),
        max_output_bytes: 256 * 1024,
        gray_threshold: Some(128),
        flavor: match data.last().copied().unwrap_or(0) % 3 {
            0 => codec::PlaydateFlavor::FntEmbedded,
            1 => codec::PlaydateFlavor::FntPng,
            _ => codec::PlaydateFlavor::Pft,
        },
        ..Default::default()
    };
    common::exercise(
        data,
        |c| codec::read(c, &Default::default()),
        |f, o| codec::write(f, o, &options),
    );
});
