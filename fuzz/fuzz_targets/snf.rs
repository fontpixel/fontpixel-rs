#![no_main]
use fontpixel::formats::snf;
use fontpixel::resources::{Input, ReadContext, ResourcePath};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = fontpixel::ResourceLimits {
        max_input_bytes: 262144,
        max_total_bytes: 2 * 1024 * 1024,
        max_glyphs: 4096,
        max_pixels_per_glyph: 65536,
        max_table_bytes: 262144,
        max_resources: 32,
        ..Default::default()
    };
    for bitmap in [
        None,
        Some(fontpixel::bitmap::ByteLayout::default()),
        Some(fontpixel::bitmap::ByteLayout {
            row_alignment: 4,
            ..Default::default()
        }),
    ] {
        let input = Input::new(data);
        if let Ok(mut c) = ReadContext::new(&input, &limits) {
            if let Ok((s, _)) = snf::read(
                &mut c,
                &snf::SnfReadOptions {
                    bitmap,
                    ..Default::default()
                },
            ) {
                if let Ok(written) = snf::write(
                    &s,
                    &fontpixel::WriteOptions::new(ResourcePath::new("font.snf").unwrap()),
                    &snf::SnfWriteOptions {
                        limits: limits.clone(),
                        max_output_bytes: 262144,
                        ..Default::default()
                    },
                ) {
                    let bytes = written.output.into_single().unwrap();
                    let input = Input::new(&bytes);
                    let mut context =
                        ReadContext::new(&input, &limits).expect("output meets input limits");
                    let (back, _) = snf::read(&mut context, &Default::default())
                        .expect("output readable under identical limits");
                    if !written.report.has_loss() {
                        assert_eq!(s, back);
                    }
                }
            }
        }
    }
});
