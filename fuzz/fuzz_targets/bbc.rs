//! Bounded native bbc malformed data and writer same-limit reopen coverage.
#![no_main]
use fontpixel::formats::{WriteOptions, bbc::*};
use fontpixel::resources::ReadContext;
use fontpixel::{Input, ResourceLimits, ResourcePath};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let limits = ResourceLimits {
        max_input_bytes: 65536,
        max_resource_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 65536,
        max_glyphs: 600,
        max_pixels_per_glyph: 65536,
        ..Default::default()
    };
    let input = Input::new(&data[1..]);
    let Ok(mut c) = ReadContext::new(&input, &limits) else {
        return;
    };
    let Ok((s, _)) = read(&mut c, &Default::default()) else {
        return;
    };
    if let Ok(w) = write(
        &s,
        &WriteOptions::new(ResourcePath::new("font").unwrap()),
        &BbcWriteOptions {
            limits: limits.clone(),
            ..Default::default()
        },
    ) {
        let b = w.output.into_single().unwrap();
        let input = Input::new(&b);
        let mut c = ReadContext::new(&input, &limits).unwrap();
        assert!(read(&mut c, &Default::default()).is_ok());
    }
});
