//! P2 codec and independent parallel-read timings using fixed 8x16 glyphs.
use fontpixel::bitmap::{Bitmap, BitmapLimits, Bounds};
use fontpixel::formats::{FormatReader, FormatWriter};
use fontpixel::formats::{
    amiga, bmfont, cpi, draw, fontx2, gem, gf, hzk, kbits, nfnt, pf2, pk, plan9, raw, sfd, snf,
};
use fontpixel::model::{Codepoint, Font, FontSet, Glyph, Rational, Vector};
use fontpixel::resources::{OutputBundle, ReadContext, ResourceResolver};
use fontpixel::{Error, Input, ReadOptions, ResourcePath, Result, WriteOptions};
use std::{hint::black_box, sync::Barrier, time::Instant};

const GLYPHS: usize = 128;
const WARMUP: usize = 5;
const ITERATIONS: usize = 100;
const PARALLEL_READS: usize = 400;

struct Assets(OutputBundle);
impl ResourceResolver for Assets {
    fn resolve(&self, path: &ResourcePath, max: usize) -> Result<Vec<u8>> {
        let asset = self
            .0
            .assets()
            .iter()
            .find(|asset| &asset.path == path)
            .ok_or_else(|| Error::MissingResource(path.to_string()))?;
        if asset.bytes.len() > max {
            return Err(Error::ResourceLimit {
                resource: "timing sidecar bytes",
                limit: max,
            });
        }
        Ok(asset.bytes.clone())
    }
}

fn source_font(unicode: bool) -> Result<FontSet> {
    let mut font = Font::default();
    font.charset.name = Some(if unicode { "unicode" } else { "ascii" }.into());
    font.metrics.pixel_size = Some(16);
    font.metrics.point_size = Some(Rational::integer(16));
    font.metrics.resolution = Some(Vector::new(72, 72));
    font.metrics.ascent = Some(16);
    font.metrics.descent = Some(0);
    font.metrics.bounds = Some(Bounds::new(8, 16, 0, 0));

    for index in 0..GLYPHS {
        let mut rows = [0_u8; 16];
        for (row, byte) in rows.iter_mut().enumerate() {
            *byte = ((index * 17 + row * 13) & 255) as u8;
        }
        let bitmap: Bitmap = Bitmap::from_raw(8, 16, &rows, BitmapLimits::default())?;
        let mut glyph = Glyph::new(bitmap);
        glyph.codepoints.push(if unicode {
            Codepoint::Unicode(index as u32)
        } else {
            Codepoint::Legacy(index as u32)
        });
        glyph.metrics.bounds = Bounds::new(8, 16, 0, 0);
        glyph.metrics.advance = Some(Vector::new(8, 0));
        font.add_glyph(glyph)?;
    }
    Ok(FontSet::from(font))
}

fn timed(mut operation: impl FnMut() -> Result<()>) -> Result<f64> {
    for _ in 0..WARMUP {
        operation()?;
    }
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        operation()?;
    }
    Ok(start.elapsed().as_secs_f64() * 1e6 / ITERATIONS as f64)
}

fn parallel_reads<C: FormatReader<FontSet> + Sync>(
    codec: &C,
    input: &Input<'_>,
    options: &ReadOptions,
    jobs: usize,
) -> Result<f64> {
    let ready = Barrier::new(jobs + 1);
    let begin = Barrier::new(jobs + 1);
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(jobs);
        for worker in 0..jobs {
            let ready = &ready;
            let begin = &begin;
            handles.push(scope.spawn(move || -> Result<()> {
                // Warmup and thread creation occur before the timed barrier.
                let warmup =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
                        for _ in 0..WARMUP {
                            black_box(
                                codec.read(
                                    &mut ReadContext::new(input, &options.limits)?,
                                    options,
                                )?,
                            );
                        }
                        Ok(())
                    }));
                // Always reach both barriers, including failed warmups, so a
                // worker error cannot leave the coordinator waiting forever.
                ready.wait();
                begin.wait();
                warmup.map_err(|_| Error::InvalidInput("timing warmup panicked".into()))??;
                let count = PARALLEL_READS / jobs + usize::from(worker < PARALLEL_READS % jobs);
                for _ in 0..count {
                    black_box(codec.read(&mut ReadContext::new(input, &options.limits)?, options)?);
                }
                Ok(())
            }));
        }
        ready.wait();
        let start = Instant::now();
        begin.wait();
        for handle in handles {
            handle
                .join()
                .map_err(|_| Error::InvalidInput("timing worker panicked".into()))??;
        }
        Ok(start.elapsed().as_secs_f64() * 1000.)
    })
}

