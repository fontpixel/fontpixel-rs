# W1.1 core benchmark report

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
