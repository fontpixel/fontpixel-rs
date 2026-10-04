//! Reconstructed full-input single-call driver. Five fresh processes are owned
//! by scripts/phase6_perf.py; this example performs one warmup and one observation.
//! Artifact materialization is outside each inner timer. No historical result
//! validates this restored, currently uncompiled driver.
use fontpixel::{
    FontSet, ReadOptions, ResourceLimits, ResourcePath, WriteOptions,
    bitmap::{Alignment, Bitmap, Direction, Mono},
    compat::bdfparser::{Bitmap as CompatBitmap, Font as CompatFont},
    formats::bdf,
    render::{GlyphOrder, RenderMode, RenderOptions},
};
use std::{
    collections::BTreeMap, error::Error, fs, hint::black_box, io::Write, path::Path, time::Instant,
};
type AnyResult<T> = Result<T, Box<dyn Error>>;

fn once<T>(
    metrics: &mut BTreeMap<String, u128>,
    name: &str,
    mut op: impl FnMut() -> fontpixel::Result<T>,
) -> fontpixel::Result<T> {
    drop(black_box(op()?));
    let start = Instant::now();
    let result = op()?;
    metrics.insert(name.into(), start.elapsed().as_nanos());
    black_box(&result);
    Ok(result)
}
fn report(out: &Path, name: &str, value: &fontpixel::Report) -> AnyResult<()> {
    serde_json::to_writer(
        fs::File::create(out.join(format!("{name}.report.json")))?,
        value,
    )?;
    Ok(())
}
fn written(out: &Path, name: &str, value: fontpixel::resources::Written) -> AnyResult<()> {
    report(out, name, &value.report)?;
    fs::write(out.join(name), value.output.into_single()?)?;
    Ok(())
}
fn raster(out: &Path, name: &str, width: u32, height: u32, bytes: &[u8]) -> AnyResult<()> {
    let mut file = fs::File::create(out.join(name))?;
    file.write_all(b"P6R1")?;
    file.write_all(&width.to_be_bytes())?;
    file.write_all(&height.to_be_bytes())?;
    file.write_all(bytes)?;
    Ok(())
}
fn native_raster(out: &Path, name: &str, value: &Bitmap<Mono>) -> AnyResult<()> {
    raster(out, name, value.width(), value.height(), value.as_bytes())
}
fn compat_raster(out: &Path, name: &str, value: &CompatBitmap) -> AnyResult<()> {
    let stride = value.width().div_ceil(8) as usize;
    let mut bytes = vec![0u8; stride * value.height() as usize];
    for (y, row) in value.bindata().iter().enumerate() {
        for (x, pixel) in row.bytes().enumerate() {
            if pixel == b'1' {
                bytes[y * stride + x / 8] |= 0x80 >> (x % 8);
            } else if pixel != b'0' {
                return Err("benchmark raster unexpectedly contains label 2".into());
            }
        }
    }
    raster(out, name, value.width(), value.height(), &bytes)
}
fn tiny_native() -> fontpixel::Result<Bitmap<Mono>> {
    let mut bitmap = Bitmap::new(64, 64)?;
    for y in 0..64 {
        for x in 0..64 {
            bitmap.set_pixel(x, y, u8::from((x * 13 + y * 7) % 11 < 4))?;
        }
    }
    Ok(bitmap)
}
fn tiny_compat() -> fontpixel::Result<CompatBitmap> {
    let rows = (0..64)
        .map(|y| {
            (0..64)
                .map(|x| if (x * 13 + y * 7) % 11 < 4 { '1' } else { '0' })
                .collect()
        })
        .collect::<Vec<String>>();
    CompatBitmap::new(&rows)
}
fn native(input: &[u8], out: &Path, metrics: &mut BTreeMap<String, u128>) -> AnyResult<usize> {
    let limits = ResourceLimits::default();
    let (font, parsed_report) = once(metrics, "parse", || bdf::read(black_box(input), &limits))?;
    report(out, "parse", &parsed_report)?;
    let fonts = FontSet::from(font);
    let read_options = ReadOptions {
        format: Some("bdf".into()),
        ..Default::default()
    };
    let public = once(metrics, "parse_public", || {
        fontpixel::read(black_box(input), &read_options)
    })?;
    report(out, "parse_public", &public.report)?;
    drop(public);
    for format in ["bdf", "pcf", "ttf"] {
        let name = format!("native.{format}");
        let options = WriteOptions::new(ResourcePath::new(&name)?);
        let value = once(metrics, &format!("write_{format}"), || {
            fontpixel::write(black_box(&fonts), format, &options)
        })?;
        written(out, &name, value)?;
    }
    for format in ["pcf", "otb", "yaff", "ttf"] {
        let name = format!("converted.{format}");
        let options = WriteOptions::new(ResourcePath::new(&name)?);
        // Keep this complete closure: the temporary FontSet and read Report are
        // destroyed before return; returned Written is destroyed outside timer.
        let value = once(metrics, &format!("bdf_to_{format}"), || {
            let (font, _read_report) = bdf::read(black_box(input), &limits)?;
            fontpixel::write(&FontSet::from(font), format, &options)
        })?;
        written(out, &name, value)?;
    }
    let font = fonts.single()?;
    let bounds = font.metrics.bounds.ok_or("full-input font lacks bounds")?;
    let text = "永".repeat(2000);
    let options = RenderOptions {
        mode: RenderMode::Cell,
        line_limit: i64::from(bounds.width) * 80,
        ..Default::default()
    };
    let screen = once(metrics, "render_screen", || {
        font.draw(black_box(&text), &options)
    })?;
    report(out, "render_screen", &screen.report)?;
    native_raster(out, "screen.raster", &screen.bitmap)?;
    drop(screen);
    let options = RenderOptions {
        mode: RenderMode::Advance,
        ..Default::default()
    };
    let atlas = once(metrics, "drawall_mode1", || {
        font.draw_all(GlyphOrder::Ascending, &[], &options)
    })?;
    report(out, "drawall_mode1", &atlas.report)?;
    native_raster(out, "drawall.raster", &atlas.bitmap)?;
    drop(atlas);
    let bitmap = tiny_native()?;
    let crop = once(metrics, "crop", || bitmap.crop(31, 31, 3, 5))?;
    native_raster(out, "crop.raster", &crop)?;
    let concat = once(metrics, "concat", || {
        bitmap.concat(&bitmap, Direction::Right, Alignment::End, 0)
    })?;
    native_raster(out, "concat.raster", &concat)?;
    Ok(font.len())
}
fn load_compat(input: &[u8]) -> fontpixel::Result<CompatFont> {
    let mut font = CompatFont::default();
    font.load_bytes(input)?;
    Ok(font)
}
fn compat(input: &[u8], out: &Path, metrics: &mut BTreeMap<String, u128>) -> AnyResult<usize> {
    let font = once(metrics, "parse", || load_compat(black_box(input)))?;
    let bytes = once(metrics, "write_bdf", || {
        font.tobdf().map(String::into_bytes)
    })?;
    fs::write(out.join("compat.bdf"), bytes)?;
    let bytes = once(metrics, "write_pcf", || font.topcf(&Default::default()))?;
    fs::write(out.join("compat.pcf"), bytes)?;
    let bytes = once(metrics, "write_ttf", || font.tottf(&Default::default()))?;
    fs::write(out.join("compat.ttf"), bytes)?;
    let width = font
        .headers()
        .get("fbbx")
        .and_then(serde_json::Value::as_i64)
        .ok_or("missing fbbx")?;
    let options = serde_json::json!({"mode":0,"linelimit":80*width});
    let options = options.as_object().ok_or("render options")?;
    let text = "永".repeat(2000);
    let screen = once(metrics, "render_screen", || {
        font.draw(black_box(&text), options)
    })?;
    compat_raster(out, "screen.raster", &screen)?;
    drop(screen);
    let options = serde_json::json!({"mode":1});
    let atlas = once(metrics, "drawall_mode1", || {
        font.drawall(options.as_object().expect("object"))
    })?;
    compat_raster(out, "drawall.raster", &atlas)?;
    drop(atlas);
    let bitmap = tiny_compat()?;
    let crop = once(metrics, "crop", || {
        let mut out = bitmap.clone();
        out.crop(31, 31, 3, 5)?;
        Ok(out)
    })?;
    compat_raster(out, "crop.raster", &crop)?;
    let concat = once(metrics, "concat", || {
        let mut out = bitmap.clone();
        out.concat(&bitmap, 1, 1, 0)?;
        Ok(out)
    })?;
    compat_raster(out, "concat.raster", &concat)?;
    report(out, "compatibility", &font.report())?;
    Ok(font.length())
}
fn main() -> AnyResult<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        return Err(
            "phase6_timing native|compat|woff2|compat-default-atlas-probe INPUT OUTPUT_DIRECTORY"
                .into(),
        );
    }
    let mode = &args[1];
    let input = fs::read(&args[2])?;
    let out = Path::new(&args[3]);
    fs::create_dir_all(out)?;
    if out.read_dir()?.next().is_some() {
        return Err("output directory must be empty".into());
    }
    let mut metrics = BTreeMap::new();
    let glyphs = match mode.as_str() {
        "native" => native(&input, out, &mut metrics)?,
        "compat" => compat(&input, out, &mut metrics)?,
        "woff2" => {
            let options = fontpixel::woff::woff2::Options {
                quality: 11,
                compression_threads: 1,
                ..Default::default()
            };
            let encoded = once(&mut metrics, "woff2_encode", || {
                fontpixel::woff::woff2::encode(black_box(&input), &options, &Default::default())
            })?;
            report(out, "woff2", &encoded.report)?;
            fs::write(out.join("font.woff2"), encoded.bytes)?;
            0
        }
        "compat-default-atlas-probe" => {
            let font = load_compat(&input)?;
            match font.drawall(&Default::default()) {
                Ok(bitmap) => {
                    compat_raster(out, "drawall-default.raster", &bitmap)?;
                    println!(
                        "{}",
                        serde_json::json!({"status":"accepted","glyphs":font.length()})
                    );
                }
                Err(error) => println!(
                    "{}",
                    serde_json::json!({"status":"refused","error":error.to_string(),"glyphs":font.length()})
                ),
            }
            return Ok(());
        }
        _ => return Err("unknown mode".into()),
    };
    println!(
        "{}",
        serde_json::json!({"schema":2,"profile":"reconstructed-full-input-v2","engine":mode,"glyphs":glyphs,"input_bytes":input.len(),"metrics_ns":metrics,"warmup_calls":1,"measured_calls":1,"atlas_mode":1,"small_bitmap":"64x64,(13*x+7*y)%11<4; crop31x31@(3,5); concat right/end/gap0","scope":"resident inputs; one warmup then one call; result destruction/output materialization outside inner timer; conversion temporary model/read-report destruction included"})
    );
    Ok(())
}
