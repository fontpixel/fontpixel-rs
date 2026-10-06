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

Phase 4 uses `p2_timing` for sixteen formats plus CPX: 128 original 8×16 glyphs,
five warmups, 100 reads/writes and 400 independent reads on 1/2/4 threads per
process. Before timing, every original raster and code mapping is checked.
The separate `w44_timing` example measures the complete 38,653-glyph Tong SC
font in BMFont, SFD and Kbits. Its five-process runner measures one read/write
per process, without explicit warmups; BDF parsing stays outside the timers.

```sh
source scripts/local-env.sh
cargo build --locked --release -p fontpixel --all-features \
  --example p2_timing --example w44_timing
python3 benches/codec_timing.py \
  --binary "$CARGO_TARGET_DIR/release/examples/p2_timing" \
  --output "$FONTPIXEL_WORK_ROOT/p2-timing-new.json" \
  --environment-note 'Describe other active processes and machine conditions'
python3 benches/editor_timing.py \
  --binary "$CARGO_TARGET_DIR/release/examples/w44_timing" \
  --font ../samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf \
  --output "$FONTPIXEL_WORK_ROOT/editor-large-timing-new.json" \
  --environment-note 'Describe other active processes and machine conditions'
```

Run the measurements sequentially with task-owned builds, fuzz and oracle jobs
stopped. Exact source/binary identities, raw runs, min/max values and ordinary
peak process RSS are in `reports/phase4-evidence/`; medians and limits are in
`reports/phase-4.md`. Process RSS includes retained input/model/output data;
these measurements do not substitute for final whole-catalogue benchmarks.

Phase 7 compares the Python bindings (`fontpixel.compat.bdfparser` and the native
API) with pure-Python bdfparser, call class by call class, in fresh processes:
`phase7_python.py`, documented in `phase7-README.md`, with results and analysis in
`PHASE7-REPORT.md` and raw data in `results/phase7-python-run*.json`.
