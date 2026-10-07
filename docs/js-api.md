# JavaScript / TypeScript API

`fontpixel` is the in-process WASM binding of `fontpixel` and `fontpixel-ops`.
It provides the native API and a separate bdfparser-js 3.0.0 compatibility API.
The complete signatures, options, Rust mappings and unavailable items are in
the generated [reference](js-api-reference.md). Phase 8 produces local packages;
it does not publish an npm release.

## Build and load

```sh
source scripts/local-env.sh
scripts/build_wasm.sh full
FONTPIXEL_WASM_PKG="$CARGO_TARGET_DIR/wasm-pkg/full" node examples/js/run.mjs
```

The output directory is an ESM package named `fontpixel`; keep its `.wasm`, JS,
Worker and declaration files together. Use a local package mapping or import
`index.js` directly. In an installed/mapped package:

```js
import * as fp from 'fontpixel';
await fp.init();
const bytes = new Uint8Array(await file.arrayBuffer()); // browser File
const loaded = fp.read(bytes, { name: file.name });
try {
  console.log(loaded.report, loaded.fonts.length);
} finally {
  loaded.fonts.free();
}
```

Serve the package over HTTP(S) with `.wasm` MIME type `application/wasm`.
`init()` fetches the adjacent module in browsers/Workers and reads it using
Node's filesystem in Node. It also accepts bytes, a URL/Response or a compiled
`WebAssembly.Module`. `initSync(bytesOrModule)` avoids asynchronous loading.
The no-argument synchronous loader only works where the runtime provides
synchronous host access. On browser main threads use `await init()`: Chromium
rejects synchronous instantiation of modules larger than 8 MB even if already
compiled. Supplying a compiled module to `await init(module)` avoids compiling
it again and uses asynchronous instantiation. `initSync` remains useful in
Workers and Node with explicit bytes/module (including Node 18).
Successful initialization is idempotent, including `initSync()` after `init()`.
Each Worker has its own instance and memory. Share a compiled module, not handles.

| Supported build | Codecs and additional facilities |
| --- | --- |
| `full` | All 99 catalogue codecs, compatibility layer, CJK tables, TOML batch parsing |
| `minimal` | BDF, PCF, YAFF; native model, bitmap, layout, typed operations and PNG |
| `merge` | BDF, PCF, OTB, TTF, OTF, WOFF, WOFF2, CJK tables, OpenType/merge operations and PNG |

All profiles ship TypeScript declarations for the API. Check
`fp.capabilities()` and `fp.formats.catalogue()` before offering a feature.
Some compiled-out namespaces are absent; retained operations reject explicitly
with `FeatureDisabledError`/`UnsupportedError`. A catalogue row is not proof
that its codec is enabled. `build_wasm.sh custom` exists for measured feature
experiments, with no additional supported profile contract.

## Native values, editing and release

Classes use PascalCase, methods use camelCase, and serde option fields retain
Rust's snake_case. For example `font.draw(text, { line_limit: 512 })`.
`read(bytes, options)` returns `{fonts, report, format}`; `write(fonts, 'pcf',
options)` returns a `Written` handle with `output` and `report`.

`FontSet.font(i)` and `Font.glyph(id)` return live views. Editing a view changes
its shared model. A view retains that storage if the parent's handle is freed;
removing the view's target makes later access fail. `copy()` makes an independent
deep copy. A glyph's `bitmap` getter returns a copy; assign it back to commit an
edit. `toDict()`/`fromDict()` cross the boundary as plain data and copy it.

Call `free()` on native handles when finished, or use `[Symbol.dispose]()` in
runtimes supporting explicit resource management. Double release is harmless;
use after release throws `InvalidInputError`. `FinalizationRegistry` is a fallback
with nondeterministic timing. Getter-created handles and iterator-returned views
also need release in long loops. The legacy compatibility facade follows the
reference API, which has no explicit disposal method; its underlying handles
are garbage-collected. Prefer the native API for predictable long-running memory
use, or bound a compatibility workload with a Worker lifetime.

Input buffers are copied into WASM. Returned `Uint8Array` buffers, output assets,
and bitmap bytes copy out; they are not views invalidated by memory growth.
Worker messages use structured cloning, leaving caller buffers attached.
Returned RGBA previews use `Uint8ClampedArray`.

## Plain data and TypeScript

Options reject unknown fields and invalid types. Resource defaults match the
native library; applications may lower them. Integers up to `2^53 - 1` return
as `number`; wider signed/unsigned 64-bit values return as `bigint`. Supply large
integers as bigint, never an already rounded Number. On wasm32, `usize` is
32-bit. JSON.stringify cannot serialize bigint directly; choose an explicit
application encoding rather than silently rounding it.

String-keyed maps return plain objects. Integer map keys are decimal strings;
`Map` is also accepted at the plain-data input boundary. Byte arguments accept
Uint8Array, ArrayBuffer and ArrayBufferView. Bytes nested in serde structures
may use arrays; consult the exact signature instead of assuming every array is
a byte buffer.

