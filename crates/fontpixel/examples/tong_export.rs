//! Bounded multicore full Tong export, reusing parsed faces, plans and sfnt bytes.
//! Usage: tong_export BDF_DIRECTORY OUTPUT_DIRECTORY [JOBS=8] [QUALITY=11]
//!                   [COMPRESSION_THREADS=1] [VERSION=1.0] [DOT_REUSE=patterns]
use fontpixel::{
    FontSet, Input, ResourcePath, Variants,
    outline::Shape,
    variants::{DotReuse, Options, OutlineFlavor, ScriptVariant},
};
use std::{
    collections::{BTreeMap, HashSet},
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

/// Validate every destination before starting any output. Comparing open-file
/// identities also catches hard links and symlinks, including cross-format
/// destinations that refer to an input or another output.
fn check_output_aliases(inputs: &[(String, PathBuf)], out: &Path) -> AnyResult<()> {
    let sources = inputs
        .iter()
        .map(|(_, path)| same_file::Handle::from_path(path))
        .collect::<Result<HashSet<_>, _>>()?;
    let mut destinations = Vec::new();
    for (stem, _) in inputs {
        for kind in ["bdf", "pcf", "otb"] {
            destinations.push(out.join(kind).join(format!("{stem}.{kind}")));
        }
    }
    for size in ["Large", "Small"] {
        for spacing in ["", "Mono"] {
            for shape in ["dot", "square"] {
                for kind in ["ttf", "otf", "woff2"] {
                    destinations.push(
                        out.join(kind)
                            .join(format!("TongPixelSans{spacing}14{size}-{shape}.{kind}")),
                    );
                }
            }
        }
    }
    let mut outputs = HashSet::new();
    for path in destinations {
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return Err(format!("output is not a regular file: {}", path.display()).into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // A dangling output link could become an alias when an earlier
                // output is written, so it is not equivalent to an absent file.
                match fs::symlink_metadata(&path) {
                    Ok(_) => {
                        return Err(format!(
                            "output has an unresolved symlink: {}",
                            path.display()
                        )
                        .into());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
                continue;
            }
            Err(error) => return Err(error.into()),
        }
        let handle = same_file::Handle::from_path(&path)?;
        if sources.contains(&handle) {
            return Err(format!("output aliases a source BDF: {}", path.display()).into());
        }
        if !outputs.insert(handle) {
            return Err(format!("output aliases another output: {}", path.display()).into());
        }
    }
    Ok(())
}

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
        return Err("tong_export BDF_DIRECTORY OUTPUT_DIRECTORY [JOBS] [QUALITY] [COMPRESSION_THREADS] [VERSION] [DOT_REUSE]".into());
    }
    let input = Path::new(&args[1]);
    let out = Path::new(&args[2]);
    let cpus = std::thread::available_parallelism().map_or(1, usize::from);
    let jobs: usize = args
        .get(3)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(cpus.min(8));
    let quality: u32 = args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(11);
    let threads: usize = args.get(5).map(|s| s.parse()).transpose()?.unwrap_or(1);
    let version = args.get(6).map_or("1.0", String::as_str);
    let dot_reuse = match args.get(7).map_or("patterns", String::as_str) {
        "literal" => DotReuse::Literal,
        "rows" => DotReuse::Rows,
        "patterns" => DotReuse::Patterns,
        _ => return Err("dot reuse must be literal, rows or patterns".into()),
    };
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
    check_output_aliases(&inputs_to_read, out)?;
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
    // Fixed task order keeps scheduling reproducible; at eight jobs all fonts
    // run together, amortizing compressor startup without rereading sources.
    let tasks = [Shape::Dot, Shape::Square]
        .into_iter()
        .flat_map(|shape| (0..families.len()).map(move |family| (family, shape)))
        .collect::<Vec<_>>();
    let vector_start = Instant::now();
    let vector_reports = parallel(&tasks, jobs, |&(family, shape)| {
        let started = Instant::now();
        let shape_name = if shape == Shape::Dot { "dot" } else { "square" };
        let options = Options {
            shape, dot_size: if shape == Shape::Dot { 0.9 } else { 1. }, dot_reuse,
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            loop {
                let path = std::env::temp_dir().join(format!(
                    "fontpixel-tong-paths-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("{error}"),
                }
            }
        }
        fn input(&self) -> (String, PathBuf) {
            let stem = "TongPixelSans14Large-SC".to_owned();
            let path = self.0.join("bdf").join(format!("{stem}.bdf"));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"source BDF fixture").unwrap();
            (stem, path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn identical_input_and_output_is_rejected_without_modification() {
        let scratch = Scratch::new();
        let input = scratch.input();
        let error = check_output_aliases(std::slice::from_ref(&input), &scratch.0).unwrap_err();
        assert!(error.to_string().contains("aliases a source BDF"));
        assert_eq!(fs::read(&input.1).unwrap(), b"source BDF fixture");
    }

    #[test]
    fn separate_new_and_existing_outputs_are_allowed() {
        let scratch = Scratch::new();
        let input = scratch.input();
        let out = scratch.0.join("out");
        check_output_aliases(std::slice::from_ref(&input), &out).unwrap();
        fs::create_dir_all(out.join("bdf")).unwrap();
        fs::write(
            out.join("bdf").join(format!("{}.bdf", input.0)),
            b"old output",
        )
        .unwrap();
        check_output_aliases(std::slice::from_ref(&input), &out).unwrap();
        assert_eq!(fs::read(&input.1).unwrap(), b"source BDF fixture");
    }

    #[test]
    fn hard_link_from_any_output_to_any_input_is_rejected() {
        let scratch = Scratch::new();
        let input = scratch.input();
        let out = scratch.0.join("out");
        fs::create_dir_all(out.join("otf")).unwrap();
        fs::hard_link(&input.1, out.join("otf/TongPixelSans14Large-dot.otf")).unwrap();
        let error = check_output_aliases(std::slice::from_ref(&input), &out).unwrap_err();
        assert!(error.to_string().contains("aliases a source BDF"));
        assert_eq!(fs::read(&input.1).unwrap(), b"source BDF fixture");
    }

    #[test]
    fn outputs_must_not_alias_each_other() {
        let scratch = Scratch::new();
        let input = scratch.input();
        let out = scratch.0.join("out");
        fs::create_dir_all(out.join("bdf")).unwrap();
        fs::create_dir_all(out.join("pcf")).unwrap();
        let bdf = out.join("bdf").join(format!("{}.bdf", input.0));
        fs::write(&bdf, b"old output").unwrap();
        fs::hard_link(&bdf, out.join("pcf").join(format!("{}.pcf", input.0))).unwrap();
        let error = check_output_aliases(std::slice::from_ref(&input), &out).unwrap_err();
        assert!(error.to_string().contains("aliases another output"));
        assert_eq!(fs::read(&bdf).unwrap(), b"old output");
    }

    #[cfg(unix)]
    #[test]
    fn file_and_directory_symlink_aliases_are_rejected() {
        use std::os::unix::fs::symlink;
        let scratch = Scratch::new();
        let input = scratch.input();
        let out = scratch.0.join("out");
        fs::create_dir_all(out.join("bdf")).unwrap();
        symlink(&input.1, out.join("bdf").join(format!("{}.bdf", input.0))).unwrap();
        let error = check_output_aliases(std::slice::from_ref(&input), &out).unwrap_err();
        assert!(error.to_string().contains("aliases a source BDF"));
        let other = scratch.0.join("other");
        fs::create_dir(&other).unwrap();
        symlink(scratch.0.join("bdf"), other.join("bdf")).unwrap();
        let error = check_output_aliases(std::slice::from_ref(&input), &other).unwrap_err();
        assert!(error.to_string().contains("aliases a source BDF"));
        assert_eq!(fs::read(&input.1).unwrap(), b"source BDF fixture");
    }

    #[cfg(unix)]
    #[test]
    fn dangling_output_symlink_cannot_alias_a_future_output() {
        let scratch = Scratch::new();
        let input = scratch.input();
        let out = scratch.0.join("out");
        fs::create_dir_all(out.join("bdf")).unwrap();
        fs::create_dir_all(out.join("pcf")).unwrap();
        std::os::unix::fs::symlink(
            out.join("bdf").join(format!("{}.bdf", input.0)),
            out.join("pcf").join(format!("{}.pcf", input.0)),
        )
        .unwrap();
        let error = check_output_aliases(std::slice::from_ref(&input), &out).unwrap_err();
        assert!(error.to_string().contains("unresolved symlink"));
        assert_eq!(fs::read(&input.1).unwrap(), b"source BDF fixture");
    }
}
