#![no_main]
use fontpixel::ResourceLimits;
use fontpixel::formats::qpf2::{self, Qpf2WriteOptions};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = ResourceLimits {
        max_input_bytes: 1 << 16,
        max_resource_bytes: 1 << 16,
        max_total_bytes: 8 << 20,
        max_resources: 256,
        max_decompressed_bytes: 1 << 16,
        max_glyphs: 512,
        max_pixels_per_glyph: 1 << 16,
        max_table_bytes: 1 << 16,
    };
    std::hint::black_box(qpf2::detect(data));
    if let Ok((mut set, _)) = qpf2::read(data, &Default::default(), &limits) {
        for canonical in [false, true] {
            if canonical {
                set.properties.remove("qpf2.source");
                for f in &mut set.fonts {
                    f.properties.remove("qpf2.source");
                }
            }
            let options = Qpf2WriteOptions {
                limits: limits.clone(),
                max_output_bytes: 1 << 16,
                ..Default::default()
            };
            if let Ok((out, report)) = qpf2::write(&set, &options) {
                let (mut again, _) = qpf2::read(&out, &Default::default(), &limits)
                    .expect("same-limit successful writer output");
                if !report.has_loss() {
                    set.properties.remove("qpf2.source");
                    again.properties.remove("qpf2.source");
                    for font in &mut set.fonts {
                        font.properties.remove("qpf2.source");
                    }
                    for font in &mut again.fonts {
                        font.properties.remove("qpf2.source");
                    }
                    assert_eq!(set, again);
                }
            }
        }
    }
});
