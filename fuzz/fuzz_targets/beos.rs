#![no_main]
use fontpixel::formats::beos;
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
    for coverage in [beos::BeosCoverage::Native8, beos::BeosCoverage::FullNibble] {
        let ro = beos::BeosReadOptions {
            coverage,
            round_fractional_metrics: true,
            ..Default::default()
        };
        if let Ok(mut c) = ReadContext::new(&input, &limits) {
            if let Ok((set, _)) = beos::read(&mut c, &ro) {
                if let Ok(written) = beos::write(
                    &set,
                    &fontpixel::WriteOptions::new(ResourcePath::new("font").unwrap()),
                    &beos::BeosWriteOptions {
                        coverage: Some(coverage),
                        round_fractional_metrics: true,
                        limits: limits.clone(),
                        max_output_bytes: 262144,
                        ..Default::default()
                    },
                ) {
                    let bytes = written.output.into_single().unwrap();
                    let input = Input::new(&bytes);
                    let mut c = ReadContext::new(&input, &limits).expect("bounded writer output");
                    let (back, _) = beos::read(&mut c, &ro)
                        .expect("successful writer output reads under the same limits");
                    if !written.report.has_loss() {
                        assert_eq!(set, back);
                    }
                }
            }
        }
    }
});
