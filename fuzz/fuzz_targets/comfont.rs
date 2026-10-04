//! Bounded native comfont malformed data and writer same-limit reopen coverage.
#![no_main]
use fontpixel::formats::{WriteOptions, comfont::*};
use fontpixel::resources::ReadContext;
use fontpixel::{Input, ResourceLimits, ResourcePath};
use libfuzzer_sys::fuzz_target;
fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let limits = ResourceLimits {
        max_input_bytes: 65536,
        max_resource_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 65536,
        max_glyphs: 600,
        max_pixels_per_glyph: 65536,
        ..Default::default()
    };
    let input = Input::new(&data[1..]);
    let Ok(mut c) = ReadContext::new(&input, &limits) else {
        return;
    };
    let Ok((s, _)) = read(&mut c, &Default::default()) else {
        return;
    };
    if let Ok(w) = write(
        &s,
        &WriteOptions::new(ResourcePath::new("font").unwrap()),
        &ComfontWriteOptions {
            variant: match data[0] % 7 {
                0 => ComfontVariant::Fontedit,
                1 => ComfontVariant::Frapt,
                2 => ComfontVariant::FraptTsr,
                3 => ComfontVariant::Letafont,
                4 => ComfontVariant::Mania,
                5 => ComfontVariant::Psfcom,
                _ => ComfontVariant::Udg,
            },
            cpm_target: match data[0] / 7 % 3 {
                0 => CpmTarget::Pcw,
                1 => CpmTarget::Cpc,
                _ => CpmTarget::Spectrum,
            },
            limits: limits.clone(),
            ..Default::default()
        },
    ) {
        let b = w.output.into_single().unwrap();
        let input = Input::new(&b);
        let mut c = ReadContext::new(&input, &limits).unwrap();
        assert!(read(&mut c, &Default::default()).is_ok());
    }
});
