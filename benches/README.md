# Benchmarks

W1.1 supplies Criterion in `crates/fontpixel/benches/bitmap.rs`, a reproducible
five-batch wall-clock executable `examples/bitmap_measure.rs`, and matching Python
measurements in `tools/bench_bitmap_python.py`. See `REPORT.md` for methods,
platform, committed raw data and limitations.

Phase 3 codec measurements use `examples/image_c_timing.rs` and
`examples/p1_timing.rs`. Both verify generated glyphs, warm up five times and
measure 200 reads/writes plus independent reads on 1/2/4 threads. Stop task-owned
builds, fuzz and heavy oracles first, then run:

```sh
source scripts/local-env.sh
cargo build --locked --release -p fontpixel --no-default-features \
  --features image,c,kbitx,pbf,hbf --example image_c_timing --example p1_timing
python3 benches/codec_timing.py \
  --binary "$CARGO_TARGET_DIR/release/examples/p1_timing" \
  --output "$FONTPIXEL_WORK_ROOT/p1-timing-new.json" \
  --environment-note 'Describe other active processes and machine conditions'
```

The runner performs five runs and refuses to overwrite evidence. Repeat for the
image/C example with a different output name. These Linux measurements include
binary identity, peak RSS, raw records and medians; they do not substitute for
full-font or whole-catalogue benchmarks. Final measurements and exact fixture
dimensions are in `reports/phase-3.md` and `reports/phase3-evidence/`.
