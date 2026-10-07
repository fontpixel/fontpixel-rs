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
  (`pythonize`) and JS objects (a serde_json bridge with exact integers, see
  "JS/WASM" below) with the Rust field names in snake_case. Unknown keys are rejected (`deny_unknown_fields`); invariants are
  checked on deserialization. Types that cannot be plain data are listed in
  `docs/bindings-serde-exclusions*.md` with the handle or function that exposes them.
- **Handles**: `FontSet`, `Font`, `Glyph`, `Bitmap`, prepared variants, registries
  and resolvers stay in Rust. Python/JS objects own or share them; copying is
  explicit (`copy()`/`clone()`), pixel data is copied out only on request
  (`to_bytes`, `rows`, `to_list`, NumPy-free).
- **Stable glyph IDs** are integers; codepoints are tagged values
  (`{"Unicode": n}` / `{"Legacy": n}`) in plain data and typed classes in the API.
- **Errors** map to one exception (Python) or `Error` subclass (JS) per
  `fontpixel::Error` variant, carrying `kind`, structured fields (`resource`,
  `limit`, `format`, `operation`, `candidates`, `line`) and the full `report` for
  strict-loss and reported-error failures.
- **Reports** are returned as plain data with every entry; nothing is truncated.
- **Integers**: Rust `u32`/`i64`/`usize` map to Python `int`; in JS an integer is
  `number` when |v| ≤ 2^53 − 1 and `bigint` otherwise, decided per value; 64-bit
  fields are typed `number | bigint` (`WideInt`) in the TypeScript types.
- **Inputs and outputs**: bytes (`bytes`/`bytearray`/`memoryview`, `Uint8Array`/
  `ArrayBuffer`) plus explicit named resource sets or resolver callbacks for
  sidecars. Python additionally offers filesystem mode (paths, sidecar discovery,
  source protection, transactional writes); the browser never reads paths.
- **Unavailable capabilities** (for example threads or filesystem access in WASM)
  are queryable (`capabilities()`) and fail with `UnsupportedError` naming the
  reason and the alternative; they never return empty success.
- **Progress and cancellation**: `fontpixel_ops::merge_with_progress` /
  `convert_with_progress` call an observer with plain-data `Progress`
  (`{step, done, total, item, glyphs, bytes}`, `step` `read`/`merge`/`encode`)
  at step boundaries; `ControlFlow::Break` fails with
  `fontpixel::Error::Cancelled` before anything is committed. Python:
  `progress=` keyword, `False` cancels (`CancelledError`).
- **Callbacks on wasm**: `FileProvider`, `ResourceResolver` and the format
  hook traits are `Send + Sync` only where threads exist, so on wasm they can
  wrap JS functions directly. WOFF2 parallel Brotli and `encode_many` run
  sequentially there with byte-identical output (see decisions.md,
  "wasm32 readiness of core and ops").

## JS/WASM (phase 8)

Crate `crates/fontpixel-wasm`, npm package `fontpixel` (`package.json` is
`private`; nothing is published). Decided in decisions.md, "2026-10-06 — WASM
binding foundation".

### Toolchain and builds

- `wasm-bindgen = "=0.2.129"` and `js-sys = "=0.3.106"` (exact pins). wasm-pack
  0.15 installs the wasm-bindgen CLI of the version in `Cargo.lock`, so glue and
  crate always match. `scripts/build_wasm.sh PROFILE [OUT]` runs
  `wasm-pack build --target web --release --no-opt --no-pack` with fat LTO
  (`CARGO_PROFILE_RELEASE_LTO=fat` for this build only), then wasm-opt 117 with
  `-Oz` and the feature flags rustc 1.99 emits (bulk memory, nontrapping
  float-to-int, sign extension, mutable globals, reference types, multivalue),
  then assembles the package. `build-info.json` records the profile, the tool
  versions, the wasm-opt flags and the module size before and after wasm-opt
  (raw and gzip -9). `WASM_OPT=off` skips wasm-opt (sizes are then
  pre-optimization only).
