//! In-memory raw/gzip transport comparison; fixture checks precede timing.
#[cfg(all(feature = "raw", feature = "gzip"))]
mod app {
    use fontpixel::formats::raw::{RawLayout, RawReadOptions, RawWriteOptions};
    use fontpixel::{Input, ReadOptions, ResourcePath, Result, WriteOptions};
    use std::{hint::black_box, time::Instant};

    pub fn run() -> Result<()> {
        let args = std::env::args().collect::<Vec<_>>();
        let argument = |index: usize, fallback: usize| -> Result<usize> {
            args.get(index).map_or(Ok(fallback), |s| {
                s.parse().map_err(|_| {
                    fontpixel::Error::InvalidInput("expected a positive integer".into())
                })
            })
        };
        let iterations = argument(1, 200)?;
        let glyphs = argument(2, 256)?;
        if iterations == 0 || iterations > 1_000_000 || glyphs == 0 || glyphs > 65_536 {
            return Err(fontpixel::Error::InvalidInput(
                "iterations 1..=1000000; glyphs 1..=65536".into(),
            ));
        }
        // Original deterministic 8x16 cell fixture. Repeated rows and diagonal
        // strokes give compressible data without an external font dependency.
        let source = (0..glyphs)
            .flat_map(|glyph| {
                (0..16).map(move |row| {
                    let value = (glyph as u8).rotate_left((row % 8) as u32);
                    if row == 0 || row == 15 {
                        0
                    } else {
                        value ^ (0x81u8.rotate_right((row / 2) as u32))
                    }
                })
            })
            .collect::<Vec<_>>();
        let mut layout = RawLayout::new(8, 16);
        layout.count = Some(glyphs);
        let mut registry = fontpixel::registry();
        registry.set_raw_read_options(RawReadOptions {
            layout: Some(layout.clone()),
        });
        registry.set_raw_write_options(RawWriteOptions {
            layout: Some(layout),
            ..Default::default()
        });
        let read_options = ReadOptions {
            format: Some("raw".into()),
            ..Default::default()
        };
        let plain_options = WriteOptions::new(ResourcePath::new("font.raw")?);
        let gzip_options = WriteOptions::new(ResourcePath::new("font.raw.gz")?);
        let fonts = registry.read(&Input::new(&source), &read_options)?.fonts;
        let plain = registry
            .write(&fonts, "raw", &plain_options)?
            .output
            .into_single()?;
        let gzip = registry
            .write(&fonts, "raw", &gzip_options)?
            .output
            .into_single()?;
        assert_eq!(plain, source);
        let loaded = registry.read(&Input::new(&gzip), &read_options)?.fonts;
        assert_eq!(loaded, fonts);
        assert_eq!(fonts.single()?.len(), glyphs);

        let mut read_ns = [0u128; 2];
        let mut write_ns = [0u128; 2];
        for (index, (bytes, options)) in [(&plain, &plain_options), (&gzip, &gzip_options)]
            .into_iter()
            .enumerate()
        {
            let input = Input::new(bytes);
            for _ in 0..5 {
                black_box(registry.read(&input, &read_options)?);
                black_box(registry.write(&fonts, "raw", options)?);
            }
            let start = Instant::now();
            for _ in 0..iterations {
                black_box(registry.read(black_box(&input), &read_options)?);
            }
            read_ns[index] = start.elapsed().as_nanos();
            let start = Instant::now();
            for _ in 0..iterations {
                black_box(registry.write(black_box(&fonts), "raw", options)?);
            }
            write_ns[index] = start.elapsed().as_nanos();
        }
        println!(
            "{{\"schema\":1,\"release\":{},\"glyphs\":{glyphs},\"iterations\":{iterations},\"raw_bytes\":{},\"gzip_bytes\":{},\"raw_read_ns\":{},\"gzip_read_ns\":{},\"raw_write_ns\":{},\"gzip_write_ns\":{},\"gzip_native_validations\":2,\"correctness\":\"full-model-and-raw-bytes\",\"baseline\":null}}",
            !cfg!(debug_assertions),
            plain.len(),
            gzip.len(),
            read_ns[0],
            read_ns[1],
            write_ns[0],
            write_ns[1]
        );
        Ok(())
    }
}

#[cfg(all(feature = "raw", feature = "gzip"))]
fn main() -> fontpixel::Result<()> {
    app::run()
}

#[cfg(not(all(feature = "raw", feature = "gzip")))]
fn main() {}
