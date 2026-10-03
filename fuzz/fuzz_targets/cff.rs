#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::sfnt::{Sfnt,cff::{Cff,Limits}};
fuzz_target!(|data:&[u8]| {
    let mut l=Limits::default();l.sfnt.resources.max_input_bytes=65536;
    l.sfnt.resources.max_table_bytes=65536;l.sfnt.resources.max_glyphs=256;
    l.max_commands=2048;l.max_total_commands=8192;l.max_dict_bytes=4096;
    l.max_total_dict_bytes=16384;l.max_program_bytes=16384;l.max_total_program_bytes=65536;
    if let Ok(t)=Cff::parse(data,1400,&l){let _=t.read_all(&l);}
    if let Ok(f)=Sfnt::parse(data,&l.sfnt) && let Ok(t)=Cff::from_sfnt(&f,&l){let _=t.read_all(&l);}
});
