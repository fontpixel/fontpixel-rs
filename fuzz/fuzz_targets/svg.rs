#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::formats::svg::{self,ReadOptions};
fuzz_target!(|data:&[u8]| {
    let mut o=ReadOptions::default();
    o.limits.resources.max_input_bytes=65536;o.limits.resources.max_table_bytes=8192;
    o.limits.resources.max_total_bytes=262144;o.limits.resources.max_glyphs=64;
    o.limits.resources.max_pixels_per_glyph=4096;o.limits.max_nodes=2048;
    o.limits.max_depth=16;o.limits.max_attributes=32;o.limits.max_fonts=8;
    o.limits.max_total_edges=8192;o.limits.max_total_pixels=32768;
    o.limits.geometry.max_edges=2048;o.limits.geometry.max_points=4096;
    o.limits.geometry.max_pixels=4096;o.limits.geometry.max_intersections=8192;
    let _=svg::read(data,&o);o.ppem=Some(16);let _=svg::read(data,&o);
});
