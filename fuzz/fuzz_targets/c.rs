//! Bounded native C and LVGL binary fuzz/readback coverage.
#![no_main]
use fontpixel::formats::c::{
    ArrayOptions, CCodec, CContainer, CReadOptions, CWriteOptions, Family, LvglBpp,
    LvglCompression, U8glibFormat,
};
use fontpixel::formats::{FormatDetector, FormatReader, FormatWriter, ReadOptions, WriteOptions};
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
    let generic = if data[0] % 11 == 1 {
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
    let family = match data[0] % 11 {
        1 => Family::Generic,
        2 => Family::Font8x8,
        3 => Family::Tmat,
        4 => Family::Gfx,
        5 => Family::U8g2,
        6 => Family::Lvgl,
        7 => Family::Netbsd,
        8 => Family::U8glib,
        9 => Family::U8x8,
        _ => Family::Auto,
    };
    let reader = CCodec {
        read_options: CReadOptions {
            family,
            generic,
            array_name: None,
            u8x8_legacy: data[0] % 11 == 9 && (data[0] / 11) % 2 != 0,
            container: if data[0] % 11 == 10 {
                Some(CContainer::LvglBinary)
            } else {
                None
            },
            ..Default::default()
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
    let _ = reader.detect(&mut context);
    if let Ok((font, _)) = reader.read(&mut context, &options) {
        for family in [
            Family::Generic,
            Family::Gfx,
            Family::U8g2,
            Family::Lvgl,
            Family::U8glib,
            Family::U8x8,
            Family::Netbsd,
        ] {
            let writer = CCodec {
                write_options: CWriteOptions {
                    family,
                    gray_threshold: Some(128),
                    u8x8_legacy: data[0] & 128 != 0,
                    u8glib_format: match (data[0] / 11) % 3 {
                        1 => U8glibFormat::Compact,
                        2 => U8glibFormat::Gray2,
                        _ => U8glibFormat::Full,
                    },
                    lvgl_bpp: match (data[0] / 11) % 4 {
                        1 => LvglBpp::Two,
                        2 => LvglBpp::Four,
                        3 => LvglBpp::Eight,
                        _ => LvglBpp::One,
                    },
                    lvgl_compression: match (data[0] / 11) % 3 {
                        1 => LvglCompression::Rle,
                        2 => LvglCompression::RleXor,
                        _ => LvglCompression::Plain,
                    },
                    container: if family == Family::Lvgl && data[0] & 64 != 0 {
                        CContainer::LvglBinary
                    } else {
                        CContainer::Source
                    },
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
                let (back, _) = CCodec {
                    read_options: CReadOptions {
                        u8x8_legacy: writer.write_options.u8x8_legacy,
                        ..Default::default()
                    },
                    ..Default::default()
                }
                .read(&mut context, &options)
                .expect("C writer output is readable");
                if !w.report.has_loss() {
                    assert_eq!(font, back);
                }
            }
        }
    }
});
