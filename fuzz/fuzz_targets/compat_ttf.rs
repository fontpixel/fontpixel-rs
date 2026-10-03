#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data:&[u8]| {
    let _=fontpixel::compat::bdfparser::ttf::read_ttf(data,Some(14));
    let _=fontpixel::compat::bdfparser::ttf::read_ttf(data,None);
});
