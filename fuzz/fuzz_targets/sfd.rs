#![no_main]
use fontpixel::{
    ResourceLimits,
    formats::sfd::{self, SfdWriteOptions},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    let limits = ResourceLimits {
        max_input_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 32768,
        max_glyphs: 128,
        max_pixels_per_glyph: 65536,
        max_resources: 16,
        ..Default::default()
    };
    if let Ok((font, _)) = sfd::read(data, &limits) {
        let options = SfdWriteOptions {
            limits: limits.clone(),
        };
        if let Ok((bytes, report)) = sfd::write(&font, &options) {
            let (back, _) =
                sfd::read(&bytes, &limits).expect("SFD output readable under identical limits");
            if !report.has_loss() {
                assert_eq!(font, back);
            }
            let (again, _) = sfd::write(&back, &options).expect("canonical SFD remains writable");
            assert_eq!(bytes, again);
        }
    }
});
