#![no_main]
use fontpixel::{
    formats::embedded,
    sfnt::bitmap::{self, Kind, Limits},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let mut l = Limits::default();
    l.max_strikes = 8;
    l.max_depth = 8;
    l.max_pixel_work = 262144;
    l.max_visits = 4096;
    l.sfnt.resources.max_glyphs = 256;
    l.sfnt.resources.max_pixels_per_glyph = 4096;
    l.sfnt.resources.max_total_bytes = 524288;
    l.sfnt.resources.max_table_bytes = 131072;
    l.sfnt.max_cmap_mappings = 1024;
    l.sfnt.max_name_records = 256;
    for kind in [Kind::OpenType, Kind::Apple] {
        if let Ok((fonts, _)) = embedded::read(
            data,
            kind,
            &embedded::EmbeddedReadOptions {
                limits: l.clone(),
                ..Default::default()
            },
        ) {
            if let Ok((bytes, report)) = embedded::write(
                &fonts,
                kind,
                &embedded::EmbeddedWriteOptions {
                    limits: l.clone(),
                    ..Default::default()
                },
            ) {
                let (reopened, _) = embedded::read(
                    &bytes,
                    kind,
                    &embedded::EmbeddedReadOptions {
                        limits: l.clone(),
                        ..Default::default()
                    },
                )
                .expect("successful embedded output must reopen under identical limits");
                if !report.has_loss() {
                    assert_eq!(fonts, reopened, "unreported embedded model loss");
                }
            }
        }
        if data.len() >= 4 {
            let split = u16::from_be_bytes([data[0], data[1]]) as usize;
            let count = u16::from_be_bytes([data[2], data[3]]) % 256 + 1;
            if split <= data.len() - 4 {
                let _ = bitmap::decode(&data[4..4 + split], &data[4 + split..], kind, count, &l);
            }
        }
    }
    let _ = bitmap::decode_scaling(data, &l);
});
