#![no_main]
use fontpixel::{
    ResourceLimits,
    formats::fbit::{self, FbitReadOptions, FbitWriteOptions, ResourceKind},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = ResourceLimits {
        max_input_bytes: 1 << 16,
        max_resource_bytes: 1 << 16,
        max_total_bytes: 8 << 20,
        max_resources: 128,
        max_decompressed_bytes: 1 << 16,
        max_glyphs: 512,
        max_pixels_per_glyph: 1 << 16,
        max_table_bytes: 1 << 16,
    };
    std::hint::black_box(fbit::detect(data));
    for kind in [ResourceKind::Fbit, ResourceKind::Hfnt] {
        if let Ok((mut set, _)) = fbit::read(
            data,
            &FbitReadOptions {
                kind,
                ..Default::default()
            },
            &limits,
        ) {
            for canonical in [false, true] {
                if canonical {
                    for f in &mut set.fonts {
                        f.properties.remove("fbit.source");
                    }
                }
                let options = FbitWriteOptions {
                    kind,
                    limits: limits.clone(),
                    max_output_bytes: 1 << 16,
                    ..Default::default()
                };
                if let Ok((out, report)) = fbit::write(&set, &options) {
                    let (mut again, _) = fbit::read(
                        &out,
                        &FbitReadOptions {
                            kind,
                            ..Default::default()
                        },
                        &limits,
                    )
                    .expect("same-limit successful writer output");
                    if !report.has_loss() {
                        set.properties.remove("fbit.source");
                        again.properties.remove("fbit.source");
                        for font in &mut set.fonts {
                            font.properties.remove("fbit.source");
                        }
                        for font in &mut again.fonts {
                            font.properties.remove("fbit.source");
                        }
                        assert_eq!(set, again);
                    }
                }
            }
        }
    }
});