fn benchmark<C: FormatReader<FontSet> + FormatWriter<FontSet> + Sync>(
    name: &str,
    codec: C,
    font: &FontSet,
    records: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<()> {
    let write_options = WriteOptions::new(ResourcePath::new(format!("font.{name}"))?);
    let options = ReadOptions::default();
    let written = codec.write(font, &write_options)?;
    let write_losses = written
        .report
        .entries
        .iter()
        .filter(|entry| entry.impact == fontpixel::report::Impact::Loss)
        .count();
    let assets = Assets(written.output);
    let primary = assets
        .0
        .assets()
        .iter()
        .find(|asset| &asset.path == assets.0.primary())
        .ok_or_else(|| Error::InvalidInput("benchmark primary asset is missing".into()))?;
    let input = Input::new(&primary.bytes)
        .with_name(primary.path.as_str())
        .with_resources(&assets);
    // Verify the generated layout before any timed work. Every source code and
    // raster must survive even if the target reports loss of other metadata.
    let (decoded, report) =
        codec.read(&mut ReadContext::new(&input, &options.limits)?, &options)?;
    let decoded = decoded.single()?;
    let source = font.single()?;
    if decoded.len() < source.len() {
        return Err(Error::InvalidInput("benchmark lost glyphs".into()));
    }
    for (_, glyph) in source.glyphs() {
        let code = glyph.codepoints[0];
        let id = decoded
            .glyphs()
            .find(|(_, g)| g.codepoints.iter().any(|c| c.value() == code.value()))
            .map(|(id, _)| id)
            .ok_or_else(|| Error::InvalidInput("benchmark mapping changed".into()))?;
        let restored = decoded
            .glyph(id)
            .ok_or_else(|| Error::InvalidInput("benchmark glyph is missing".into()))?;
        if restored.bitmap.width() != 8 || restored.bitmap.height() != 16 {
            return Err(Error::InvalidInput(
                "benchmark glyph dimensions changed".into(),
            ));
        }
        for y in 0..16 {
            for x in 0..8 {
                let expected = glyph.bitmap.as_bytes()[y as usize] & (128 >> x) != 0;
                let actual = match &restored.bitmap {
                    fontpixel::model::GlyphBitmap::Mono(bitmap) => bitmap.get_pixel(x, y)? != 0,
                    fontpixel::model::GlyphBitmap::Gray8(bitmap) => bitmap.get_pixel(x, y)? != 0,
                };
                if expected != actual {
                    return Err(Error::InvalidInput("benchmark glyph pixels changed".into()));
                }
            }
        }
    }
    let read = timed(|| {
        black_box(codec.read(&mut ReadContext::new(&input, &options.limits)?, &options)?);
        Ok(())
    })?;
    let write = timed(|| {
        black_box(codec.write(font, &write_options)?);
        Ok(())
    })?;
    records.insert(
        name.into(),
        serde_json::json!({
            "source_glyphs": source.len(),
            "decoded_glyphs": decoded.len(),
            "read_us": read,
            "write_us": write,
            "bytes": assets.0.assets().iter().map(|asset| asset.bytes.len()).sum::<usize>(),
            "assets": assets.0.assets().len(),
            "write_losses": write_losses,
            "read_losses": report.entries.iter().filter(|entry| entry.impact == fontpixel::report::Impact::Loss).count()
        }),
    );
    for jobs in [1, 2, 4] {
        records.insert(
            format!("{name}_400_{jobs}_threads_ms"),
            serde_json::json!(parallel_reads(&codec, &input, &options, jobs)?),
        );
    }
    Ok(())
}

fn main() -> Result<()> {
    let unicode = source_font(true)?;
    let legacy = source_font(false)?;
    let mut records = serde_json::Map::new();
    benchmark("draw", draw::DrawCodec::default(), &unicode, &mut records)?;
    let layout = raw::RawLayout {
        count: Some(GLYPHS),
        charset: Some("ascii".into()),
        ..raw::RawLayout::new(8, 16)
    };
    benchmark(
        "raw",
        raw::RawCodec {
            read_options: raw::RawReadOptions {
                layout: Some(layout.clone()),
            },
            write_options: raw::RawWriteOptions {
                layout: Some(layout),
                ..Default::default()
            },
        },
        &legacy,
        &mut records,
    )?;
    benchmark(
        "cpi",
        cpi::CpiCodec {
            write_options: cpi::CpiWriteOptions {
                code_page: Some(437),
                ..Default::default()
            },
            ..Default::default()
        },
        &legacy,
        &mut records,
    )?;
    benchmark(
        "cpx",
        cpi::CpiCodec {
            write_options: cpi::CpiWriteOptions {
                version: cpi::Version::Cpx,
                code_page: Some(437),
                ..Default::default()
            },
            ..Default::default()
        },
        &legacy,
        &mut records,
    )?;
    benchmark("pf2", pf2::Pf2Codec::default(), &unicode, &mut records)?;
    benchmark(
        "fontx2",
        fontx2::Fontx2Codec {
            read_options: fontx2::Fontx2ReadOptions {
                charset: Some("ascii".into()),
            },
            write_options: fontx2::Fontx2WriteOptions {
                charset: Some("ascii".into()),
                ..Default::default()
            },
        },
        &legacy,
        &mut records,
    )?;
    let geometry = hzk::Geometry {
        width: 8,
        height: 16,
        first_code: 0,
        last_code: GLYPHS as u16 - 1,
        code_space: hzk::CodeSpace::Ascii,
        storage: hzk::Storage::Rows,
    };
    benchmark(
        "hzk",
        hzk::HzkCodec {
            read_options: hzk::HzkReadOptions {
                geometry: Some(geometry),
                ..Default::default()
            },
            write_options: hzk::HzkWriteOptions {
                geometry: Some(geometry),
                ..Default::default()
            },
        },
        &legacy,
        &mut records,
    )?;
    benchmark("nfnt", nfnt::NfntCodec::default(), &legacy, &mut records)?;
    benchmark(
        "bmfont",
        bmfont::BmfontCodec::default(),
        &unicode,
        &mut records,
    )?;
    benchmark("sfd", sfd::SfdCodec::default(), &unicode, &mut records)?;
    benchmark(
        "kbits",
        kbits::KbitsCodec::default(),
        &unicode,
        &mut records,
    )?;
    benchmark("pk", pk::PkCodec::default(), &legacy, &mut records)?;
    benchmark("gf", gf::GfCodec::default(), &legacy, &mut records)?;
    benchmark(
        "plan9",
        plan9::Plan9Codec::default(),
        &unicode,
        &mut records,
    )?;
    benchmark("amiga", amiga::AmigaCodec::default(), &legacy, &mut records)?;
    benchmark("gem", gem::GemCodec::default(), &legacy, &mut records)?;
    benchmark("snf", snf::SnfCodec::default(), &legacy, &mut records)?;
    println!(
        "{}",
        serde_json::json!({ "source_glyphs": GLYPHS, "width": 8, "height": 16, "warmup": WARMUP, "iterations": ITERATIONS, "parallel_reads": PARALLEL_READS, "parallel_thread_creation_timed": false, "timings": records })
    );
    Ok(())
}
