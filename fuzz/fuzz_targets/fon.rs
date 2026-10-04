#![no_main]
use fontpixel::{ResourceLimits,formats::{fnt::FntWriteOptions,fon}};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data:&[u8]|{
 let limits=ResourceLimits{max_input_bytes:65536,max_total_bytes:8*1024*1024,max_table_bytes:65536,max_resource_bytes:65536,max_resources:64,max_glyphs:512,max_pixels_per_glyph:65536,..Default::default()};
 if let Ok((f,_))=fon::read(data,&Default::default(),&limits){
   if let Ok((out,r))=fon::write(&f,&FntWriteOptions{limits:limits.clone(),max_output_bytes:65536,..Default::default()}){
     let (back,_)=fon::read(&out,&Default::default(),&limits).expect("writer emits readable NE");
     if !r.has_loss(){assert_eq!(f.fonts,back.fonts);}
   }
 }
});