- Profiles (crate features): `full` (default: every format, the
  bdfparser-js layer, CJK charsets, TOML batch text and the clap request parser),
  `minimal` (BDF, PCF, YAFF) and `merge` (the W8.3 page: BDF/PCF/OTB/TTF/OTF/
  WOFF/WOFF2 + CJK charsets). `fontpixel-ops` is always built without `fs`
  (cap-std does not build for wasm32). Output: `$FONTPIXEL_WASM_OUT/<profile>`,
  else `${CARGO_TARGET_DIR:-target}/wasm-pkg/<profile>` (outside the repository
  when `CARGO_TARGET_DIR` is set, gitignored otherwise).
- `scripts/check_wasm.sh [PROFILE…]` (default `full minimal`): wasm32 clippy
  `-D warnings` for all three profiles, the crate's native unit tests,
  `gen_dts.py --check`, build + node:test suites per profile, a `tsc --strict`
  smoke (`tests/typing/smoke.ts`) over the declarations, then
  `tools/bindings/gate.sh python,js` (`SKIP_GATE=1` stops before it).
  `scripts/check.sh` keeps the native gate: the crate compiles natively in the
  workspace clippy/test lines, plus one wasm32 clippy line for `full`.

### Package layout and loading

```
<pkg>/package.json        private, "type": "module", exports "." -> index.js
<pkg>/index.js, index.d.ts   entry point (crates/fontpixel-wasm/js/)
<pkg>/errors.js, errors.d.ts error classes (imported by the glue and index.js)
<pkg>/plain.js            plain-data codec (internal, imported by the glue)
<pkg>/guard.js            released-handle guard (internal)
<pkg>/resources.js, .d.ts withAsyncResources
<pkg>/fontpixel_wasm.js, .d.ts, fontpixel_wasm_bg.wasm   wasm-bindgen output
<pkg>/build-info.json
```

The glue imports `./errors.js` and `./plain.js` through
`#[wasm_bindgen(raw_module = "./…")]`, and `index.js` re-exports the classes
from the same file, so there is one module instance and `instanceof` works.
Hand-written JS and `.d.ts` live in `crates/fontpixel-wasm/js/`; area units
that need JS wrappers add `js/<area>.js` (+ `.d.ts`), re-export them from
`index.js`/`index.d.ts` and add them to the copy list in
`scripts/build_wasm.sh`.

- `await init(source?)` (idempotent): browsers and workers fetch the `.wasm`
  next to the module (`wasmUrl`); Node reads it with `node:fs/promises` (Node's
  `fetch` cannot read `file:` URLs). `source` may also be a URL, Response,
  bytes or a compiled `WebAssembly.Module`.
- `initSync(bytesOrModule)`; without an argument only where
  `process.getBuiltinModule` exists (Node ≥ 20.16/22.3). Browsers compile
  large modules synchronously only off the main thread.
- Workers: compile once with `WebAssembly.compile`, `postMessage` the
  `WebAssembly.Module`, `initSync(module)` in the worker (tested with
  `worker_threads`). Each worker has its own instance and memory: handles never
  cross workers, only plain data and bytes do.
- A Rust panic traps the instance (`panic = abort`); a hook logs the message
  with `console.error` first. The instance must then be discarded (restart the
  Worker). A panic inside a batch task does the same on wasm.

### Naming

Classes PascalCase (`#[wasm_bindgen(js_name = Font)] pub struct JsFont`),
methods/functions/getters camelCase (`js_name = addGlyph`), plain-data keys
snake_case (the Rust field names, unchanged), enum values as serde spells them.
Each area adds methods in its own file with
`#[wasm_bindgen(js_class = Font)] impl JsFont { … }`; several impl blocks per
class across files work with the pinned wasm-bindgen (no feature flag).

### Plain data, numbers, maps, bytes (`src/conv.rs`, `js/plain.js`)

