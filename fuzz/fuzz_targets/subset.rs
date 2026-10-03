#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::sfnt::{subset::{self,Options},cff};
fuzz_target!(|data:&[u8]| {
    let mut o=Options::text("A中“…”");o.limits.max_records=4096;o.limits.sfnt.resources.max_input_bytes=65536;o.limits.sfnt.resources.max_table_bytes=65536;o.limits.sfnt.resources.max_total_bytes=262144;o.limits.sfnt.resources.max_glyphs=128;o.limits.sfnt.max_cmap_mappings=512;
    if let Ok(s)=subset::subset(data,&o){let _=subset::subset(&s.bytes,&o);}
    let l=cff::Limits{sfnt:o.limits.sfnt.clone(),max_commands:2048,max_total_commands:8192,max_program_bytes:16384,max_total_program_bytes:65536,max_dict_bytes:4096,max_total_dict_bytes:16384,..Default::default()};
    if let Ok(c)=cff::Cff::parse(data,1400,&l){let mut ids=vec![0];if c.len()>1{ids.push((c.len()-1)as u16);}if let Ok(s)=c.subset(&ids,&l){let _=cff::Cff::parse(&s,1400,&l);}}
});
