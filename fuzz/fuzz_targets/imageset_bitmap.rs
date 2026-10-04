#![no_main]
use fontpixel::formats::image::{ImageCodec, ImageLayout, ImageWriteOptions};
use fontpixel::formats::{FormatReader, FormatWriter};
use libfuzzer_sys::fuzz_target;
#[path = "pilfont_bitmap/common.rs"]
mod common;
fuzz_target!(|data: &[u8]| {
    let c = ImageCodec {
        write_options: ImageWriteOptions {
            layout: ImageLayout::Imageset,
            limits: common::limits(),
            ..Default::default()
        },
        ..Default::default()
    };
    common::exercise(
        data,
        "glyphs",
        true,
        |r| c.read(r, &Default::default()),
        |s, o| c.write(s, o),
    );
});
