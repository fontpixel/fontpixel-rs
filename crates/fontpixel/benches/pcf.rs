//! PCF criterion measurements use real resident Tong SC source/model data.
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use fontpixel::{
    ResourceLimits,
    formats::{
        bdf,
        pcf::{self, PcfWriteOptions},
    },
};
use std::{hint::black_box, path::PathBuf, time::Duration};
fn bench(c: &mut Criterion) {
    let path = std::env::var_os("FONTPIXEL_PCF_BENCH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf")
        });
    let bytes = std::fs::read(path).unwrap();
    let limits = ResourceLimits::default();
    let options = PcfWriteOptions::default();
    let (font, _) = bdf::read(&bytes, &limits).unwrap();
    let wire = pcf::write(&font, &options).unwrap().0;
    let mut g = c.benchmark_group("pcf");
    g.sample_size(10).measurement_time(Duration::from_secs(2));
    g.bench_function("tong_sc_write", |b| {
        b.iter_batched(
            || (),
            |()| pcf::write(black_box(&font), &options).unwrap(),
            BatchSize::PerIteration,
        )
    });
    g.bench_function("tong_sc_read", |b| {
        b.iter_batched(
            || (),
            |()| pcf::read(black_box(&wire), &limits).unwrap(),
            BatchSize::PerIteration,
        )
    });
    g.bench_function("tong_sc_bdf_to_pcf", |b| {
        b.iter_batched(
            || (),
            |()| {
                let (f, _) = bdf::read(black_box(&bytes), &limits).unwrap();
                pcf::write(&f, &options).unwrap()
            },
            BatchSize::PerIteration,
        )
    });
    g.finish();
}
criterion_group!(benches, bench);
criterion_main!(benches);
