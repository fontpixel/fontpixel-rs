# W1.2 BDF benchmark report

Measured 2026-10-02 after the final fuzz process stopped, in the same shared Linux
workspace as W1.1. This report covers actual BDF parsing/serialization, not PCF,
OTB, TTF, rendering, bindings or the whole export pipeline.

## Platform and method

- AMD EPYC 9V74 80-Core Processor; guest exposes **9 logical CPUs**
- Memory: 10,206,508 KiB (~9.73 GiB); Linux x86_64, shared virtualized host
- CPU affinity/frequency/quota is not controlled. `lscpu` cannot read absent
  sysfs; the CPU/memory facts come from `/proc` and `getconf`
- Rust 1.99.0, normal optimized release profile, no target-cpu-native flags
- CPython 3.12.14; supplied pure-Python bdfparser 3.0.0 reference
- Node 24.19.0; supplied bdfparser-js TypeScript compiled unchanged by esbuild
  0.28.2 to a temporary ESM module. No build files enter the reference tree
- Resident source bytes, one untimed warmup, five independent single calls,
  **median of five wall-clock samples**, single-threaded. No I/O or process startup
  is timed. Each output is consumed; result drop/GC is outside the timed interval
- Python and Node collect garbage before each sample. Serialization includes
  UTF-8 encoding, so all three return wire bytes. Rust eagerly constructs compact
  pixels and source provenance; Python/JS retain hex strings and draw lazily
- Input: `TongPixelSans14Large-SC.bdf`, **5,746,021 bytes, 38,653 glyphs**.
  All three canonical outputs are **5,746,025 bytes**, with SHA-256 identical to
  the independent corpus check. Input/output difference is canonical formatting

## Five-sample results

| Operation | Rust | Python | Node | Python/Rust | Node/Rust |
|---|---:|---:|---:|---:|---:|
| Parse BDF into font | 92.418 ms | 373.624 ms | 288.879 ms | 4.04× | 3.13× |
| Write canonical UTF-8 BDF | 48.744 ms | 61.568 ms | 58.535 ms | 1.26× | 1.20× |

Raw five-call measurements are committed separately:
[bdf-rust.json](results/bdf-rust.json),
[bdf-python.json](results/bdf-python.json), and
[bdf-node.json](results/bdf-node.json).

The PLAN §10 BDF parse target of ≤100 ms **passes its specified five-sample median
measurement**. It is close to the boundary: one direct sample was 100.682 ms, and
Criterion's independent parse estimate was **100.38 ms** (95% interval
90.917–111.51 ms, 20 timed iterations). This is not a guarantee that every parse
finishes within 100 ms, and the repeated-estimate point value is slightly above
the target. More margin would be useful on a less variable benchmark host.
Criterion's write estimate was **49.484 ms** (48.881–49.858 ms, 55 iterations).
Its ten-sample run extended collection to ~2.94 seconds to satisfy sampling.
Committed Criterion estimates/samples are under `results/bdf-criterion-*`.

BDF writing is modestly faster here, not an order-of-magnitude speedup. PCF/OTB/
yaff conversion targets, renderer performance, TTF and whole-export goals remain
unmeasured/unimplemented by W1.2; this result does not establish phase-1 completion.

## Reproduce

```sh
cargo build --locked --release -p fontpixel --example bdf_timing
sample=../samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf
target/release/examples/bdf_timing "$sample"
PYTHONDONTWRITEBYTECODE=1 python3 benches/bdf_python.py "$sample"
npm_config_cache=/tmp/fontpixel-npm-cache npm exec --yes --package=esbuild@0.28.2 -- \
  esbuild ../reference/bdfparser-js/src/index.ts --bundle --platform=node \
  --format=esm --outfile=/tmp/fontpixel-bdfparser.mjs
node --expose-gc benches/bdf_node.mjs "$sample" /tmp/fontpixel-bdfparser.mjs
cargo bench --locked -p fontpixel --bench bdf
```

The Criterion path defaults to the supplied sample, or set `FONTPIXEL_BDF_BENCH`.
The TypeScript bundle/npm cache is entirely temporary. Initial npm execution's
home cache path was unavailable; setting an explicit temporary cache resolved it.
No benchmark binaries, package downloads or font copies are bundled. Production
feature dependency trees and toolchain/environment metadata accompany the raw
results. BDF/PCF-only builds currently include flate2 plus five small supporting
crates; no-default remains dependency-free.
