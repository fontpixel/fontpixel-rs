#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::{ReadOptions, ResourceLimits};
use fontpixel::formats::yaff::{self, YaffWriteOptions};
fuzz_target!(|data: &[u8]| {
    let limits=ResourceLimits {
        max_input_bytes:65536,max_total_bytes:4*1024*1024,max_decompressed_bytes:65536,
        max_table_bytes:65536,max_glyphs:256,max_pixels_per_glyph:65536,
        ..ResourceLimits::default()
    };
    let options=ReadOptions {format:Some("yaff".into()),limits:limits.clone()};
    if let Ok(loaded)=fontpixel::read(data,&options) {
        let write=YaffWriteOptions {max_output_bytes:1024*1024,limits:limits.clone()};
        if let Ok((bytes,_))=yaff::write(&loaded.fonts,&write) {
            if bytes.len()<=limits.max_input_bytes {
                if let Ok((back,_))=yaff::read(&bytes,&limits) { assert_eq!(loaded.fonts,back); }
            }
        }
    }
});
