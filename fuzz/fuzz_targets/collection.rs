#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::{sfnt::collection::{self,Collection,Limits},woff};
fuzz_target!(|data:&[u8]| {
    let mut l=Limits::default();l.max_faces=8;l.max_table_references=256;l.max_expanded_bytes=262144;l.sfnt.resources.max_input_bytes=65536;l.sfnt.resources.max_table_bytes=65536;l.sfnt.resources.max_total_bytes=262144;l.sfnt.resources.max_glyphs=128;l.sfnt.resources.max_decompressed_bytes=262144;
    if let Ok(c)=Collection::parse(data,&l){let _=c.validate_checksums();let mut files=Vec::new();for i in 0..c.faces.len(){if let Ok((bytes,_))=c.extract(i,&l){files.push(bytes);}}let refs=files.iter().map(Vec::as_slice).collect::<Vec<_>>();let _=collection::encode(&refs,0x10000,&l);}
    let w=woff::Limits{sfnt:l.sfnt.clone(),max_batch_fonts:8,max_batch_bytes:262144,max_total_points:8192,max_total_components:1024,max_total_instructions:8192,max_auxiliary_bytes:4096,..Default::default()};
    if data.get(4..8)==Some(b"ttcf"){let _=woff::woff2::decode(data,&w);}
});
