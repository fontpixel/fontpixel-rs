#![no_main]
use fontpixel::{
    ResourceLimits,
    formats::pbf::{self, Compression, PbfReadOptions, PbfWriteOptions, Version},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = ResourceLimits {
        max_input_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 65536,
        max_glyphs: 512,
        max_pixels_per_glyph: 65536,
        ..Default::default()
    };
    for trim in [false, true] {
        if let Ok((font, _)) = pbf::read(data, &PbfReadOptions { trim }, &limits) {
            for (version, compression) in [
                (Version::One, Compression::Raw),
                (Version::Two, Compression::Raw),
                (Version::Three, Compression::Raw),
                (Version::Three, Compression::Rle4),
            ] {
                if let Ok((out, report)) = pbf::write(
                    &font,
                    &PbfWriteOptions {
                        version,
                        compression,
                        limits: limits.clone(),
                        max_output_bytes: 262144,
                        ..Default::default()
                    },
                ) {
                    let back = pbf::read(&out, &PbfReadOptions::default(), &limits)
                        .expect("PBF writer output must decode")
                        .0;
                    if !report.has_loss() {
                        assert_eq!(font, back);
                    }
                }
            }
        }
    }
});
