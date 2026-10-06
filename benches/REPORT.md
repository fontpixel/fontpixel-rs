# Phase 6 performance report (PLAN §10), final strict source

Two formal five-run measurements exist for phase 6. Each case runs in five fresh processes and
values are medians. The recorder is `scripts/phase6_perf.py` and the independent auditor is
`scripts/phase6_perf_audit.py`; case contracts are in [phase6-README.md](phase6-README.md).

- **v3** (2026-10-05 morning, source of the PLAN-text delivery `6297998`). Measured in a quiet
  window: 1-minute load about 1–2. Gates 049/051.
- **v5** (2026-10-06, final strict source `8cd6d66`, with the full codec ledgers). Measured at
  CPU weight 10000 while the desktop and other user agents kept running: load median 5.2, peak 17.
  Gates 078/079. The audit re-verified 2,918 frozen files, 110 invocations, 220 raw streams and
  9,215 archived output members, and recomputed all 22 medians.

The v5 run was slowed by outside load. The original programs, which did not change, ran about
1.33× slower than in the quiet v3 window (export-rs 88.4 s against 66.3 s; the Python catalogue
48.0 s against 43.5 s). An interleaved A/B run therefore compares v3 and v5 directly under
identical conditions: gate 080, `../local-work/phase6-local-20261004/perf-ab/ab.py`, five rounds
alternating A and B, load 1.7–7.1.

**Machine:** 12th Gen Intel Core i7-12700H, 20 logical CPUs (14 cores), 62 GB, Linux 7.0.0;
Rust 1.99.0 release builds with all features; Python 3.12.13 and Node with the pinned oracles.

## PLAN §10 targets

| Task | Target | v3 quiet | v5 under load | A/B v3 → v5 (same load) | Result |
| --- | --- | --- | --- | --- | --- |
| Parse full SC BDF (5.7 MB, 38,653 glyphs), single thread | ≤ 100 ms | 53.3 ms | 57.2 ms | 59.5 → 59.9 ms | pass |
| → PCF | ≤ 200 ms | 88.2 ms | 94.5 ms | 93.1 → 93.4 ms | pass |
| → OTB | ≤ 200 ms | 119.9 ms | 130.3 ms | 123.1 → 134.7 ms (+9 %) | pass |
| → yaff | ≤ 200 ms | 144.9 ms | 150.3 ms | 145.3 → 149.7 ms | pass |
| → square TTF (with `glyf`) | ≤ 1 s | 449.4 ms; CLI 653.6 ms | 500.1 ms; CLI 597.1 ms | 490.7 → 497.8 ms | pass |
| Full Tong export (8 vector families × TTF/OTF/WOFF2 with `locl`, 20 PCF, 20 OTB), `fontpixel batch` defaults | ≤ 40 s | 32.2 s | 43.7 s | 30.1 → 30.6 s | pass on the A/B and quiet runs; the 43.7 s formal v5 median reflects outside load |
| Unifont 57,086 + Tong Latin merge → 57,106-glyph BDF with full report | < 1 s | 0.36 s | 0.36 s | 0.40 → 0.44 s | pass |
| Catalogue: 34 sources → 173 downloads + 8 ZIPs | report | 5.50 s vs Python 43.54 s | 5.74 s vs Python 48.04 s | — | 7.9–8.4× faster |

Conversion times include parsing and destroying the temporary model and read report; returned
bytes are written outside the inner timer. The strict ledgers cost about 9 % on BDF→OTB (embedded
writer accounting) and nothing measurable elsewhere.

## Full Tong export variants (v5 formal)

| Profile | Median wall | 8 WOFF2 total | Note |
| --- | --- | --- | --- |
| `fontpixel batch`, defaults (quality 11, 2 WOFF2 threads, 4 workers) | 43.7 s (A/B 30.6 s) | 8,753,280 B | default path |
| original `tools/export-rs` | 88.4 s (v3 quiet 66.3 s) | 7,457,432 B | prepare + vectors |
| native API example, default (quality 11, 1 thread) | 111.2 s | 8,781,608 B | families in sequence |
| native API, WOFF2 quality 8 / 5 | 68.7 s / 77.1 s | 10,252,448 / 11,147,708 B | |
| native API, serial (1 job) | 265.2 s | 8,781,608 B | single-threaded baseline |
| native API, 2 jobs × 4 WOFF2 threads | 125.5 s | 8,753,724 B | needs an explicit 400 MiB per-call WOFF2 budget (minimum 385.75 MiB): four concurrent quality-11 Brotli workers exceed the 256 MiB default |
| native API, dot reuse literal / rows | 149.1 s / 122.1 s | 7,634,592 / 7,758,360 B | smaller WOFF2, slower |

