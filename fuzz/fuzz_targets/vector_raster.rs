#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::formats::vector_raster::{self,Options};
fuzz_target!(|data:&[u8]| {
    let mut o=Options::default();o.limits.sfnt.resources.max_input_bytes=65536;o.limits.sfnt.resources.max_table_bytes=65536;o.limits.sfnt.resources.max_total_bytes=262144;o.limits.sfnt.resources.max_decompressed_bytes=262144;o.limits.sfnt.resources.max_glyphs=128;o.limits.sfnt.resources.max_pixels_per_glyph=4096;o.limits.sfnt.max_cmap_mappings=512;o.limits.max_total_edges=8192;o.limits.max_total_work=65536;o.limits.max_fonts=8;o.limits.geometry.max_edges=2048;o.limits.geometry.max_points=8192;o.limits.geometry.max_pixels=4096;o.limits.geometry.max_intersections=8192;
    let _=vector_raster::read(data,&o);
    o.ppem=Some(16);o.prefer_embedded=false;let _=vector_raster::read(data,&o);
    let _=vector_raster::detect_native_ppem(data,&o.limits);
});
