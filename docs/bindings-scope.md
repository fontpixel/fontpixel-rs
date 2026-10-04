# Python and WASM/JS binding requirements

This is a requirements document, not implementation or acceptance evidence.

The owner authorized continuing after phase-6 delivery through phase 7 then phase
8 in message Sentinel_48c10f3ada288191bde0fb6104d272c7:
“我要出去一下，假如做好了，你继续往下做第7、8阶段”.

The owner clarified the binding scope in message
Sentinel_dace93f6188481919f6c86afd528fc84:
“我希望在Python 绑定和WASM/JS 绑定时，不是仅针对bdfparser、bdfparser-js兼容，当然他们也要兼容，但是要Python和WASM/JS绑定所有fontpixel-rs的功能，开发作为API，可以吗？”

Both languages must expose all public fontpixel-rs capabilities as a complete
native developer API, plus a separate legacy-compatible API. A few bulk functions,
a CLI subprocess wrapper or compatible Font/Glyph/Bitmap alone are insufficient.

## Shared coverage gate

Maintain an exhaustive Rust → Python → WASM/JS matrix at public module, type,
constructor/method/function, option and error/result granularity. Each item needs
its binding entry, feature conditions, native-equivalence tests, platform differences
and implementation status. Automatically detect newly unmapped public capabilities.
Language-idiomatic interfaces may adapt Rust lifetime/generic/trait syntax while
preserving capabilities. A module-level checkbox cannot hide missing operations.

Coverage includes all format registry/capability/detection/read/write operations
and typed options for all99 IDs; models, stable glyph IDs, code/sequence mappings,
metrics/names/properties; all Mono/Gray8 bitmap construction/editing/geometric/pixel
operations; rendering/preview/PNG/SVG; outline/vectorization/rasterization;
OpenType tables, collections, WOFF/WOFF2; regional variants/locl, subsets,
charset, merge; convert/batch/validate/diff; all structured errors/reports/locations,
loss/strictness, resource limits, retained/clone accounting, sidecar/directory and
multi-asset handling. The list is a minimum, not a reason to omit other public APIs.

CLI-owned domain orchestration must be shared and callable through bindings.
File persistence should be separate from memory bundles, maintaining promised
source-alias protection and transaction behavior where filesystem mode applies.
Bindings preserve actual codec profiles and limitations; all formats does not
mean arbitrary historical variants or universal lossless conversion.

Native coverage is separate from legacy reference parity. Test each operation's
normal/error/boundary/strict/resource behavior and each format's binding-layer
read/write/roundtrip or appropriate direct-Rust equivalence. Compare models,
pixels/metrics, deterministic bytes where promised, and complete reports. Test
disabled features, sidecars, directories, multi-assets and complete workflows.

Genuine target/dependency inability must be queryable and fail explicitly, with
reason, evidence, documented alternative and acceptance status. Missing work is
unfinished, not proof of platform impossibility. Do not silently remove APIs or
return empty success. No generic WASM limitation excuses omitting an entire
format or operation without evidence.

## Phase 7: Python

pyo3/abi3, Python>=3.10, package/module fontpixel. Full native APIs plus
fontpixel.compat.bdfparser. Keep handles in Rust and document pixel extraction,
ownership, copies/borrows, integer ranges and synchronous/asynchronous boundaries.
Use bytes/buffers and explicit file/resource sets; filesystem access follows the
caller's paths/resolver authority. Supply type hints, complete API docs and examples.
Adapt original Python tests for compatibility; independently satisfy native coverage.
Benchmark whole workloads and small boundary calls honestly, including slow cases.
Build local manylinux wheel artifacts; do not publish.

## Phase 8: WASM/JS

wasm-bindgen, package fontpixel. Full native API and independent bdfparser-js
compatibility surface. Supply TypeScript types and JS documentation/examples.
Test Node and browsers separately, with explicit complete/minimal feature matrices.
Browser input/output uses Uint8Array/ArrayBuffer and named file/resource collections
instead of implicit arbitrary paths. Support explicit sidecar/directory resolvers
or preloaded files; asynchronous acquisition and Worker/progress adapters must not
hide network/filesystem reads in synchronous WASM calls.

Document and test threading, Promise/synchronous behavior, progress/cancellation,
handle release, memory copies, Number/BigInt precision and host-specific persistence.
Browser output bundles do not imply disk transactions. Preserve native deterministic
results, strict diagnostics and budgets. Run original JS compatibility tests and
the full native matrix; include Node/headless-browser timing, wasm feature-size
and startup measurements. W8.3 adds info/merge/render, Worker/progress and native
byte comparisons; these endpoints are a subset of the full required API.

Phase9 remains excluded. No push, publication or deployment. Long ASAN validation
is explicitly deferred to the owner's computer, remains unperformed, and cannot
be relabelled passed because bindings proceed after phase-6 delivery.
