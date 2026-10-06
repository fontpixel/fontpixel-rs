# Binding design (phases 7 and 8)

Requirements are in [bindings-scope.md](bindings-scope.md); the architecture decision
is recorded in [decisions.md](decisions.md) under "Phases 7–9 architecture". This page
is the working contract for everyone implementing the Python and WASM/JS bindings.

## Layers

```
fontpixel (core)      models, bitmaps, rendering, charsets, 99 format codecs,
                      sfnt/WOFF/variants/subset/merge, reports, resource limits
fontpixel-ops         typed operations formerly owned by the CLI: convert, batch,
                      merge, variants, subset, charset, validate, diff, info,
                      preview/render, registry queries; memory and filesystem modes
fontpixel-cli         clap parsing + terminal/file I/O over fontpixel-ops
fontpixel-py          pyo3 module `fontpixel._native` + Python package `fontpixel`
fontpixel-wasm        wasm-bindgen package `fontpixel` (phase 8)
```

Bindings call `fontpixel` and `fontpixel-ops` in-process. Nothing spawns the CLI or
builds argv strings.

## Shared conventions

- **Typed options and plain data** cross the boundary through serde: Python dicts
  (`pythonize`) and JS objects (`serde-wasm-bindgen`) with the Rust field names in
  snake_case. Unknown keys are rejected (`deny_unknown_fields`); invariants are
  checked on deserialization. Types that cannot be plain data are listed in
  `docs/bindings-serde-exclusions*.md` with the handle or function that exposes them.
- **Handles**: `FontSet`, `Font`, `Glyph`, `Bitmap`, prepared variants, registries
  and resolvers stay in Rust. Python/JS objects own or share them; copying is
  explicit (`copy()`/`clone()`), pixel data is copied out only on request
  (`to_bytes`, `rows`, `to_list`, NumPy-free).
- **Stable glyph IDs** are integers; codepoints are tagged values
  (`{"unicode": n}` / `{"legacy": n}`) in plain data and typed classes in the API.
- **Errors** map to one exception (Python) or `Error` subclass (JS) per
  `fontpixel::Error` variant, carrying `kind`, structured fields (`resource`,
  `limit`, `format`, `operation`, `candidates`, `line`) and the full `report` for
  strict-loss and reported-error failures.
- **Reports** are returned as plain data with every entry; nothing is truncated.
- **Integers**: Rust `u32`/`i64`/`usize` map to Python `int`; in JS, values that can
  exceed 2^53 are `bigint`, everything else `number` (documented per field in the
  TypeScript types).
- **Inputs and outputs**: bytes (`bytes`/`bytearray`/`memoryview`, `Uint8Array`/
  `ArrayBuffer`) plus explicit named resource sets or resolver callbacks for
  sidecars. Python additionally offers filesystem mode (paths, sidecar discovery,
  source protection, transactional writes); the browser never reads paths.
- **Unavailable capabilities** (for example threads or filesystem access in WASM)
  are queryable (`capabilities()`) and fail with `UnsupportedError` naming the
  reason and the alternative; they never return empty success.

## Coverage gate

`tools/bindings/gate.sh python` (phase 7) and `tools/bindings/gate.sh python,js`
(phase 8) build nightly rustdoc JSON for `fontpixel` and `fontpixel-ops`, list every
public item (`tools/bindings/inventory.py`) and check `bindings/coverage/*.toml`
(`tools/bindings/check.py`): every item mapped by exactly one most-specific rule,
`bound`/`adapted` rules name an existing test per required language, `unavailable`
rules carry reason and evidence, no stale rules. A new public Rust item without a
mapping fails the gate.

## Testing

- `crates/fontpixel-py/tests/` (pytest): native coverage per area (normal, error,
  boundary, strict, resource-limit behaviour) and a read/write/round-trip test per
  format ID compared with direct Rust results.
- Compatibility: the unmodified `reference/bdfparser/tests` run against
  `fontpixel.compat.bdfparser` through an import shim; every failure is explained.
- Benchmarks compare whole workloads and single small calls against pure Python
  bdfparser and report slower calls as slower.
- Build: `maturin develop`/`maturin build` (abi3, CPython ≥ 3.10); manylinux wheels
  with `maturin build --release --zig --compatibility manylinux2014`; nothing is
  published. `scripts/build_wheels.sh` builds the x86_64 and aarch64 wheels into
  `local-work/wheels/`, runs `auditwheel show` and the pytest suite against the
  installed wheel on CPython 3.10 and 3.12 (results in `benches/PHASE7-REPORT.md`).
