# W1.3 PCF performance

Date: 2026-10-02. These are real codec measurements, not whole-phase completion.
Raw five-call results and Criterion samples/estimates are in `results/pcf-*`.

## Platform and method

- Linux x86_64 shared guest; AMD EPYC 9V74, nine available logical CPUs,
  10,206,508 KiB memory (~9.73 GiB); Rust 1.99.0 release profile
- Single-threaded calls; resident input bytes; filesystem I/O excluded. Five
  independent calls per operation, median wall time as PLAN §10 specifies
- The complete BDF-to-PCF measurement parses the original BDF and serializes its
  real model. Separate write measurements start from the already-parsed model
- Input: supplied `TongPixelSans14Large-SC.bdf`, 5,746,021 bytes / 38,653 glyphs
- Output: 2,995,428 PCF bytes. Rust, Python and Node SHA-256 all agree:
  `867339698270ee9fba76c97fbbe235e4c27d4130c3ef95e8b68120acd12cd238`
- Python 3.12.14 / supplied bdfparser; Node 24.19.0 / read-only bundled
  bdfparser-js. Python/Node GC runs before each independent timed call. Rust
  parse/write result destruction is outside the individual-call timing;
  Criterion BDF-to-PCF also includes the parsed model's in-closure destruction
- The final Rust wall-clock and Criterion commands run sequentially, without
  simultaneous local builds/tests/fuzzing. Other tenant load is uncontrollable.
  A preliminary run overlapped `scripts/check.sh`; it is retained separately as
  `pcf-rust-concurrent-check.json` and is not the isolated-method baseline

## Five-call medians (milliseconds)

| Operation | Rust | Python | Node |
|---|---:|---:|---:|
| Parse original BDF | 85.623 | 351.299 | 251.798 |
| Write PCF from parsed model | 52.543 | 393.515 | 543.451 |
| Read PCF | 16.484 | 364.847 | 58.667 |
| Parse BDF + write PCF | **135.772** | 946.846 | 721.040 |

PLAN targets: BDF parsing ≤100 ms **passes**, BDF-to-PCF ≤200 ms **passes**.
Every sample of the final Rust run is below its corresponding target. This does
not guarantee the thresholds under all shared-machine loads: the preliminary
contended run's BDF parse median was 104.550 ms (three samples exceeded 100 ms).

Criterion 0.8.2, 10 samples / 2-second requested measurement, normal warmup:

| Operation | Point estimate | 95% interval |
|---|---:|---:|
| PCF write | 51.802 ms | 51.164–53.120 ms |
| PCF read | 18.460 ms | 17.804–19.303 ms |
| BDF-to-PCF | 147.73 ms | 145.52–150.71 ms |

Criterion's end-to-end estimate also passes the 200 ms PCF goal. The historical
BDF-only Criterion estimate is documented in `BDF-REPORT.md`; no improved BDF
parser algorithm is claimed by this PCF work item.

## Reproduce

```sh
cargo run --release -p fontpixel --example pcf_timing -- \
  ../samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf
python3 -B benches/pcf_python.py ../samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf
# Use the independently bundled reference as explained in bdf_node.mjs:
node --expose-gc benches/pcf_node.mjs \
  ../samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf /path/to/bdfparser.mjs
cargo bench -p fontpixel --bench pcf
```

`FONTPIXEL_PCF_BENCH` overrides the Criterion input. No font or reference source
is modified. Python bytecode generation is disabled. OTB/yaff, rendering, TTF,
WOFF/WOFF2, bindings, catalogue batch and full Tong export remain later work;
no timing or completion claim is made for those paths.
