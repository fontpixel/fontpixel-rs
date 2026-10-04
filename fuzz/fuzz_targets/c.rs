#![no_main]
use fontpixel::formats::c::{ArrayOptions, CCodec, CReadOptions, CWriteOptions, Family};
use fontpixel::formats::{FormatReader, FormatWriter, ReadOptions, WriteOptions};
use fontpixel::resources::{ReadContext, ResourceResolver};
use fontpixel::{Input, ResourceLimits, ResourcePath};
use libfuzzer_sys::fuzz_target;
struct Side<'a>(&'a [u8]);
impl ResourceResolver for Side<'_> {
    fn resolve(&self, _: &ResourcePath, max: usize) -> fontpixel::Result<Vec<u8>> {
        if self.0.len() > max {
            return Err(fontpixel::Error::ResourceLimit {
                resource: "C fuzz sidecar",
                limit: max,
            });
        }
        Ok(self.0.to_vec())
    }
}
fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let limits = ResourceLimits {
        max_input_bytes: 65536,
        max_resource_bytes: 65536,
        max_total_bytes: 4 * 1024 * 1024,
        max_table_bytes: 65536,
        max_resources: 4,
        max_glyphs: 512,
        max_pixels_per_glyph: 65536,
        ..Default::default()
    };
    let generic = if data[0] % 5 == 1 {
        Some(ArrayOptions {
            width: 9,
            height: 2,
            first_code: 65,
            charset: "unicode".into(),
            baseline: 1,
            lsb_first: false,
        })
    } else {
        None
    };
    let family = match data[0] % 5 {
        1 => Family::Generic,
        2 => Family::Font8x8,
        3 => Family::Tmat,
        4 => Family::Gfx,
        _ => Family::Auto,
    };
    let reader = CCodec {
        read_options: CReadOptions {
            family,
            generic,
            array_name: None,
        },
        ..Default::default()
    };
    let side = Side(&data[1..]);
    let input = Input::new(&data[1..])
        .with_name("font.h")
        .with_resources(&side);
    let options = ReadOptions {
        limits: limits.clone(),
        ..Default::default()
    };
    let Ok(mut context) = ReadContext::new(&input, &limits) else {
        return;
    };
    if let Ok((font, _)) = reader.read(&mut context, &options) {
        for family in [Family::Generic, Family::Gfx] {
            let writer = CCodec {
                write_options: CWriteOptions {
                    family,
                    limits: limits.clone(),
                    ..Default::default()
                },
                ..Default::default()
            };
            if let Ok(w) = writer.write(
                &font,
                &WriteOptions::new(ResourcePath::new("font.h").unwrap()),
            ) {
                let bytes = w.output.into_single().unwrap();
                let input = Input::new(&bytes).with_name("font.h");
                let mut context = ReadContext::new(&input, &limits).unwrap();
                let (back, _) = CCodec::default()
                    .read(&mut context, &options)
                    .expect("C writer output is readable");
                if !w.report.has_loss() {
                    assert_eq!(font, back);
                }
            }
        }
    }
});
