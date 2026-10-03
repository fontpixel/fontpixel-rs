#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::{outline, sfnt::{Sfnt,glyf::{GlyfTable,Limits,LocaFormat}}};
fuzz_target!(|data:&[u8]| {
    let mut limits=Limits::default();limits.sfnt.resources.max_input_bytes=65536;
    limits.sfnt.resources.max_table_bytes=65536;limits.sfnt.resources.max_total_bytes=1024*1024;
    limits.sfnt.resources.max_glyphs=256;limits.geometry.max_points=4096;
    limits.max_components=256;limits.max_depth=16;limits.max_visits=8192;
    limits.max_total_points=16384;limits.max_total_components=16384;limits.max_total_instructions=65536;
    let inspect=|t:GlyfTable| {
        for id in 0..t.glyphs.len().min(16) {if let Ok(cs)=t.contours(id as u16,&limits) {let _=outline::rasterize(&cs,100.,(-2,-2),8,8,&limits.geometry);}}
        if let Ok(e)=t.encode(LocaFormat::Auto,&limits) {assert_eq!(GlyfTable::decode(&e.glyf,&e.loca,t.glyphs.len() as u16,e.index_to_loc_format,&limits).unwrap(),t);}
    };
    if let Ok(f)=Sfnt::parse(data,&limits.sfnt) && let Ok(t)=GlyfTable::from_sfnt(&f,&limits){inspect(t);}
    let mut loca=vec![0;4];loca.extend((data.len() as u32).to_be_bytes());
    if let Ok(t)=GlyfTable::decode(data,&loca,1,1,&limits){inspect(t);}
    if data.len()>4 {let n=(data[0] as usize).min(32)+1;let size=if data[1]&1==0{2}else{4};let split=2+(n+1)*size;
        if split<=data.len() && let Ok(t)=GlyfTable::decode(&data[split..],&data[2..split],n as u16,if size==2{0}else{1},&limits){inspect(t);}}
});
