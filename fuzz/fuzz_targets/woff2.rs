#![no_main]
use libfuzzer_sys::fuzz_target;
use fontpixel::woff::{Limits, woff2};
fuzz_target!(|data: &[u8]| {
    let mut l=Limits::default();
    l.sfnt.resources.max_input_bytes=524288;
    l.sfnt.resources.max_total_bytes=1048576;
    l.sfnt.resources.max_decompressed_bytes=524288;
    l.sfnt.resources.max_table_bytes=262144;
    l.sfnt.resources.max_glyphs=1024;
    l.sfnt.max_tables=64;
    l.max_points_per_glyph=4096;
    l.max_total_points=16384;
    l.max_total_components=4096;
    l.max_total_instructions=65536;
    l.max_auxiliary_bytes=8192;
    let _=woff2::decode(data,&l);
    // Exercise transformed streams directly behind a valid, freshly compressed
    // envelope. Otherwise most compressed-byte mutations stop in Brotli.
    if data.first()==Some(&0xff) && data.len()>9 && data.len()<131072 {
        let raw=&data[1..];let ng=u16::from_be_bytes([raw[4],raw[5]]);
        let mut payload=raw.to_vec();payload.extend([0u8;54]);payload.extend([0,1,0,0,raw[4],raw[5]]);
        let mut encoded=Vec::new();let params=brotli::enc::BrotliEncoderParams{quality:0,..Default::default()};
        if brotli::BrotliCompress(&mut payload.as_slice(),&mut encoded,&params).is_ok(){
            let mut file=vec![0u8;48];file[..4].copy_from_slice(b"wOF2");file[4..8].copy_from_slice(&0x10000u32.to_be_bytes());file[12..14].copy_from_slice(&4u16.to_be_bytes());file[20..24].copy_from_slice(&(encoded.len()as u32).to_be_bytes());
            fn base(out:&mut Vec<u8>,n:usize){let bytes=((usize::BITS-n.leading_zeros()).max(1)as usize).div_ceil(7);for i in (0..bytes).rev(){out.push(((n>>(7*i))&127)as u8|if i==0{0}else{128});}}
            file.extend([10,0]);base(&mut file,raw.len());file.push(11);base(&mut file,(usize::from(ng)+1)*if raw[7]==0{2}else{4});file.extend([0,1,54,4,6]);file.extend(encoded);let len=file.len();file[8..12].copy_from_slice(&(len as u32).to_be_bytes());let _=woff2::decode(&file,&l);
        }
    }

});
