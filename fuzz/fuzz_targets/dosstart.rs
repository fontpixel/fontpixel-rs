#![no_main]
use fontpixel::formats::dosstart;
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = fontpixel::ResourceLimits {
        max_input_bytes: 262144,
        max_total_bytes: 2097152,
        max_glyphs: 1024,
        max_pixels_per_glyph: 65536,
        max_table_bytes: 262144,
        max_resources: 8,
        ..Default::default()
    };
    let input = Input::new(data);
    if let Ok(mut context) = ReadContext::new(&input, &limits) {
        if let Ok((set, _)) = dosstart::read(&mut context, &Default::default()) {
            if let Ok(w) = dosstart::write(
                &set,
                &fontpixel::WriteOptions::new(ResourcePath::new("font.dosstart").unwrap()),
                &dosstart::DosstartWriteOptions {
                    limits: limits.clone(),
                    max_output_bytes: 262144,
                    ..Default::default()
                },
            ) {
                let bytes = w.output.into_single().unwrap();
                let input = Input::new(&bytes);
                let mut c = ReadContext::new(&input, &limits).expect("output input budget");
                let (back, _) = dosstart::read(&mut c, &Default::default())
                    .expect("writer output readable under identical limits");
                if !w.report.has_loss() {
                    assert_eq!(set, back);
                }
            }
        }
    }
});