Default batch and default API outputs are byte-identical for 76 of 84 artifacts. The 8 WOFF2 files
differ only in the Brotli stream (2 threads against 1) and decompress to identical sfnt fonts.
literal outlines give 13 % smaller WOFF2 than the default pattern reuse at 1.3–1.5× the time.

## Single operations versus bdfparser (Python) and bdfparser-js (Node), v5 formal

| Operation | Rust native | Rust compat | Python bdfparser | bdfparser-js |
| --- | --- | --- | --- | --- |
| Parse | 57.2 ms | 136.1 ms | 497.8 ms | 289.8 ms |
| Write BDF | 39.8 ms | 89.2 ms | 63.4 ms | 54.2 ms |
| Write PCF | 35.2 ms | 110.0 ms | 509.6 ms | 691.1 ms |
| Write TTF | 452.2 ms | 1,408.5 ms | 12,002.9 ms | 1,387.4 ms |
| Render a 2,000-character line | 0.9 ms | 91.2 ms | 57.1 ms | 12.2 ms |
| `drawall` (mode 1) | 18.5 ms | 135.1 ms | 10,148.8 ms | 2,993.8 ms |
| Crop a 64×64 bitmap to 31×31 | 0.47 µs | 0.48 µs | 21.9 µs | 8.7 µs |
| Concat two 64×64 bitmaps | 0.78 µs | 1.20 µs | 67.7 µs | 87.8 µs |

The compat layer keeps bdfparser's row-string semantics, so it is slower than the native API
throughout. Against the originals it is faster at parsing, PCF, TTF (versus Python) and
`drawall`, but slower than both at writing BDF and at rendering a text line, and about equal to
bdfparser-js at writing TTF. These single-call numbers exclude any binding overhead.

## fontTools comparisons (v5 formal)

| Task | Rust | fontTools | Ratio |
| --- | --- | --- | --- |
| Full SC → square TTF (Tong `python_reference.py`) | 0.70 s | 13.38 s | 19× |
| WOFF2 of the same square sfnt (quality 11, 1 worker) | 10.04 s encode | 25.94 s encode | 2.6× |
| WOFF2 of the same Latin dot sfnt | 1.03 s encode | 1.70 s encode | 1.6× |

The catalogue's production `opf_vector` extension was not supplied, so its vector side uses the
original fontTools fallback.

# W1.1 core benchmark report

Later-stage complete-family measurements are in [PHASE2-REPORT.md](PHASE2-REPORT.md).
Phase 5 native-profile measurements are in [PHASE5-REPORT.md](PHASE5-REPORT.md).
The historical W1.1 measurements below retain their original scope; complete
phase 6 Tong/catalogue/merge targets remain pending.

Measured 2026-10-02 in the assigned shared Linux cloud workspace. These are core
raster/index operations, not font decoding, a layout engine or language bindings.
No PLAN §10 BDF/PCF/OTB/TTF/WOFF2 or whole-export target is claimed here.

## Platform and method

- CPU reported by `/proc/cpuinfo`: AMD EPYC 9V74 80-Core Processor, ~2596 MHz
- The guest reports **9 logical CPUs** (`getconf _NPROCESSORS_ONLN`); 80-core is
  the processor model name, not the guest's usable CPU count
- `/proc/meminfo` MemTotal: 10,206,508 KiB (~9.73 GiB)
- x86_64 Linux 6.18.44; shared virtualized host, no CPU affinity/pinning
- `/sys` and cgroup quota files are unavailable; host quotas/frequency policy
  cannot be verified. `lscpu` fails on the absent sysfs, so `/proc`/getconf are used
- Rust 1.99.0, LLVM 23.1.1, normal Cargo release profile; no native-CPU flags
- CPython 3.12.14, supplied pure-Python bdfparser `bitmap.py` (MIT, source hash
  recorded in THIRD_PARTY_NOTICES.md)
- Single-threaded operations, deterministic ink `(x*13+y*7)%11<4`; source creation
  and font-table construction are outside timed regions
- Rust `Instant` and Python `perf_counter_ns` measure elapsed wall time. Each row
  is the **median of five batch means**, with iteration counts and every sample
  retained in the linked raw JSON. A warmup runs before timing
- Immutable Rust result operations include result allocation/drop. Python clones
  before its mutating operation when needed to keep the same source semantics
- Screen paste uses an already allocated 1120×350 destination and 2000 individual
  14×14 pastes, reusing destination across batches in both languages. It measures
  the core composition API, not Font.draw, shaping, missing-glyph handling or I/O

## Five-batch wall-clock medians

