# W1.4 rendering benchmarks

Measured 2026-10-02 in the shared assigned Linux cloud workspace after the
renderer's bounded diagnostic deduplication review. Production Rust uses the
normal release profile, no target-CPU override and no unsafe project code.

## Platform and method

- AMD EPYC 9V74 80-Core Processor (reported model); **9 logical guest CPUs**
- MemTotal 10,206,508 KiB (~9.73 GiB), x86_64 Linux 6.18.44
- Rust 1.99.0 / LLVM 23.1.1, CPython 3.12.14, Node v24.19.0
- Same supplied read-only pure-Python and TypeScript bdfparser implementations;
  esbuild 0.28.2 temporary bundle, no source modification or runtime dependency
- Resident parsed font, one warmup per operation, **median of five single-call
  wall-clock samples**; no parsing, process startup, I/O or image export included
- Each output is retained through the end of its measured call. Rust drop/Python
  release is outside timing; Node uses normal automatic GC. No explicit GC or
  affinity is imposed. These are native resident APIs, not Python/WASM bindings
- Final five-call runs were sequential after corpus validation. Other workloads
  can exist on this shared host, so figures are measurements, not hard guarantees
- Input TongPixelSans14Large-SC.bdf has 38,653 glyphs. Screen text is 2,000 `永`
  characters, cell mode, limit 80 × the font's 32-pixel global box. All languages
  produce **2560×450** (80×25 cells of 32×18); this includes the reference's full
  cell rather than pretending every cell is a 14×14 tight ink bitmap
- drawall uses the reference's cell/ascending/512 defaults and produces
  **512×43,488**, requiring the explicit 64-Mi-pixel output budget

## Five-call results

| Operation | Rust | Python | Node | Python/Rust | Node/Rust |
|---|---:|---:|---:|---:|---:|
| 2,000-character 80×25 cell screen | 1.006 ms | 43.804 ms | 9.282 ms | 43.53× | 9.22× |
| All 38,653 effective codes | 29.189 ms | 1103.633 ms | 423.255 ms | 37.81× | 14.50× |

All raw samples/dimensions are in [Rust](results/render-rust.json),
[Python](results/render-python.json) and [Node](results/render-node.json).
The real-font screen is in PLAN §7's intended microsecond-to-millisecond range.
This is a rendering result only; codec/export and binding targets remain separate.

## Criterion synthetic hot paths

Thirty samples, one-second warmup and two-second measurement target:

| Operation | Estimate | 95% interval |
|---|---:|---:|
| 80×25 screen of synthetic 8×16 glyphs, advance mode | 383.42 µs | 378.90–391.44 µs |
| drawall of 96 synthetic glyphs, cell mode | 18.162 µs | 17.747–18.802 µs |

Criterion estimates use its regression estimator, not the five-call median above;
these are also different, smaller synthetic rasters. Raw estimates including all
statistics are in [render-criterion.json](results/render-criterion.json).
The renderer allocates placement/line buffers and the final raster, with no
per-glyph bitmap or line-image allocation. Exceptional loss reports have their
own bounded allocations. A 12,000-distinct-missing-code regression guards against
the reviewed quadratic diagnostic-deduplication behavior.

## Reproduce

```sh
cargo run --locked --release -p fontpixel --example render_timing -- \
  ../samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf
python3 -B benches/render_python.py ../samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf
# Build the temporary bundle using benches/bdf_node.mjs instructions first.
node benches/render_node.mjs ../samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf /tmp/fontpixel-bdfparser.mjs
cargo bench --locked -p fontpixel --bench render --no-default-features -- \
  --warm-up-time 1 --measurement-time 2 --sample-size 30
```

The full synthetic/real-font pixel comparisons and independent PNG/SVG/terminal
validation are reported in [W1.4](../reports/w1.4.md). No benchmark result substitutes
for those correctness checks. PNG compression throughput, binding crossing costs,
OpenType/vector outputs and complete application exports remain future measurements.
