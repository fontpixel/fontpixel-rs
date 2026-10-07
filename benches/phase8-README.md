# Phase 8 evidence and reproduction

See [PHASE8-REPORT.md](PHASE8-REPORT.md) for the measurements and
[phase-8.md](../reports/phase-8.md) for acceptance. JSON in `results/phase8-*.json`
is the fresh Astra run; old Sol results are not used.

Run from the repository root. Inputs in sibling `reference/` and `samples/`
are read-only. All tools, temporary reference test copies and build outputs
belong in `../local-work/`; never install Node packages in this repository or
the sibling repositories.

## Environment

The recorded run used Rust 1.99.0, wasm-pack 0.15.0, wasm-bindgen 0.2.129,
wasm-opt 117, Node 24.21.0, TypeScript 6.0.3, Playwright 1.63.0, esbuild 0.28.2,
CPython 3.10.20. `scripts/local-env.sh` selects the external target directory.
The gate runner recorded `CARGO_BUILD_JOBS=2`. Release builds use fat LTO and
the exact wasm-opt flags in `build-info.json`; fast builds are not acceptance
or performance evidence.

Node tooling was installed at
`/home/chen/githubprojects/fontpixel/local-work/p8-tools/`. The harness reads
`FONTPIXEL_JS_TOOLS` to override that location. Use its Playwright CLI to
install browser runtimes if they are not already cached. On this host only,
WebKit needs `FONTPIXEL_WEBKIT_EXECUTABLE` pointing to the local wrapper
`../local-work/p8-tools/webkit-local.sh`, with locally extracted ICU/libxml/JXL
libraries; that environment detail is not a fontpixel runtime requirement.

## Build and gates

```sh
source scripts/local-env.sh
export CARGO_BUILD_JOBS=2
export FONTPIXEL_WASM_LARGE=1
scripts/check_wasm.sh
scripts/check.sh
scripts/check_python.sh /home/chen/githubprojects/fontpixel/local-work/py310-venv
```

`check_wasm.sh` builds the current native CLI, release packages for full,
minimal and merge, runs the JS tests, strict TypeScript smoke, documentation
check and `tools/bindings/gate.sh python,js`. The large tests require the Tong
and Unifont samples at the documented sibling locations. Unit tests report
missing external samples as skips; such a run cannot replace the recorded
large-input acceptance. `FONTPIXEL_WASM_LARGE=1` enables the large merge WOFF2
output. The browser harness requires these inputs and fails if absent. The reference CLI can
be overridden with `FONTPIXEL_CLI`, and Python's frozen golden checks with
`FONTPIXEL_FROZEN_CLI`; both must be built with portable-float.

The archived evidence used the freshly frozen binary
`../local-work/p8-redo/reference/fontpixel`, SHA
`d660d98beb978b9f32f70b99ea71e5b0d178123221079fbbb1ebf669784dcf03`.
The final packages and their hashes are in `../local-work/p8-redo/packages/`.
Targets were cleaned after delivery, so rebuild before using target paths.

## Browsers, benchmarks and initialization

Run these sequentially. Avoid unrelated heavy jobs while timing; the scripts
record load and retain every sample instead of removing inconvenient runs.

```sh
export FONTPIXEL_CLI="$CARGO_TARGET_DIR/release/fontpixel"
export FONTPIXEL_WASM_PKG="$CARGO_TARGET_DIR/wasm-pkg/full"
# On the recorded host, also set FONTPIXEL_WEBKIT_EXECUTABLE as described above.
node benches/phase8_browser.mjs
node benches/phase8_js.mjs
node benches/phase8_sizes.mjs
```

`phase8_browser.mjs` compares real module Worker results with native CLI bytes
and reports in Chromium, Firefox and WebKit, including isolated and ordinary
contexts. It writes `results/phase8-browser.json`. The shared examples run
there and via `node examples/js/run.mjs`.

`phase8_js.mjs` bundles the reference bdfparser-js 3.0.0 sources into external
scratch space and writes `results/phase8-js.json`. It runs five alternating
paired repetitions per host. Inputs, warmups, iterations, result consumption,
GC, native disposal and the noop floor are described in the report and source.
`FONTPIXEL_BENCH_HOSTS` / `FONTPIXEL_BENCH_REPEATS` exist for debugging; shortened
runs must not be labelled the formal five-pair result.

`phase8_sizes.mjs` requires the three supported release packages, builds five
experimental combinations itself, and writes `results/phase8-sizes.json`.
Each combination gets five new Node processes and five new Chromium processes.
All builds finish first, then five rounds rotate/reverse the profile order.
`node benches/phase8_sizes.mjs --measure-only` repeats only measurements using
existing release artifacts after the same feature/hash checks. This was used
for gate 248 after the first fixed-order run showed time/load confounding;
neither its slower nor faster individual samples were selectively retained.
Bytes are preloaded; JS import, compile and async instantiation are separate
timers, with format writes outside timing. WASM/input transfer is excluded;
browser JS import includes loopback HTTP module fetches, not real-world network latency. It rejects fast/unoptimized/dirty
packages and verifies the saved WASM hash and runtime features. Logs for custom
builds live in `../local-work/p8-redo/sizes/`. A failed run saves partial evidence
with a `failure` field and must not replace an accepted result without review.

Other location overrides: `FONTPIXEL_P8_WORK`, `FONTPIXEL_TONG_SAMPLES`,
`FONTPIXEL_UNIFONT`, `BDFPARSER_JS_REFERENCE`. The loopback server mounts only
explicit package/input directories and uses no-store; no public service is
created or deployed.

## Reference suites and heap check

Python reference adapter:
`crates/fontpixel-py/tests/compat_bdfparser/run_reference.py`; JS adapter:
`crates/fontpixel-wasm/tests/compat_bdfparser/run_reference.mjs`. Their
`KNOWN_DIFFERENCES.md` files explain the expected nonzero exits. The exact
default/slow commands are retained in [phase8-gates.json](results/phase8-gates.json).
JS dependencies and copied reference tests belong in an external `--work`
directory. Do not relabel an expected failing reference run as a passing gate.

The portable-float counting-allocator run used the current CLI to generate the
5,621,172-byte SC CFF sfnt, then:

```sh
FONTPIXEL_HEAP_CHECK_FONT=/home/chen/githubprojects/fontpixel/local-work/p8-redo/golden/sc.otf \
  cargo test --locked --release -p fontpixel-heap-check --test encoder_bound \
  -- --ignored --nocapture --test-threads=1
```

The 684 cases passed unchanged admission limits. Quality 10–11 remains an
empirical envelope. This check is independent of the JS throughput benchmark.
