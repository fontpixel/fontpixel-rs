//! Reproducible integration/benchmark driver for the WOFF container APIs.
use fontpixel::woff::{Limits, woff1, woff2};
use std::{path::Path, time::Instant};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = std::env::args().collect::<Vec<_>>();
    let l = Limits::default();
    if a.len() < 4 {
        return Err("usage: woff_check encode|decode|bench FORMAT INPUT OUTPUT [QUALITY] [THREADS] [null] | batch QUALITY THREADS JOBS INPUT...".into());
    }
    if a[1] == "batch" {
        let o = woff2::Options {
            quality: a[2].parse()?,
            compression_threads: a[3].parse()?,
            ..Default::default()
        };
        let jobs = a[4].parse()?;
        let inputs = a[5..]
            .iter()
            .map(std::fs::read)
            .collect::<std::io::Result<Vec<_>>>()?;
        let inputs = inputs.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let mut times = Vec::new();
        let mut bytes = 0;
        for _ in 0..5 {
            let start = Instant::now();
            let out = woff2::encode_many(&inputs, &o, &l, jobs)?;
            times.push(start.elapsed().as_secs_f64());
            bytes = out.iter().map(|e| e.bytes.len()).sum();
        }
        println!(
            "{}",
            serde_json::json!({"seconds":times,"bytes":bytes,"jobs":jobs,"quality":o.quality,"threads":o.compression_threads})
        );
        return Ok(());
    }
    let data = std::fs::read(&a[3])?;
    if a[1] == "decode" {
        let d = if a[2] == "woff" {
            woff1::decode(&data, &l)?
        } else {
            woff2::decode(&data, &l)?
        };
        std::fs::write(a.get(4).ok_or("output")?, d.sfnt)?;
        eprint!("{}", d.report);
        return Ok(());
    }
    let quality = a
        .get(5)
        .map(|s| s.parse())
        .transpose()?
        .unwrap_or(if a[2] == "woff" { 9 } else { 11 });
    let threads = a.get(6).map(|s| s.parse()).transpose()?.unwrap_or(1);
    let transform = a.get(7).is_none_or(|s| s != "null");
    let o = woff2::Options {
        quality,
        compression_threads: threads,
        transform_glyf: transform,
        transform_hmtx: transform,
        ..Default::default()
    };
    let mut times = Vec::new();
    let runs = if a[1] == "bench" { 5 } else { 1 };
    let mut bytes = 0;
    for _ in 0..runs {
        let start = Instant::now();
        let e = if a[2] == "woff" {
            woff1::encode(
                &data,
                &woff1::Options {
                    compression_level: quality,
                    ..Default::default()
                },
                &l,
            )?
        } else {
            woff2::encode(&data, &o, &l)?
        };
        times.push(start.elapsed().as_secs_f64());
        bytes = e.bytes.len();
        eprint!("{}", e.report);
        if let Some(p) = a.get(4)
            && p != "-"
        {
            std::fs::write(Path::new(p), &e.bytes)?;
        }
    }
    println!(
        "{}",
        serde_json::json!({"seconds":times,"bytes":bytes,"quality":quality,"threads":threads,"transformed":transform})
    );
    Ok(())
}
