#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::woff::{Limits, woff1};
fuzz_target!(|data: &[u8]| {
    let mut l=Limits::default();
    l.sfnt.resources.max_input_bytes=524288;
    l.sfnt.resources.max_total_bytes=1048576;
    l.sfnt.resources.max_decompressed_bytes=524288;
    l.sfnt.resources.max_table_bytes=262144;
    l.sfnt.resources.max_glyphs=1024;
    l.sfnt.max_tables=64;
    l.max_points_per_glyph=4096;
    l.max_total_points=16384;
    l.max_total_components=4096;
    l.max_total_instructions=65536;
    l.max_auxiliary_bytes=8192;
    let _=woff1::decode(data,&l);
});
