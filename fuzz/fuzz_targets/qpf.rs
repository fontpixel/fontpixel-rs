#![no_main]
use fontpixel::ResourceLimits;
use fontpixel::formats::qpf::{self, QpfWriteOptions};
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
    std::hint::black_box(qpf::detect(data));
    if let Ok((mut set, _)) = qpf::read(data, &Default::default(), &limits) {
        for canonical in [false, true] {
            if canonical {
                set.properties.remove("qpf.source");
                for f in &mut set.fonts {
                    f.properties.remove("qpf.source");
                }
            }
            let options = QpfWriteOptions {
                limits: limits.clone(),
                max_output_bytes: 1 << 16,
                ..Default::default()
            };
            if let Ok((out, report)) = qpf::write(&set, &options) {
                let (mut again, _) = qpf::read(&out, &Default::default(), &limits)
                    .expect("same-limit successful writer output");
                if !report.has_loss() {
                    set.properties.remove("qpf.source");
                    again.properties.remove("qpf.source");
                    for font in &mut set.fonts {
                        font.properties.remove("qpf.source");
                    }
                    for font in &mut again.fonts {
                        font.properties.remove("qpf.source");
                    }
                    assert_eq!(set, again);
                }
            }
        }
    }
});
