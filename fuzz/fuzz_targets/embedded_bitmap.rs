#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::{sfnt::bitmap::{self, Kind, Limits}, formats::embedded};
fuzz_target!(|data:&[u8]| {
    let mut l=Limits::default();l.max_strikes=8;l.max_depth=8;l.max_pixel_work=262144;l.max_visits=4096;
    l.sfnt.resources.max_glyphs=256;l.sfnt.resources.max_pixels_per_glyph=4096;l.sfnt.resources.max_total_bytes=524288;l.sfnt.resources.max_table_bytes=131072;l.sfnt.max_cmap_mappings=1024;l.sfnt.max_name_records=256;
    for kind in [Kind::OpenType,Kind::Apple] {
        let _=embedded::read(data,kind,&embedded::EmbeddedReadOptions{limits:l.clone(),..Default::default()});
        if data.len()>=4 {
            let split=u16::from_be_bytes([data[0],data[1]]) as usize;let count=u16::from_be_bytes([data[2],data[3]])%256+1;
            if split<=data.len()-4 {let _=bitmap::decode(&data[4..4+split],&data[4+split..],kind,count,&l);}
        }
    }
    let _=bitmap::decode_scaling(data,&l);
});
