#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::{ReadOptions, ResourceLimits};
use fontpixel::formats::pcf::{self, PcfWriteOptions, Order, GlyphPad, ScanUnit};
fuzz_target!(|data: &[u8]| {
    let limits=ResourceLimits {
        max_input_bytes:65536,max_total_bytes:4*1024*1024,max_decompressed_bytes:65536,
        max_table_bytes:65536,max_glyphs:256,max_pixels_per_glyph:65536,
        ..ResourceLimits::default()
    };
    let options=ReadOptions {format:Some("pcf".into()),limits:limits.clone()};
    if let Ok(loaded)=fontpixel::read(data,&options) {
        for font in &loaded.fonts.fonts {
            let write=PcfWriteOptions {max_output_bytes:1024*1024,limits:limits.clone(),..PcfWriteOptions::default()};
            if let Ok((bytes,_))=pcf::write(font,&write) {let _=pcf::read(&bytes,&limits);}
            let alternate=PcfWriteOptions {byte_order:Order::Lsb,bit_order:Order::Msb,glyph_pad:GlyphPad::Eight,scan_unit:ScanUnit::Four,..write};
            let _=pcf::write(font,&alternate);
        }
    }
});
