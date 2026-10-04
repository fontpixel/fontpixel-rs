#![no_main]
use fontpixel::formats::sfont as codec;
use libfuzzer_sys::fuzz_target;
#[path = "pilfont_bitmap/common.rs"]
mod common;
fuzz_target!(|data: &[u8]| {
    for flavor in [codec::SfontFlavor::Sfont, codec::SfontFlavor::Rfont] {
        let o = codec::SfontWriteOptions {
            limits: common::limits(),
            max_output_bytes: 256 * 1024,
            flavor,
            ..Default::default()
        };
        let r = codec::SfontReadOptions {
            flavor: Some(flavor),
            ..Default::default()
        };
        common::exercise(
            data,
            "font.png",
            false,
            |c| codec::read(c, &r),
            |s, v| codec::write(s, v, &o),
        );
    }
});