| Operation | Rust | Python reference | Python/Rust |
|---|---:|---:|---:|
| Crop 64×64 to 31×31 at (3,5) | 0.371 µs | 6.404 µs | 17.3× |
| Concatenate two 64×64 rasters | 0.904 µs | 37.950 µs | 42.0× |
| Concatenate 80 × 14×14 glyphs | 12.744 µs | 284.819 µs | 22.3× |
| Paste 80×25 glyph screen | 331.453 µs | 240.820 ms | 726.6× |
| Crop 4096×1024 to 4000×1000, unaligned | 49.446 µs | not measured | — |
| Overlay 4096×1024 at (3,-2) | 47.639 µs | not measured | — |
| Bold 64×64 by 8 pixels | 3.409 µs | 1.632 ms | 478.8× |
| Clockwise rotation 64×64 | 2.711 µs | 162.433 µs | 59.9× |
| Lookup among 38,653 indexed codes | 0.105 µs | not measured | — |

The ~0.33 ms screen result falls in the intended core microsecond-to-millisecond
range, but does **not** establish W1.4's complete renderer or any font-codec target.
Large-raster samples vary on this shared host; use raw sample ranges, not a single
number as a universal guarantee. Pure Rust/Python ratios do not include FFI or
WASM boundary costs and must not be extrapolated to binding small calls.

## Criterion validation

Criterion 0.8.2 ran every raster workload with 20 samples, 200 ms warmup and 1 s
measurement target. It provides independent statistical estimates and confidence
intervals in `bitmap-criterion.json`; its estimator is different from the five
wall-clock batch method above. No CPU-native or unsafe project code is used.

Byte-kernel profiling exposed per-byte alignment/mode overhead in the initial
implementation. Full-byte runs now select alignment/operation once and allow
LLVM vectorization, while partial edges keep masks. Differential tests were rerun
after this change. The final 4096-pixel unaligned operations are tens of
microseconds; small glyph composition still correctly handles partial-byte edges.

## Reproduce

From the repository root with the pinned Rust toolchain:

```sh
cargo run --locked --release -p fontpixel --example bitmap_measure --no-default-features > benches/bitmap-wallclock.json
cargo bench --locked -p fontpixel --bench bitmap --no-default-features -- --noplot
PYTHONDONTWRITEBYTECODE=1 python3 tools/bench_bitmap_python.py --reference ../reference/bdfparser > benches/bitmap-python-wallclock.json
PYTHONDONTWRITEBYTECODE=1 python3 tools/gen_bitmap_cases.py --reference ../reference/bdfparser --check
```

`tools/bench_bitmap_python.py` needs the supplied read-only reference; standard
Cargo tests/benchmarks do not. No fonts or downloaded third-party source are copied
for these synthetic workloads. Criterion is dev-only with defaults disabled;
`cargo tree -p fontpixel --no-default-features --edges normal` has no dependencies.

## Raw data

- [Rust five batches](bitmap-wallclock.json)
- [Python five batches](bitmap-python-wallclock.json)
- [Criterion estimates](bitmap-criterion.json)

## Deferred comparisons

BDF/PCF parsing/writing and default full parity are W1.2–W1.5; actual rendering is
W1.4; TTF/OTB/vector/WOFF/WOFF2 and Tong's complete export are later stages. The
Node/bdfparser-js comparison, fontTools, reference export-rs, bindings and catalogue
batch conversion will be added when those respective paths exist. This report
neither replaces nor reduces the final PLAN §10 performance comparison scope.

## W1.2 codec measurements

Actual BDF parse/write benchmarks and Python/Node comparisons are now available
in [BDF-REPORT.md](BDF-REPORT.md). The W1.1 measurements above remain historical
core-only results; they are not retroactively claimed as font-codec measurements.

## W1.3 PCF codec measurements

[PCF-REPORT.md](PCF-REPORT.md) records actual PCF read/write and end-to-end
BDF-to-PCF measurements against Python and Node. The final five-sample BDF parse
and BDF-to-PCF medians are 85.623 ms and 135.772 ms; both corresponding PLAN §10
goals pass in the documented run. The preliminary contended run is retained too.

## W1.4 text-rendering measurements

[RENDER-REPORT.md](RENDER-REPORT.md) records complete resident Font.draw and drawall
against Python/Node, alongside Criterion hot paths. The real 2,000-character
screen median is 1.006 ms; the 38,653-code atlas median is 29.189 ms. These are
actual layout/render operations, extending rather than replacing W1.1's paste test.

## W2.5: WOFF1/WOFF2 containers and WOFF2 acceleration

See [WOFF-REPORT.md](WOFF-REPORT.md) for 29 five-sample quality/thread/batch and
same-input independent-encoder comparisons, raw samples, byte counts, restart
segments and limits. See `reports/w2.5.md` for correctness, fuzzing, decisions and
scope. This does not claim the full phase-2 export pipeline is complete.
