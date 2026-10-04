#![no_main]
use fontpixel::{ResourceLimits,formats::fnt::{self,FntWriteOptions,Version}};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data:&[u8]|{
 let limits=ResourceLimits{max_input_bytes:65536,max_total_bytes:8*1024*1024,max_table_bytes:65536,max_resources:64,max_glyphs:512,max_pixels_per_glyph:65536,..Default::default()};
 if let Ok((f,_))=fnt::read_default(data,&limits){
   for version in [Version::One,Version::Two,Version::Three]{
     if let Ok((out,r))=fnt::write(&f,&FntWriteOptions{version:Some(version),limits:limits.clone(),max_output_bytes:65536,..Default::default()}){
       let (back,_)=fnt::read_default(&out,&limits).expect("writer emits readable FNT");
       if !r.has_loss(){assert_eq!(f,back);}
     }
   }
 }
});
