# Phase 6 recovery status

**The restored phase 6 implementation has not been compiled or measured.**
The earlier working environment was replaced on 2026-10-04. Its native binaries,
raw timing logs and generated output archives are unavailable. Reconstructed
source and historical summaries cannot validate this restored implementation.

The benchmark recorder, independent auditor and corrected catalogue task adapter
are restored as source. Standard-library contract tests cover unique dotted case IDs,
five observations, immutable identities, archive members, independent medians,
full report retention and explicit application metadata. These are tooling tests,
not the PLAN §10 performance gate or codec correctness acceptance.

The latest historical native medians were direct BDF parse 113.54 ms, public read
98.89 ms, PCF conversion 194.90 ms, OTB 249.72 ms, YAFF 278.59 ms and square TTF
793.06 ms. These are unavailable-evidence historical observations only. OTB, YAFF
and direct parse had not passed their targets. The complete comparison suite,
including the actual catalogue with corrected metadata, remained unfinished.

Current contracts, dependencies and reproduction steps are in
[phase6-README.md](phase6-README.md). Complete native and independent-oracle
validation followed by five fresh processes per case is required after the
restored source builds. No older result is labelled an acceptance of this code.

Fresh external oracle preparation on 2026-10-04 has now passed independently of
Rust. All nine handoff-pinned Python packages and lock-pinned esbuild 0.27.7 are
installed in local-work. Python and Node produce identical seven-artifact output
for the complete 38,653-glyph SC input. FreeType verifies every encoded PCF/TTF
pixel and advance; OTS, fontTools, HarfBuzz, Brotli and monobit pass the documented
valid-font fixtures. The original catalogue creates all 181 downloads, with all
105 sfnt application names/copyright/version/UPM checked and all 8 ZIPs hashed.
These are external-only correctness fixtures, not formal timing or native parity.
The [setup record](phase6-oracle-setup/README.md),
[complete evidence](phase6-oracle-setup/oracle-setup-evidence.zip) and
[archive member SHA index](phase6-oracle-setup/oracle-setup-evidence.index.json)
retain exact versions, source/input identities, commands, logs and readback scope.
The absent production opf_vector extension is explicitly replaced by its supplied
fontTools oracle; Bun and native Rust validation remain unperformed.

The historical reports below retain their original source, machine and scope.

---

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
