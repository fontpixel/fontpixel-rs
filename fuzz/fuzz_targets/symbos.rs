#![no_main]
use fontpixel::formats::{FormatDetector, symbos};
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = fontpixel::ResourceLimits {
        max_input_bytes: 262144,
        max_total_bytes: 4 * 1024 * 1024,
        max_glyphs: 1024,
        max_pixels_per_glyph: 65536,
        max_table_bytes: 262144,
        max_resources: 8,
        ..Default::default()
    };
    let input = Input::new(data);
    let ro = Default::default();
    if let Ok(mut c) = ReadContext::new(&input, &limits) {
        let _ = symbos::SymbosCodec::default().detect(&mut c);
        if let Ok((set, _)) = symbos::read(&mut c, &ro) {
            if let Ok(written) = symbos::write(
                &set,
                &fontpixel::WriteOptions::new(ResourcePath::new("font").unwrap()),
                &symbos::SymbosWriteOptions {
                    limits: limits.clone(),
                    max_output_bytes: 262144,
                    ..Default::default()
                },
            ) {
                let bytes = written.output.into_single().unwrap();
                let input = Input::new(&bytes);
                let mut c = ReadContext::new(&input, &limits).expect("bounded writer output");
                let (back, _) = symbos::read(&mut c, &ro)
                    .expect("successful writer output reads under the same limits");
                if !written.report.has_loss() {
                    assert_eq!(set, back);
                }
            }
        }
    }
});
