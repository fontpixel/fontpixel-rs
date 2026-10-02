# Design decisions

## 2026-10-02 — Phase 0 has two sequential work items

**Question:** PLAN §12 calls phase 0 one work item but lists W0.1 and W0.2.
**Choice:** Track and complete both items separately, W0.1 then W0.2. Only their
merged, validated result can close phase 0.
**Reason:** The explicit item list is more precise than the section's count;
charset generation must not be silently omitted or included in W0.1's claims.

## 2026-10-02 — Stage acceptance follows available capabilities

**Question:** Phase-1 acceptance references TTF/SVG parity and OTB performance,
although those codecs are allocated to phase 2.
**Choice:** Following the coordinator's resolved instruction, accept and report
phase 1's implemented capability subset first. Track TTF, SVG, OTB, and any other
later-capability checks as deferred until their implementation phases; retain the
unchanged final full-parity and performance goals.
**Reason:** Avoid a cyclic stage gate without pretending deferred checks passed.
This does not relax the final goals or the requirement to report every limitation.

## 2026-10-02 — One model-independent registry, no pretend font model

**Question:** W0.1 needs read/write contracts while the actual Font/Glyph/Bitmap/
FontSet model belongs to W1.1.
**Choice:** Use `Registry<M>`, `FormatReader<M>`, and `FormatWriter<M>` over a
shared model type. The CLI temporarily uses the uninhabited `Infallible` type.
W1.1 will specialize these contracts with the real `FontSet`; no empty `FontSet`
or successful placeholder parser is exposed.
**Reason:** Exercise real dispatch contracts without implementing W1.1 early or
claiming an empty font is a successful parse. Format-specific option structs are
introduced with their actual formats; the skeleton contains only shared options.

## 2026-10-02 — Multi-resource input and multi-asset output

**Question:** PLAN's convenient byte-vector example cannot represent HBF,
BMFont, image atlases, Plan 9, or Amiga sidecars.
**Choice:** Readers receive primary input plus explicitly scoped resource access;
writes return an in-memory bundle with a primary path and all assets. A single-byte
output convenience method errors when a bundle contains sidecars. Resource paths
are relative, canonical, and validated. Duplicate and file/directory output
collisions fail. The skeleton has no filesystem resolver or output persistence.
**Reason:** Avoid implicit arbitrary file access and silent sidecar loss. Future
filesystem adapters must protect against symlink escape and verify platform path
rules; they must not mistake lexical validation for a filesystem sandbox.

## 2026-10-02 — Strictness checks loss independently from severity

**Question:** A warning can be harmless, while an informational entry may describe
an irreversible approximation. Loss may also occur during reading.
**Choice:** Report entries carry `Impact` separately from `Level`. Strict output
rejects loss in the combined read and write reports. Reader error diagnostics and
writer error diagnostics always fail. Codecs only return in-memory assets; strict
checking precedes handover to a persistence layer.
**Reason:** This implements the no-output-on-loss contract without relying on
warning wording or forgetting earlier loss. Whole-directory atomic persistence
and crash recovery belong to the later real CLI output implementation.

## 2026-10-02 — Catalogue state is independent of support

**Question:** All 46 IDs must be visible even when format features are disabled
or implementations do not exist; extension collisions must remain honest.
**Choice:** Every catalogue entry records feature selection and actual hook
availability separately. In W0.1 every built-in hook is absent, in every feature
configuration. Detectors rank structural evidence; only if none matches does the
registry consider longest-suffix filename hints. Equal matches stay ambiguous.
**Reason:** A selected feature or familiar extension must never masquerade as
implemented parsing. No magic/header parsing is implemented in W0.1. Special
extensionless names (HZK/ASC) and TeX DPI suffixes await their real detectors.

## 2026-10-02 — Minimal dependencies and deferred bindings