```ts
import type { TaskRequest, BatchTaskSpec } from 'fontpixel';
const validate: TaskRequest = { kind: 'validate', input: 'font.bdf' };
const task: BatchTaskSpec = { id: 'check', ...validate };
```

Requests use an internal `kind` discriminant, not an externally tagged wrapper.
Generated declarations and the strict TypeScript smoke test are checked by
`scripts/check_wasm.sh`. Generate the reference after a full build with
`python3 tools/bindings/gen_jsref.py`; `--check` detects stale documentation.

## Errors, strict mode and reports

Catch `FontpixelError` or a specific subclass. Errors preserve `kind`, `exitCode`,
and applicable fields such as `format`, `resource`, `limit`, `line`, `path`,
`candidates` and the complete `report`. `StrictLossError` has exit code 3.
Do not discard a report merely because an output was produced. Unknown requests
and unsupported features fail rather than return an empty success. Host callback
exceptions propagate as their original value; Worker traps reject all pending
calls and permit a later automatic restart unless explicitly terminated.

## Resources and host persistence

Core calls receive bytes and explicit relative resource names. They do not
read arbitrary files or access the network. `resources` accepts an object/Map of
names to bytes, a synchronous `(path, maxBytes) => bytes | null` function, or a
resolver object with `resolve` and optional `list`. Missing sidecars raise
`MissingResourceError`; unsafe resource paths and budget overruns fail.

For asynchronous resources, use `withAsyncResources(run, resolve, options)`.
It fetches requested sidecars and reruns the deterministic synchronous operation.
The default maximum is 1024 rounds. Keep `run` free of host side effects: it may
execute more than once. Putting `await` around a synchronous WASM call does not
move computation off the event loop.

Output bundles are in-memory results, not filesystem transactions. The browser
host chooses download/storage behavior and overwrite confirmation. In Node,
`fontpixel/node-files` provides `readFiles`, `readDirectory` and `writeOutputs`;
these explicit host helpers are not transactional, and earlier writes remain
if a later write fails. Native disk-storage transactions are unavailable inside
WASM. Bindings do not invoke the CLI.

## Operations, Worker progress and cancellation

`fp.ops` exposes all 22 operations over named input buffers. `ops.batch` accepts
typed tasks in every profile; `ops.batchToml`/`ops.parseArgs` need the full profile.
Parallel native job counts are unavailable in WASM. Each instance runs on one
thread, including Brotli's sequential implementation of its deterministic
multi-part stream layout. `portable-float` preserves native/WASM WOFF2 bytes.

Top-level `info(bytes)`, `merge(parts, options)` and `render(bytes, text, options)`
are the W8.3 helpers. `mergeLib` is the lower-level model merge namespace;
`render` follows native `preview` layout/report semantics. These calls are
synchronous. Their progress callback receives read/merge/encode checkpoints;
exactly `false` cancels, and thrown values propagate. Cancellation is checked
between checkpoints, not during a long codec call.

```js
const worker = await fp.createWorker();
const abort = new AbortController();
try {
  const result = await worker.merge(
    [{ bytes, name: 'font.bdf', take: 'all' }],
    { family: 'My Font', spacing: 'prop', outputs: ['font.bdf', 'font.woff2'] },
    { signal: abort.signal, onProgress: event => console.log(event) },
  );
  // result.files contains bytes; result.report retains the native report.
} finally {
  worker.terminate();
}
```

`createWorker()` supports browser module Workers and Node worker_threads.
`cancellation: 'auto'` uses cooperative SharedArrayBuffer cancellation when
available (Node, or cross-origin isolated browser pages); otherwise it terminates
the running Worker and restarts it for the next call. Browser cooperative mode
requires COOP `same-origin` and COEP `require-corp` (and compatible resource
headers). A requested cooperative mode without shared memory rejects explicitly.
Hard termination cancels queued calls too. Explicit `worker.terminate()` closes
the client permanently. Reuse a Worker for repeated work; `mergeInWorker` creates
and closes one when no Worker is supplied.

## Compatibility and examples

`fontpixel/compat/bdfparser` initializes itself and mirrors the lowercase legacy
API: `new Font().load_bytes(bytes)`, `font.glyphbycp(65)`, `font.draw(text)`,
`bitmap.todata(4)`, and the PCF/TTF helpers. `/cjk` retains the reference side-effect
import; `/cli` exposes explicit Node host operations. This API is separate from
the native `Font`/`Bitmap` classes. See the checked
[known differences](../crates/fontpixel-wasm/tests/compat_bdfparser/KNOWN_DIFFERENCES.md).

[recipes.mjs](../examples/js/recipes.mjs) contains 12 executable examples covering
read/write, model editing, pixels, PNG, bigint, limits, sidecars, WOFF2, batches,
Worker merge/cancel, compatibility and capabilities. The Node runner above and
the Chromium/Firefox/WebKit harness execute the same recipes. Run the browser
harness with `node benches/phase8_browser.mjs`; its tooling dependencies live
outside the repository (`FONTPIXEL_JS_TOOLS`). This is a test harness, not the
phase 9 tool page.
