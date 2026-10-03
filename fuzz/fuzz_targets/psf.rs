#![no_main]
use fontpixel::{ResourceLimits, formats::psf::{self,PsfWriteOptions,Version}};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits=ResourceLimits {max_input_bytes:65536,max_total_bytes:4*1024*1024,max_table_bytes:65536,max_glyphs:512,max_pixels_per_glyph:65536,..Default::default()};
    if let Ok((font,_))=psf::read(data,&limits) {
        for version in [Version::One,Version::Two] {
            if let Ok((out,report))=psf::write(&font,&PsfWriteOptions{version,limits:limits.clone(),max_output_bytes:262144,..Default::default()}) {
                if let Ok((back,_))=psf::read(&out,&limits) {if !report.has_loss(){assert_eq!(font,back);}}
            }
        }
    }
});