**Question:** How much toolchain and dependency setup belongs in the skeleton?
**Choice:** Pin the verified current stable Rust 1.99.0 (edition 2024), install
cargo-deny 0.20.2 from crates.io, and leave the core dependency-free unless `serde`
is selected. CLI uses clap/serde_json. Defaults reserve BDF/PCF/YAFF; each format
and future heavy dependency has a feature slot. Python/WASM crates exist but are
non-publishable reserved crates with no binding functionality or heavy tooling.
**Reason:** Small BDF/PCF builds should remain cheap. Nightly/fuzz, Bun, binding
build tools, and external crosscheck setup are not W0.1 prerequisites. The task's
crates.io/PyPI connectivity was verified with actual downloads, not just HEAD.

## 2026-10-02 — Preserve the full original requirements

**Question:** Should delivery depend on the surrounding task package?
**Choice:** Copy original PLAN.md and FORMATS.md into the repository unchanged.
Catalogue tests read the checked-in FORMATS table, including split `pk`/`gf` IDs.
**Reason:** A cloned bundle can inspect the scope and execute catalogue tests
without private absolute paths or the original package directory.

## 2026-10-02 — CLI documentation avoids Cargo's duplicate target path

**Question:** Both the library and CLI binary are named `fontpixel`; Cargo doc
warns that they overwrite the same generated documentation path.
**Choice:** Set `doc = false` only on the CLI binary target; library API rustdoc
remains enabled. CLI help is generated by clap and command usage lives in README.
**Reason:** Preserve correct library docs and a warning-free documentation build
without renaming the mandated crate or executable.

## 2026-10-02 — Exact charset tables with explicit feature failures

**Question:** How can small builds preserve correctness without compiling CJK?
**Choice:** Check in a self-contained Python-codec generator, registry metadata,
35 unique core tables and 12 unique CJK tables. Use sorted disjoint ranges plus
packed arrays in both directions, with Python's strict encoder preference rather
than a smallest-code reverse map. Metadata stays visible without `cjk-charsets`,
but opening a CJK codec returns an explicit feature error. No runtime fallback.
**Reason:** Meet table-exact parity and O(log n) lookup without runtime allocation,
large dependencies, or claiming approximate support. Generated drift is a CI error.

## 2026-10-02 — Per-glyph codes are separate from stateful encoding streams

**Question:** PLAN mentions ISO 2022 and GB18030, which are not simple byte maps.
**Choice:** Implement SPEC's `iso2022cn` as GB2312 GL glyph codes, with an explicit
GL override matching Python. Do not claim ISO-2022 escape/shift stream support;
HZ and non-table ISO-2022 names remain unknown. GB18030 enumerates every four-byte
candidate at generation time into 207 exact algorithmic runs, with byte validation
and binary search in both directions. EUC-JP three-byte and multi-character HKSCS
sequences remain unmapped under the SPEC contract.
**Reason:** A glyph integer cannot carry stream state. This implements the exact
reference API and all named table keys without disguising a partial stream codec.

## 2026-10-02 — Preserve numeric surrogate behavior without invalid Rust chars

**Question:** Python Unicode identity permits surrogate code points; Rust `char`
does not. Unknown charsets also require a warning owned by the future font layer.
**Choice:** Expose exact numeric `cp2unicode`/`unicode2cp` over `u32` and scalar
`cp2char`/`char2cp` conveniences. Numeric identity preserves surrogates; scalar
conversion rejects them. Unknown name resolution exposes `known() == false` and
uses identity; W1.5 must integrate the warning-once policy with its Report API.
**Reason:** Keep Python parity possible without manufacturing invalid Rust values
or introducing process-global warning state before the font/compatibility layer.

## 2026-10-02 — Font charset metadata must not guess context

**Question:** FNT DEFAULT/OEM/MAC/SYMBOL charset bytes do not uniquely specify a
Unicode map; Windows CJK code pages differ from similarly named standard sets.
**Choice:** Provide exact fixed Windows code-page resolutions, explicit contextual
variants for DEFAULT/OEM/MAC/SYMBOL, and unknown for undocumented values. Provide
94×94 grid helpers for HBF/HZK and distinct Shift-JIS/CP932 tables for FONTX2.
**Reason:** Preserve ambiguity for the real format codec and caller to resolve;
never quietly substitute the host's locale, ASCII, CP437, or a symbol mapping.
