# Phase 6 performance report (PLAN §10), 2026-10-05

Formal five-run measurement of the final phase-6 source (freeze v3). Every case
ran in five fresh processes; values are medians. The recorder and the independent
auditor are `scripts/phase6_perf.py` and `scripts/phase6_perf_audit.py`; case
contracts are in [phase6-README.md](phase6-README.md). The audit re-verified 2,893
frozen input/binary/source files, 110 invocations, 220 raw streams and 9,215
archived output members, and recomputed all 22 median rows independently.

**Machine:** 12th Gen Intel Core i7-12700H, 20 logical CPUs (14 cores), 62 GB,
Linux 7.0.0. Local desktop workstation: no build, test, fuzz or validation jobs
ran during measurement, but the desktop session (file indexer, browser) stayed
open. Rust 1.99.0 release builds; Python 3.12.13 and Node with the pinned oracles.
Local evidence: `../local-work/phase6-local-20261004/perf-final/run-01/` (gates
049 measurement, 051 audit).

## PLAN §10 targets

| Task | Target | Median | Result |
| --- | --- | --- | --- |
| Parse `TongPixelSans14Large-SC.bdf` (5.7 MB, 38,653 glyphs), single thread | ≤ 100 ms | 53.3 ms (public `read` 49.2 ms) | pass |
| Same file → PCF | ≤ 200 ms | 88.2 ms | pass |
| Same file → OTB | ≤ 200 ms | 119.9 ms | pass |
| Same file → yaff | ≤ 200 ms | 144.9 ms | pass |
| Same file → square TTF (with `glyf`) | ≤ 1 s | 449.4 ms in-process; 653.6 ms CLI pipeline | pass |
| Full Tong export (8 vector families × TTF/OTF/WOFF2 with `locl`, 20 PCF, 20 OTB), default settings | ≤ 40 s | 32.2 s (`fontpixel batch`, default options) | pass |
| Unifont 57,086 + Tong Latin merge → 57,106-glyph BDF with full report | < 1 s | 0.36 s whole process | pass |
| Catalogue: all 34 sources → 173 downloads + 8 family ZIPs | report | 5.50 s native vs 43.54 s original Python | 7.9× faster |

Conversion times include parsing and destroying the temporary model and read
report; returned bytes are written outside the inner timer.

## Full Tong export variants

| Profile | Median wall | 8 WOFF2 total | Note |
| --- | --- | --- | --- |
| `fontpixel batch`, defaults (quality 11, 2 WOFF2 threads, 4 workers) | 32.2 s | 8,753,280 B | the default path; ≤ 40 s target |
| original `tools/export-rs` | 66.3 s | 7,460,280 B | prepare 28.0 s, vectors 38.4 s |
| native API example, default (quality 11, 1 thread) | 79.8 s | 8,781,608 B | bitmap 49.6 s + vector 20.6 s, families in sequence |
| native API, WOFF2 quality 8 / 5 | 62.1 s / 57.2 s | 10,252,448 / 11,147,708 B | vector stage 2.5 s / 2.0 s |
| native API, serial (1 job) | 208.0 s | 8,781,608 B | single-threaded baseline |
| native API, 2 jobs × 4 threads | 101.7 s | 8,753,724 B | |
| native API, dot reuse literal / rows | 119.6 s / 88.3 s | 7,634,592 / 7,758,360 B | smaller WOFF2, slower than the default pattern reuse |

The dot-reuse setting trades size for time: literal outlines compress to 13% smaller
WOFF2 than the default pattern reuse but take 1.5× as long. export-rs WOFF2 output
is 15% smaller than the default.

The API example builds the families in sequence. The batch CLI schedules all 64
exports across workers, which is why it is faster than the same work through the
example. Default batch and default API outputs are byte-identical for 76 of 84
artifacts (correctness gates 100/101). The 8 WOFF2 files differ because the CLI
compresses with 2 Brotli threads by default and the API example with 1. All 8
decompress (fontTools) to byte-identical sfnt fonts; only the Brotli stream differs.

## Single operations versus bdfparser (Python) and bdfparser-js (Node)

Same 38,653-glyph file. Rust columns are in-process medians; the compat column is
the bdfparser-compatible Rust API.

| Operation | Rust native | Rust compat | Python bdfparser | bdfparser-js |
| --- | --- | --- | --- | --- |
| Parse | 53.3 ms | 124.1 ms | 437.4 ms | 244.5 ms |
| Write BDF | 36.5 ms | 77.8 ms | 58.1 ms | 48.0 ms |
| Write PCF | 32.4 ms | 103.5 ms | 436.6 ms | 569.1 ms |
| Write TTF | 401.1 ms | 1,094.0 ms | 9,258.4 ms | 1,223.0 ms |
| Render a 2,000-character line | 0.8 ms | 75.8 ms | 52.6 ms | 11.4 ms |
| `drawall` (mode 1) | 17.4 ms | 117.6 ms | 9,423.0 ms | 2,583.1 ms |
| Crop a 64×64 bitmap to 31×31 | 0.55 µs | 0.37 µs | 17.6 µs | 11.2 µs |
| Concat two 64×64 bitmaps | 0.75 µs | 1.04 µs | 69.0 µs | 50.3 µs |

The compat layer keeps bdfparser's row-string semantics, so it is slower than the
native API throughout. Against the originals it is faster at parsing, PCF, TTF and
`drawall`, but slower at writing BDF than both (77.8 ms vs 58.1/48.0 ms) and slower
at rendering a text line than both (75.8 ms vs 52.6/11.4 ms). These single-call
numbers exclude any binding overhead and feed the phase 7/8 decision on whether
bindings are worth using as an accelerator.

## fontTools comparisons

| Task | Rust | fontTools | Ratio |
| --- | --- | --- | --- |
| Full SC → square TTF (Tong `python_reference.py`) | 0.75 s | 12.82 s | 17× |
| WOFF2 of the same square sfnt (quality 11, 1 worker) | 9.58 s encode | 25.33 s encode | 2.6× |
| WOFF2 of the same Latin dot sfnt | 1.02 s encode | 1.66 s encode | 1.6× |

WOFF2 sizes and the acceleration options are recorded in the W2.5 section below
and in the archived outputs. The catalogue's production `opf_vector` extension
was not supplied, so its vector side uses the original fontTools fallback.

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
