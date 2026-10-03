//! Bounded multicore full Tong export, reusing parsed faces, plans and sfnt bytes.
//! Usage: tong_export BDF_DIRECTORY OUTPUT_DIRECTORY [JOBS=4] [QUALITY=11]
//!                   [COMPRESSION_THREADS=2] [VERSION=1.0]
use fontpixel::{
    FontSet, Input, ResourcePath, Variants,
    outline::Shape,
    variants::{Options, OutlineFlavor, ScriptVariant},
};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};
type AnyResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

fn parallel<'a, T: Sync, R: Send>(
    items: &'a [T],
    jobs: usize,
    f: impl Fn(&'a T) -> AnyResult<R> + Sync,
) -> AnyResult<Vec<R>> {
    let next = AtomicUsize::new(0);
    let slots = Mutex::new(
        (0..items.len())
            .map(|_| None)
            .collect::<Vec<Option<AnyResult<R>>>>(),
    );
    std::thread::scope(|scope| -> AnyResult<()> {
        let mut workers = Vec::new();
        for _ in 0..jobs.min(items.len()) {
            workers.push(scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= items.len() {
                        break;
                    }
                    let result = f(&items[i]);
                    match slots.lock() {
                        Ok(mut slots) => slots[i] = Some(result),
                        Err(_) => break,
                    }
                }
            }));
        }
        for worker in workers {
            worker.join().map_err(|_| "export worker panicked")?;
        }
        Ok(())
    })?;
    slots
        .into_inner()
        .map_err(|_| "export result lock poisoned")?
        .into_iter()
        .map(|r| {
            r.ok_or_else(|| "missing export result".into())
                .and_then(|r| r)
        })
        .collect()
}

struct InputFace {
    stem: String,
    path: PathBuf,
    font: FontSet,
}
struct Family {
    stem: String,
    name: String,
    variants: Variants,
}

fn write_bitmap(face: &InputFace, out: &Path) -> AnyResult<serde_json::Value> {
    fs::copy(
        &face.path,
        out.join("bdf").join(format!("{}.bdf", face.stem)),
    )?;
    let mut results = Vec::new();
    for format in ["pcf", "otb"] {
        let name = format!("{}.{format}", face.stem);
        let written = fontpixel::write(
            &face.font,
            format,
            &fontpixel::WriteOptions::new(ResourcePath::new(&name)?),
        )?;
        let mut reports = BTreeMap::<String, usize>::new();
        for entry in &written.report.entries {
            *reports.entry(entry.code.clone()).or_default() += 1;
        }
        let bytes = written.output.into_single()?;
        fs::write(out.join(format).join(&name), &bytes)?;
        results.push(serde_json::json!({"file":name,"bytes":bytes.len(),"report_codes":reports}));
    }
    Ok(serde_json::json!({"source":face.stem,"outputs":results}))
}

