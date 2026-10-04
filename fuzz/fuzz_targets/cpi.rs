#![no_main]
use fontpixel::{
    ResourceLimits,
    formats::cpi::{self, CpiReadOptions, CpiWriteOptions, PointerMode, Version},
};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    let l = ResourceLimits {
        max_input_bytes: 65536,
        max_total_bytes: 8 * 1024 * 1024,
        max_table_bytes: 65536,
        max_decompressed_bytes: 65536,
        max_glyphs: 512,
        max_pixels_per_glyph: 65536,
        ..Default::default()
    };
    // Repair only transport checksums in a second bounded copy so mutations can
    // reach the native NRV decoder rather than always stopping at Adler-32.
    if bytes.len() <= 65536
        && let Some(at) = bytes[..bytes.len().min(128)]
            .windows(4)
            .position(|w| w == b"UPX!")
        && at + 22 <= bytes.len()
    {
        let packed = usize::from(u16::from_le_bytes([bytes[at + 18], bytes[at + 19]]));
        if let Some(stream) = bytes.get(at + 22..at + 22 + packed) {
            let (mut a, mut b) = (1u32, 0u32);
            for &value in stream {
                a = (a + u32::from(value)) % 65521;
                b = (b + a) % 65521;
            }
            let mut repaired = bytes.to_vec();
            repaired[at + 12..at + 16].copy_from_slice(&((b << 16) | a).to_le_bytes());
            repaired[at + 21] = (repaired[at + 4..at + 21]
                .iter()
                .map(|&v| u32::from(v))
                .sum::<u32>()
                % 251) as u8;
            let _ = cpi::read(&repaired, &Default::default(), &l);
        }
    }
    for pointer_mode in [PointerMode::Linear, PointerMode::Segmented] {
        if let Ok((font, _)) = cpi::read(bytes, &CpiReadOptions { pointer_mode }, &l) {
            for version in [
                Version::Font,
                Version::FontNt,
                Version::DrFont,
                Version::Cp,
                Version::Cpx,
            ] {
                if let Ok((b, r)) = cpi::write(
                    &font,
                    &CpiWriteOptions {
                        version,
                        limits: l.clone(),
                        max_output_bytes: 65536,
                        ..Default::default()
                    },
                ) {
                    let back = cpi::read(&b, &Default::default(), &l)
                        .expect("successful CPI output must decode")
                        .0;
                    if !r.has_loss() {
                        assert_eq!(font, back);
                    }
                }
            }
        }
    }
});
