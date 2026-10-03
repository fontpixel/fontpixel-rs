#![no_main]
use fontpixel::{ResourceLimits, formats::hex::{self,HexReadOptions,HexWriteOptions}};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits=ResourceLimits {max_input_bytes:65536,max_total_bytes:4*1024*1024,max_table_bytes:8192,max_glyphs:256,max_pixels_per_glyph:65536,..Default::default()};
    for height in [16,8,24] {
        let geometry=HexReadOptions{height,descent:0};
        if let Ok((font,_))=hex::read(data,&geometry,&limits) {
            if let Ok((out,report))=hex::write(&font,&HexWriteOptions{geometry:geometry.clone(),limits:limits.clone(),max_output_bytes:262144,..Default::default()}) {
                assert!(!report.has_loss());
                if let Ok((back,_))=hex::read(&out,&geometry,&limits) {assert_eq!(font,back);}
            }
        }
    }
});
