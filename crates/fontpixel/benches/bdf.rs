//! BDF Criterion estimates for resident source bytes, separate from wall-clock runs.
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use fontpixel::{
    ResourceLimits,
    formats::bdf::{self, BdfWriteOptions},
};
use std::{hint::black_box, path::PathBuf, time::Duration};
fn bdf_bench(c: &mut Criterion) {
    let path = std::env::var_os("FONTPIXEL_BDF_BENCH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf")
        });
    let bytes = std::fs::read(path)
        .expect("benchmark needs the supplied Tong SC sample or FONTPIXEL_BDF_BENCH");
    let limits = ResourceLimits::default();
    let options = BdfWriteOptions::default();
    let (font, _) = bdf::read(&bytes, &limits).unwrap();
    let mut group = c.benchmark_group("bdf");
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(2));
    group.bench_function("tong_sc_parse", |b| {
        b.iter_batched(
            || (),
            |()| bdf::read(black_box(&bytes), &limits).unwrap(),
            BatchSize::PerIteration,
        )
    });
    group.bench_function("tong_sc_write", |b| {
        b.iter_batched(
            || (),
            |()| bdf::write(black_box(&font), &options).unwrap(),
            BatchSize::PerIteration,
        )
    });
    group.finish();
}
criterion_group!(benches, bdf_bench);
criterion_main!(benches);