fn main() -> AnyResult<()> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 3 {
        return Err("tong_export BDF_DIRECTORY OUTPUT_DIRECTORY [JOBS] [QUALITY] [COMPRESSION_THREADS] [VERSION]".into());
    }
    let input = Path::new(&args[1]);
    let out = Path::new(&args[2]);
    let cpus = std::thread::available_parallelism().map_or(1, usize::from);
    let jobs: usize = args
        .get(3)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(cpus.min(4));
    let quality: u32 = args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(11);
    let threads: usize = args.get(5).map(|s| s.parse()).transpose()?.unwrap_or(2);
    let version = args.get(6).map_or("1.0", String::as_str);
    if !(1..=8).contains(&jobs)
        || !(1..=8).contains(&threads)
        || jobs * threads > 16
        || quality > 11
    {
        return Err(
            "jobs and compression threads must be 1..=8, their product <=16, quality <=11".into(),
        );
    }
    for kind in ["bdf", "pcf", "otb", "ttf", "otf", "woff2"] {
        fs::create_dir_all(out.join(kind))?;
    }
    let start = Instant::now();
    let mut inputs_to_read = Vec::new();
    for size in ["Large", "Small"] {
        for spacing in ["", "Mono"] {
            for region in ["SC", "TC", "JP", "KR", "Latin"] {
                let stem = format!("TongPixelSans{spacing}14{size}-{region}");
                let path = input.join(format!("{stem}.bdf"));
                inputs_to_read.push((stem, path));
            }
        }
    }
    let faces = parallel(&inputs_to_read, jobs, |(stem, path)| {
        let read_options = fontpixel::ReadOptions {
            format: Some("bdf".into()),
            ..Default::default()
        };
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(read_options.limits.max_input_bytes as u64 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > read_options.limits.max_input_bytes {
            return Err("source BDF exceeds input byte budget".into());
        }
        let loaded = fontpixel::registry().read(
            &Input::new(&bytes).with_name(&path.to_string_lossy()),
            &read_options,
        )?;
        Ok(InputFace {
            stem: stem.clone(),
            path: path.clone(),
            font: loaded.fonts,
        })
    })?;
    let read_seconds = start.elapsed().as_secs_f64();
    let bitmap_start = Instant::now();
    let bitmap_reports = parallel(&faces, jobs, |face| write_bitmap(face, out))?;
    let bitmap_seconds = bitmap_start.elapsed().as_secs_f64();
    let plan_start = Instant::now();
    let mut inputs = faces
        .into_iter()
        .map(|face| (face.stem, face.font))
        .collect::<BTreeMap<_, _>>();
    let mut families = Vec::new();
    for size in ["Large", "Small"] {
        for spacing in ["", "Mono"] {
            let stem = format!("TongPixelSans{spacing}14{size}");
            let mut take = |region: &str| -> AnyResult<_> {
                Ok(inputs
                    .remove(&format!("{stem}-{region}"))
                    .ok_or("missing source face")?
                    .into_single()?)
            };
            let mut variants = Variants::new(take("SC")?);
            let tc = take("TC")?;
            variants.add_language("zh-Hant", tc.clone())?;
            variants.add_language("zh-HK", tc)?;
            variants.add_language("ja", take("JP")?)?;
            variants.add_language("ko", take("KR")?)?;
            variants.scripts.push(ScriptVariant {
                scripts: vec![*b"latn", *b"grek", *b"cyrl"],
                font: take("Latin")?,
            });
            let name = format!(
                "Tong Pixel Sans{} 14 {size}",
                if spacing.is_empty() { "" } else { " Mono" }
            );
            families.push(Family {
                stem,
                name,
                variants,
            });
        }
    }
    let prepared = parallel(&families, jobs, |family| {
        Ok(family.variants.prepare(&Default::default())?)
    })?;
    let plan_seconds = plan_start.elapsed().as_secs_f64();
    // The historically expensive dot jobs start first, reducing tail idle time.
    let tasks = [Shape::Dot, Shape::Square]
        .into_iter()
        .flat_map(|shape| (0..families.len()).map(move |family| (family, shape)))
        .collect::<Vec<_>>();
    let vector_start = Instant::now();
    let vector_reports = parallel(&tasks, jobs, |&(family, shape)| {
        let started = Instant::now();
        let shape_name = if shape == Shape::Dot { "dot" } else { "square" };
        let options = Options {
            shape, dot_size: if shape == Shape::Dot { 0.9 } else { 1. },
            family: Some(families[family].name.clone()), version: version.into(), vendor: *b"TONG",
            copyright: Some("Tong Pixel Sans. Derived from Source Han Sans, Source Sans 3, Source Code Pro (Adobe), the Noto fonts (Google), Plangothic P1; Han bitmaps partly after TUMBLED (TsFreddie).".into()),
            license: Some("This Font Software is licensed under the SIL Open Font License, Version 1.1.".into()),
            license_url: Some("https://openfontlicense.org".into()), ..Default::default()
        };
        let ttf = prepared[family].encode(&options)?;
        let otf = prepared[family].encode(&Options {
            flavor: OutlineFlavor::Cff,
            ..options
        })?;
        let stem = format!("{}-{shape_name}", families[family].stem);
        fs::write(out.join("ttf").join(format!("{stem}.ttf")), &ttf.bytes)?;
        fs::write(out.join("otf").join(format!("{stem}.otf")), &otf.bytes)?;
        let sfnt_seconds = started.elapsed().as_secs_f64();
        let compress_start = Instant::now();
        let input = if shape == Shape::Dot {
            &ttf.bytes
        } else {
            &otf.bytes
        };
        let web = fontpixel::woff::woff2::encode(
            input,
            &fontpixel::woff::woff2::Options {
                quality,
                compression_threads: threads,
                ..Default::default()
            },
            &Default::default(),
        )?;
        fs::write(out.join("woff2").join(format!("{stem}.woff2")), &web.bytes)?;
        Ok(
            serde_json::json!({"font":stem,"ttf_bytes":ttf.bytes.len(),"otf_bytes":otf.bytes.len(),
            "woff2_bytes":web.bytes.len(),"sfnt_seconds":sfnt_seconds,
            "compression_seconds":compress_start.elapsed().as_secs_f64(),"wall_seconds":started.elapsed().as_secs_f64()}),
        )
    })?;
    let result = serde_json::json!({"wall_seconds":start.elapsed().as_secs_f64(),"read_seconds":read_seconds,
        "bitmap_seconds":bitmap_seconds,"plan_seconds":plan_seconds,"vector_seconds":vector_start.elapsed().as_secs_f64(),
        "jobs":jobs,"quality":quality,"compression_threads":threads,"available_cpus":cpus,
        "bitmaps":bitmap_reports,"vectors":vector_reports});
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
