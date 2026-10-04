#![no_main]
use fontpixel::formats::fzx;
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = fontpixel::ResourceLimits {
        max_input_bytes: 262144,
        max_total_bytes: 2 * 1024 * 1024,
        max_glyphs: 1024,
        max_pixels_per_glyph: 65536,
        max_table_bytes: 262144,
        max_resources: 8,
        ..Default::default()
    };
    let input = Input::new(data);
    if let Ok(mut context) = ReadContext::new(&input, &limits) {
        if let Ok((set, _)) = fzx::read(&mut context, &Default::default()) {
            if let Ok(written) = fzx::write(
                &set,
                &fontpixel::WriteOptions::new(ResourcePath::new("font.fzx").unwrap()),
                &fzx::FzxWriteOptions {
                    limits: limits.clone(),
                    max_output_bytes: 262144,
                    ..Default::default()
                },
            ) {
                let bytes = written.output.into_single().unwrap();
                let input = Input::new(&bytes);
                let mut context =
                    ReadContext::new(&input, &limits).expect("output meets input limits");
                let (back, _) = fzx::read(&mut context, &Default::default())
                    .expect("output readable with identical limits");
                if !written.report.has_loss() {
                    assert_eq!(set, back);
                }
            }
        }
    }
});