- In: `encodePlain` (JS) writes the value as JSON text with exact integers;
  serde_json parses it straight into the Rust type (serde's messages name the
  expected type; serde_json's position suffix is removed). Unknown keys are
  rejected by the types; plain-data failures throw `InvalidInputError`
  (`exitCode` 1; request builders use 2 like Python's ops). Only plain objects,
  arrays, `Map`s, strings, numbers, bigints, booleans and `null`/`undefined`
  are plain data (a handle or `Date` is rejected with its class name);
  `undefined` object members are omitted.
- Out: serde_json writes JSON text; integers beyond 2^53 − 1 become marked
  strings that `decodePlain`'s reviver turns into bigints (plain `JSON.parse`
  when there are none). A string starting with NUL, or raw JSON with a wide
  integer, takes an exact `serde_json::Value` → JS walk instead. Non-finite
  floats serialize as `null` (serde_json).
- Numbers: per value, `number` when |v| ≤ 2^53 − 1, else `bigint`. Inputs take
  both; a non-integral number, NaN/±Infinity, or an integral number beyond
  2^53 (not exact) is rejected by integer fields (`floating point … expected
  u64`); bigints of any size are exact (range-checked by the type). Floats are
  written in shortest round-trip form without `+` (`1e21`), which keeps them
  exact through the `arbitrary_precision` buffers of the ops requests.
  `usize`/`isize` are 32-bit on wasm32: always `number`, larger inputs fail.
  TypeScript: `u64`/`i64`/`u128`/`i128` fields are `WideInt` (`number |
  bigint`), all other numeric fields `number`.
- Method parameters: integers are declared `f64` in Rust and checked with
  `conv::int::<T>(v, "name")` (exact, in range) — never `u32`/`usize`
  parameters, which wasm-bindgen would wrap or truncate; 64-bit parameters are
  `JsValue` + `conv::int_from_js` (number or bigint), results
  `conv::int_to_js`.
- Maps: plain objects; integer keys are decimal strings (`{"65": 3}`), and
  inputs may also be `Map`s with string, safe-integer or bigint keys
  (serialization output is always accepted back). JS orders integer-like keys
  numerically, which equals Rust's `BTreeMap` order for unsigned keys.
- Bytes: byte parameters accept `Uint8Array`, `ArrayBuffer`, `DataView`
  (`BytesLike`, copied once, `conv::bytes_from_js`); byte results are fresh
  `Uint8Array`s (`conv::bytes_to_js`, never views of WASM memory). Byte lists
  inside plain data are `number[]` on output (`SerdeBytes`) and accept the
  buffer types on input.
- Options objects (`conv::Options`): plain objects whose keys are checked
  against the allowed list (unknown keys throw); trailing option parameters
  are `Option<JsValue>` with `unchecked_optional_param_type` so TypeScript
  makes them optional.
- Cost (full build, Node 24, Tong Pixel Sans 14 SC, 38,653 glyphs, 23 MB of
  JSON): `toDict` 215 ms (a bare `JSON.parse` of the same text: 125 ms),
  `fromDict` 745 ms, `copy()` 7 ms. Plain data is for options, reports and
  inspection; bulk edits go through handles
  (`crates/fontpixel-wasm/bench/foundation.mjs`).

### TypeScript

wasm-bindgen writes the classes and functions; parameter/return types of plain
data use `unchecked_param_type`/`unchecked_optional_param_type`/
`unchecked_return_type` naming types of `ts/types.d.ts`, generated by
`tools/bindings/gen_dts.py` from the rustdoc JSON through `serde_shapes.py`
(same names as the Python `fontpixel.types`; the plain shape of a binding class
`X` is `XDict`), plus the hand-written aliases in `ts/aliases.d.ts`
(`BytesLike`, `WideInt`, `ResourcesLike`, `ProgressFunction`, …). Both are
emitted through `typescript_custom_section` into `fontpixel_wasm.d.ts`
(stripped from the module, no size cost). `SerdeTypes` maps every serde type's
path to its declaration. Regenerate after any serde change; the gate checks it.

### Errors (`src/errors.rs`, `js/errors.js`)

`FontpixelError extends Error` with one subclass per `fontpixel::Error`
variant, named as in Python: `UnknownFormatError`, `FeatureDisabledError`,
`NotImplementedFormatError`, `AmbiguousFormatError`, `UnrecognizedFormatError`,
`InvalidInputError`, `BdfParseError`, `ResourceLimitError`,
`MissingResourceError`, `MultipleAssetsError`, `StrictLossError`,
`ReportedErrorsError`, `FontpixelIoError`, `CancelledError`, plus
`UnsupportedError`. Every error has `kind` (the variant name), `exitCode` and,
where Rust has them, `format`, `operation` (`"Read"`/`"Write"`),
`candidates`, `line`, `resource`, `limit`, `path`, `count`, `ioKind` and
`report` (complete plain data). `UnsupportedError` (kind `Unsupported`) also
covers `Error::Io` of kind `Unsupported`; capability gates add `capability`,
`reason`, `evidence`, `alternative`. Rust code returns `JsResult<T>` and maps
with `.js()` (`errors::IntoJs`), `errors::to_js`, `errors::invalid`,
`errors::request_error` or `capabilities::unsupported`. Errors wasm-bindgen
raises itself (a non-handle where a handle class is expected) are plain
`Error`s.

### Handles (`src/handles.rs`, `js/guard.js`)

`FontSet`, `Font`, `Glyph`, `Bitmap` keep their data in WASM memory with the
Python semantics: owned values or views addressed by index/ID (a view whose
target disappeared throws `InvalidInputError`), reading a glyph's `bitmap`
returns a copy, `copy()` is the explicit deep copy, a view keeps the shared
storage alive after its container's handle is freed. Storage is
`Rc<RefCell<…>>`; every exported method takes `&self` and borrows through
`handles::borrow`/`borrow_mut`, so a JS callback re-entering a value the
running call holds gets `InvalidInputError`, never a panic or wasm-bindgen's
aliasing error. Bindings snapshot handle arguments before borrowing the
receiver mutably, and never hold a borrow while calling JS unless the callback
contract says so (Python's `edit_glyph` pattern: copy out, call, write back).

Release: `free()` and `[Symbol.dispose]()` (so `using x = …` works);
otherwise a `FinalizationRegistry` frees the memory after garbage collection.
`guard.js` wraps every exported class: using a released handle (as receiver or
argument) throws `InvalidInputError` ("this Bitmap handle was released") instead
of wasm-bindgen's `Error("null pointer passed to rust")`, and `free()` is
idempotent. Cost: about 10 ns per call (1M `getPixel`: 39 ms guarded, 29 ms
unguarded).

### Resources, callbacks, async boundaries (`src/resources.rs`, `src/callbacks.rs`, `js/resources.js`)

Synchronous calls never read files or the network. `resources` is a plain
object or `Map` of relative path → bytes, a function `(path, maxBytes) =>
bytes | null | undefined`, or `{resolve(path, maxBytes), list?(directory,
maxEntries, maxBytes)}`. `null`/`undefined` or a thrown `MissingResourceError`
means "missing"; any other exception aborts the call and is rethrown unchanged
(the same object) when it returns. JS callbacks reach the core/ops traits
through `CallbackScope` tokens (a thread-local slot, `Send + Sync` index), which
compile on native hosts too; on wasm the traits have no `Send + Sync` bound,
so a type holding a `JsValue` could implement them directly, but the token
keeps one code path for both targets.

Async sources use `withAsyncResources(run, resolve, {files, list, maxRounds})`:
it runs the deterministic synchronous call with a recording resolver over the
files fetched so far, awaits every name the run asked for and did not have
(`resolve(path)`, all of a round in parallel; `null` marks a name absent), and
runs again until a run asks for nothing new; the result is what a synchronous
call with every file would return. `maxRounds` (default 1024) bounds it with
`ResourceLimitError`.

### Progress, cancellation, Worker (W8.3)

`callbacks::ProgressCallback` adapts a JS function to the observer of
`fontpixel_ops::merge_with_progress`/`convert_with_progress`: it receives
`Progress` plain data; only a return of exactly `false` cancels
(`CancelledError`), a throw cancels and is rethrown. The merge page runs merges
in a Web Worker: the worker posts each progress record to the page;
cancellation is `worker.terminate()` (always available, nothing returned), or
cooperative through a `SharedArrayBuffer` flag the progress callback reads
(`Atomics.load`) when the page is cross-origin isolated — a busy synchronous
call cannot receive messages.

### Capabilities (`src/capabilities.rs`)

`capabilities()` returns `{name, available, description, reason, evidence,
alternative}` records: `filesystem` and `threads` (unavailable on wasm),
`toml_batch`, `compat_bdfparser`, `cjk_charsets` (per profile),
`sync_resolvers` (available). `requireCapability(name)` throws
`UnsupportedError` with the same fields. Entry points kept for API
completeness that need an unavailable capability call
`capabilities::unsupported(name, "entryPoint")`; areas add records to
`CAPABILITIES` for every new unavailable item. Never an empty success.

### Tests

node:test suites `crates/fontpixel-wasm/tests/<area>.test.mjs` (run with
`node --test crates/fontpixel-wasm/tests/*.test.mjs`; `FONTPIXEL_WASM_PKG`
selects the package, default `${CARGO_TARGET_DIR:-target}/wasm-pkg/full`).
`tests/helpers.mjs` loads and initializes the package, reads samples from
`tests/data`, and runs the native CLI (`FONTPIXEL_CLI`, default the current
target's release binary; the acceptance gate builds it) to produce reference bytes —
tests may run the CLI, the binding never does. Profile-dependent tests skip
with a reason (`has(fp, 'compat_bdfparser')`, `fp.optionTypes()`). Coverage
rules name tests as `crates/fontpixel-wasm/tests/<area>.test.mjs::<test name>`
(the checker looks for the file and the name in it).

### bdfparser-js compatibility (`fontpixel/compat/bdfparser`)

The independent legacy API mirrors bdfparser-js 3.0.0 (SPEC-v3): `Font`,
`Glyph`, `Bitmap` with bdfparser-js's lowercase method names, option objects
and defaults, `readPcf`/`readPcfAsync`/`writePcf`, `readTtf`/`writeTtf`,
`bitmapContours`/`rasterize`, the charset functions (`getCodec`,
`canonicalCharset`, `sanitizeCharset`, `charsetProps`, `detectGL`,
`char2cp`/`cp2char`, ...), `fmtnum`, `defaultglyphname`, the constants,
`$Font`/`$Glyph`/`$Bitmap`, `BDFParseError` and the warning handler
(`setWarningHandler`, `warn`, `defaultWarningHandler`). Package subpaths
(`full` build only, capability `compat_bdfparser`):

```
fontpixel/compat/bdfparser       js/compat_bdfparser.js (+ .d.ts)
fontpixel/compat/bdfparser/cjk   side-effect module kept for bdfparser-js's
                                 `bdfparser/cjk` import (the exact tables are built in)
fontpixel/compat/bdfparser/cli   loadFontFile, main (Node.js; node:fs in JS, in-process)
```

- The module initializes the instance itself (top-level `await init()`), so
  `import { Font } from 'fontpixel/compat/bdfparser'` is ready to use; a
  Worker that wants `initSync(module)` calls it before importing the module.
  In a build without the layer the import rejects with `UnsupportedError`.
- Data stays in Rust: the shared `fontpixel::compat::bdfparser` model (the
  same one the Python `fontpixel.compat.bdfparser` uses) behind the handles
  `BdfparserRawFont`, `BdfparserRawGlyph`, `BdfparserRawBitmap`
  (`src/compat*.rs`; `BdfparserRawBitmapList` for `concatall`,
  `BdfparserRawCodec` for charsets, `bdfparser*` module functions). They are
  public (also reachable with `rawHandle(obj)`) and expose the Rust compat
  API one to one, e.g. `BdfparserRawFont.core()` (a native `Font` copy) and
  `fromCore`, `report()`, `saveTo(writer, format, options)` over a JS writer
  object, `loadFileObj(reader)`; `loadFilePath`/`save(path)` throw
  `UnsupportedError` (`filesystem`).
- `font.headers` (a Proxy), `font.props` (a Proxy with a null prototype),
  `font.glyphs` (a `Map` subclass), `font.unencoded` (an array Proxy) and
  `font._propnames` (a `Map` subclass) are live views: reads project the
  model (cached until the next edit), writes edit it in place, so glyph
  handles stay attached. Item assignment into a 17-slot meta list
  (`font.glyphs.get(cp)[i] = v`, `font.unencoded[i].meta[k] = v`) and to
  `font.unencoded[i].altcode` is written back; `push`, `splice` and
  `length = n` on `font.unencoded` edit the font. Values read are copies
  (pushing into a returned array does not change the font).
- `Glyph.meta` is a plain object; direct edits are written with the next
  editing method (only the changed keys, so another handle's edits are not
  reverted). `Bitmap.bindata` is an accessor building the rows on each read.
  Handles live in non-enumerable symbol properties, so `toEqual`-style
  comparisons see bdfparser-js's attributes only.
- Warnings: the Rust model records them in its report; after each call the
  layer passes the new warning-level entries to the current handler (with
  bdfparser-js's quoting: `JSON.stringify` for numbers in parse messages,
  plain single quotes for keywords). bdfparser-js's own JS-side warnings
  (overlay size mismatch, `addglyph` without `replace`, unknown charsets of
  the module functions) are emitted by the JS layer. The native PCF
  decoder's notes stay in the report (bdfparser's binary readers are silent).
- Errors: Rust failures become bdfparser-js's classes (`BDFParseError` with
  `lineno`/`msg`, `RangeError`, `TypeError`, `Error`) with the same message;
  the original `FontpixelError` is the `cause` and its fields (`kind`,
  `exitCode`, `line`, ...) are copied. Exceptions of JS callbacks
  (converters, `transform` functions, reader/writer objects) are rethrown
  unchanged.
- Charsets always use fontpixel's exact tables (`getCodec(...).exact` is
  `true`; `iconv` options are accepted and unused).
- The unmodified bdfparser-js suite (vitest 5.0.3) runs through
  `tests/compat_bdfparser/run_reference.mjs` (shim modules for every
  `../src/*` import); every remaining failure is explained in
  `tests/compat_bdfparser/KNOWN_DIFFERENCES.md`.

## Coverage gate

`tools/bindings/gate.sh python` (phase 7) and `tools/bindings/gate.sh python,js`
(phase 8) build nightly rustdoc JSON for `fontpixel` and `fontpixel-ops`, list every
public item (`tools/bindings/inventory.py`) and check `bindings/coverage/*.toml`
(`tools/bindings/check.py`): every item mapped by exactly one most-specific rule,
`bound`/`adapted` rules name an existing test per required language, `unavailable`
rules carry reason and evidence, no stale rules. A new public Rust item without a
mapping fails the gate.

Phase 8: with `js` required, every `bound`/`adapted` rule needs a `js` entry
(the JS name; `{camelName}` expands to the last path segment in camelCase,
`{name}` to it unchanged) and a `js_test`
(`crates/fontpixel-wasm/tests/<area>.test.mjs::<test name>`), the summary line
adds `missing.js.{entry,test}` counts, and `gen_dts.py --check` runs.
`unavailable` rules on the JS side (an item that exists natively but cannot
work in WASM) keep their own rule with `reason`/`evidence` and the entry point
throwing `UnsupportedError`. Area units fill the `js`/`js_test` fields of their
own `bindings/coverage/<area>.toml` rules by hand; `formats.toml` is generated
by `tools/bindings/gen_formats_coverage.py`, whose `js_entry()`/`js_test_of()`
derive the JS names from the Python ones (`fontpixel.` dropped, snake_case
identifiers in camelCase; `tests/test_<area>.py::test_<name>` →
`tests/<area>.test.mjs::<name with spaces>`) — the formats unit names its
tests that way or edits the scheme there.

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
