# Design decisions

## 2026-10-04 — Keep decoding diagnostics when a writer rejects output

`Registry::write_with_report` appends a writer's `ReportedErrors` or `StrictLoss`
report to the preceding read report before returning it. Early read-loss strict
rejection still happens before calling the writer. Other typed errors retain
their existing identity. A regression exercises both writer error variants and
checks that the earlier repair appears first; no output reaches persistence.

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

## 2026-10-02 — W1.1 bounded generic raster and explicit label palettes

**Question:** PLAN requires packed 1-bit plus Gray8, while SPEC effects need a
third value (label 2) that cannot fit in one bit.
**Choice:** Implement `Bitmap<Mono>` with canonical per-row MSB-packed bytes and
`Bitmap<Gray8>` with byte samples. All conversions, thresholds and palettes are
explicit. A compatibility raster may carry 0/1/2 labels in Gray8; shadow/glow and
binary inversion accept explicit sample values. Gray tone negation is separate.
**Reason:** Preserve the labels exactly without a second competing raster engine,
unrequested grayscale quantization, or weakening the compact Mono representation.
Default allocation/dimension budgets propagate through derived bitmaps; codecs
must tighten them to their input budgets before allocating.

## 2026-10-02 — Core geometry is fallible and dimension-preserving

**Question:** Rust native API need not inherit ambiguous Python list shapes or
allocation-hiding operators; some SPEC operations mutate into nonrectangular data.
**Choice:** Dimension-changing operations return `Result<Bitmap>`; both dimensions
are preserved even when area is zero. Pixel/paste coordinates are top-left, crop/
shift/bbox are bottom-left as SPEC specifies. Pattern replacement rejects ragged
results. Integer enlarge requires positive factors. Numeric row values use u64
with an explicit error above 64 bits and lossless arbitrary-width hex available.
**Reason:** Keep native invariants/overflow safety clear. W1.5 owns mutating/chaining,
Python zero-height normalization, <=1 enlarge handling, warnings and language
numeric serialization. These are documented adapter work, not a claim that the
full bdfparser facade or full phase-1 parity is already finished.

## 2026-10-02 — Preserve file order, duplicate mappings and stable glyph IDs

**Question:** Multiple encodings/sequences and duplicate source codes do not fit a
single code-to-glyph map; unrestricted mutable access could leave a stale cmap.
**Choice:** Font keeps stable file-order slots plus ordered BTree indexes of all
code/sequence mappings. Effective lookup uses the last file-order glyph, with all
IDs exposed for diagnostics. Validated replace/edit methods update indexes;
deleting a glyph does not renumber saved IDs. Glyph Unicode/legacy domains are
explicit; legacy identity comes from its FontCharset. Ordered raw properties keep
original case, duplicate keys, typed/opaque values and optional exact record bytes.
**Reason:** Support lossless readers and predictable O(log n) lookup/ranges without
silently deleting duplicate or unencoded glyphs. Writers reconcile raw provenance
with edited semantic fields; raw bytes must not override a subsequent edit.

## 2026-10-02 — Reuse real-model registry contracts without inventing codecs

**Question:** W0 deliberately made `Registry<M>` independent of the future model.
**Choice:** Supply `FontRegistry = Registry<FontSet>`, public built-in `registry`,
`read` and `write` entry points, and switch CLI dispatch to the real model. No
built-in capability hooks become live in W1.1. Synthetic real-model tests exercise
sidecar absence, multi-asset refusal and strict loss alongside existing contracts.
**Reason:** W1.2 can register its actual BDF hooks here, retaining all W0 safety and
reporting behavior; the data model alone is not evidence of format support.

## 2026-10-02 — Benchmark dependencies are development-only

**Question:** PLAN asks for Criterion while BDF/PCF-only production builds must stay
small and dependency-light.
**Choice:** Add Criterion 0.8.2 as a dev dependency, default features disabled,
with no plotter/rayon production footprint. Include a separate five-batch
wall-clock executable and matching Python benchmark; commit measurements and
platform details. Normal no-default-feature dependency tree remains empty.
**Reason:** Quantify core hotspots reproducibly without implying BDF/PCF parsing or
future whole-font performance targets have been reached.

## 2026-10-02 — Preserve Unicode-tagged invalid source numbers

**Question:** The full reference parity generator deliberately puts 0x110000 and
0x7FFFFFFF in Unicode BDF fonts; rejecting such aliases in the model would prevent
later codecs from preserving/querying them as the Python reference does.
**Choice:** `Codepoint::Unicode(u32)` tags the source's declared encoding, without
asserting scalar validity. It retains all u32 numbers, including surrogates and
out-of-range source codes. `scalar()` rejects these for character use. Actual
Unicode sequences retain their numeric range validation; negative BDF ENCODING
values are represented as unencoded glyphs plus preserved source properties.
**Reason:** Preserve tolerant input information and defer format-specific legality
and loss diagnostics to readers/writers. This does not make an invalid number a
valid Unicode character or bypass W0.2's strict charset conversion rules.

## 2026-10-02 — BDF source fidelity versus canonical SPEC behavior

**Question:** Core preserves duplicates and rectangular pixels, while SPEC chooses
last duplicate mappings, keeps raw hexadecimal rows and defers malformed-row
errors until drawing; the catalogue parser instead chooses first duplicates.
**Choice:** Follow SPEC/Python for parsing and canonical writing. Retain source
records and field-level bitmap provenance in reserved `bdf.*` properties. Keep
source duplicates in the core but select only effective mappings in SPEC output.
Reconcile semantic edits against source snapshots; never return a saved whole
input file. Materialization repairs are separately identified from the exact
Python parser warnings; invalid hex remains round-trippable but a public source
validation guard fails before consumers claim a drawable glyph.
**Reason:** Preserve information and future compatibility without weakening the
core rectangle invariant or silently substituting a different BDF dialect.

## 2026-10-02 — Gzip is a transport layer, not a codec capability

**Question:** BDF and later PCF need gzip without duplicating decompression or
advertising nonexistent PCF support.
**Choice:** Bounded multi-member gzip decoding is shared in ReadContext behind
`gzip`; BDF enables that small pure-Rust dependency. Registry output applies a
deterministic gzip envelope only to a single `.gz` primary asset. Codec hooks
still advertise only actually implemented formats.
**Reason:** Enforce decompression budgets once before content detection and keep
strict output validation before any file is persisted.

## 2026-10-02 — Isolate libFuzzer's license and sandbox limitations

**Question:** `libfuzzer-sys` carries LLVM's permissive NCSA notice in addition to
MIT/Apache, and this sandbox does not allow LeakSanitizer's exit-time ptrace check.
**Choice:** Keep the production deny policy unchanged. A separate fuzz-only deny
config permits the BSD-style NCSA license, with the same advisory/source/ban
checks and no wildcard dependencies. Run address-sanitized fuzzing with leak
checking disabled, recording the initial infrastructure failure and clean reruns.
**Reason:** Validate the actual isolated dependency graph without hiding a license
or changing operating-system security settings. No memory-leak certification is
claimed from these runs.

## 2026-10-02 — PCF wire fidelity and safe table accounting

**Question:** PCF includes independent table formats, optional records, and the
bdftopcf convention that the last accelerator's TOC size exceeds EOF.
**Choice:** Use checked bounded table cursors; permit only accelerator padding to
be absent, never required fields. Enforce counts, slices, glyph pixels, table and
model limits before expansion. Parse and preserve ink metrics/attributes and
unknown-table provenance beyond SPEC's minimum projection. Recompute output from
actual fields; diagnose overwritten opaque metadata and discarded duplicate
records. A direct reader consumes uncompressed PCF; registry gzip remains shared.
**Reason:** Match all supplied wire variants without trusting malformed lengths
or confusing a cached source file with a real editable serializer.

## 2026-10-02 — Share X11 field projection without enabling BDF capabilities

**Question:** PCF uses the BDF-compatible model, field snapshots and source hex
interpretation, but `pcf` alone must not depend on the BDF feature.
**Choice:** Move existing metadata/provenance helpers into an internal `x11`
module, retaining BDF's public API and behavior. Register codecs independently.
Reconcile PCF FONT, point-size and resolution properties when normalized fields
change; otherwise honor direct property edits. Use BDF source hex only while its
pixel/dimension snapshot matches; diagnose explicitly lenient nibble decoding.
**Reason:** Avoid a second model, duplicated edit rules, false feature advertising,
or interpreting an invalid BDF raster's placeholder bytes as validated pixels.

## 2026-10-02 — PCF aliases, compiler quirks and independent oracle limits

**Question:** The core can retain shared glyph identity, while SPEC's legacy map
expands aliases and reorders unencoded glyphs; some compiler flags are broken.
**Choice:** Preserve PCF aliases as shared glyph indices. BDF-origin canonical
output selects effective mappings then unencoded records; report omissions and
alternate-marker loss. Default serialization follows bdftopcf exactly. Follow
Python's corrected pad-8 flag and final scan-unit swap for nondefault options;
reject swap units larger than row padding. Retain input metrics/attributes but
report any recomputed ink/accelerator differences and unsupported vertical fields.
**Reason:** Honor the real model and SPEC rather than reproducing undefined memory
access or corrupting terminal pixels for a tool bug. monobit 0.54 also reverses
its two-byte encoding-table axes; record those independent-tool mismatches,
verify all its glyph pixels/metrics, and separately verify exact one-byte codes.

## 2026-10-02 — Native rendering preserves SPEC's cell and direction rules

**Question:** PLAN calls mode 0 ink concatenation, while the supplied Python
renderer uses full font cells and width-based line limits even for vertical text.
**Choice:** Keep `Advance`/`Cell` exact to Python, add separate native `Ink`, and
expose signed letter/line gaps without changing defaults. All sixteen independent
direction pairs, current/prior spacing, missing zero-advance cells, effective
mapping order and zero-area normalization follow executable oracle cases.
**Reason:** Preserve compatibility without silently changing historical layouts
or preempting the later full bdfparser facade's warning/exception contract.

## 2026-10-02 — Rendering borrows packed rasters and reports bounded loss

**Question:** Per-glyph/line temporary bitmaps and linear diagnostic deduplication
make large strings expensive; malformed BDF placeholders must never look valid.
**Choice:** Store borrowed placements, blit clipped packed row regions into one
result, validate unchanged BDF source rows, and report missing, clipped or
thresholded content. Separately budget layout and diagnostic storage; use fallible
hash deduplication and bounded diagnostic names. CLI strictness includes read loss
before any text or atomically replaced image output is emitted.
**Reason:** Meet the no-per-glyph-raster-allocation goal while bounding adversarial
input and retaining deterministic, inspectable information-loss reporting.

## 2026-10-02 — Image rendering does not advertise later font codecs

**Question:** W1.4 requires PNG/SVG images before W2.8 SVG fonts and W3.3 image atlases.
**Choice:** Add a small independent `png` export feature and dependency-free SVG
run paths, both with typed palette alpha and integer scale. SVG uses disjoint
paper/ink paths so alpha matches PNG. Empty PNG output fails explicitly. Leave
catalogue `image`/`svg` hooks absent; contour/dot/SVG-font serialization stays W2.8.
**Reason:** Deliver the requested render commands without falsely claiming future
font codec or byte-for-byte contour serialization capability.

## 2026-10-02 — Full phase-1 parity brings forward narrow compatibility dependencies

**Question:** W1.5/§9.2 requires the entire default differential suite, but that
suite includes TTF and SVG/outline methods otherwise assigned to W2.2/W2.8.
**Choice:** Implement the exact SPEC outline, SVG-font and glyf/TrueType reader/
writer inside `compat::bdfparser`, behind `compat-bdfparser`, including BDFP
metadata and composite glyph reading. Do not register native `ttf`/`svg` codecs.
**Reason:** The stronger phase-1 completion gate must not be weakened by skipping
operations or substituting Python. W2 still owns native sfnt architecture, CFF,
embedded bitmaps, variants, general SVG-format parsing, independent external
crosschecks and integration; it may reuse these tested compatibility algorithms.

## 2026-10-02 — Dynamic SPEC metadata lives on the native model

**Question:** SPEC permits independent null metric components and edits through
stale attached handles, while native metrics are typed vectors with stable IDs.
**Choice:** Keep one native Font as the authority. Store dynamic 17-slot metadata
only as native glyph properties when needed; retain raw BDF bitmap provenance.
Generation-checked handles hold snapshots and merge only their changed fields.
Projection materializes partial metrics for native rendering/PCF without altering
SPEC metadata. PCF writes preserve numeric encodings, and an `encoding()` override
never changes serialized properties. Recompute native derived metadata before
snapshot reconciliation, preserving source properties such as `PIXEL_SIZE -1`.
**Reason:** Avoid a second competing font implementation and accidental charset
conversion or source-property loss. Python's actual removeglyph behavior is used:
removed/replaced attached handles cannot remove replacements, but a freshly
cloned detached handle removes by codepoint. This is subtler than §11.4's prose.

## 2026-10-02 — Rust-specific known-issue assessment and UTF-16 transport

**Question:** JS's wide-integer and ragged-array exceptions do not transfer
unchanged to Rust; Python JSON also allows lone UTF-16 surrogates.
**Choice:** K11 is not exempt: arbitrary-width numeric bitmap rows are exact
serde_json integers. The runner preserves lone surrogate code units and uses
numeric charset APIs instead of manufacturing invalid Rust strings. Scalar
cp2char remains a Rust char convenience; cp2unicode retains every numeric value.
K12 applies only to invalid ragged construction/replacement, which Rust rejects
transactionally to retain the native rectangle invariant. Its matcher checks the
specific error and trigger, not arbitrary case failures. All other historical
findings remain strict regressions, including state after exceptions.
**Reason:** Preserve exact valid-domain comparison without importing JavaScript
precision loss or undefined ragged behavior. Diagnostic JSON is ASCII-escaped so
printing a surrogate-containing failing case cannot crash the test harness.

## 2026-10-02 — Exact JSON integers retain original fixture numeric semantics

**Question:** `serde_json/arbitrary_precision` feature unification changes existing
float equality from parsed f64 equality to lexical decimal equality (for example,
`0.00001` versus `1e-05`) and makes an untyped empty-byte assertion ambiguous.
**Choice:** The pre-existing BDF fixture test explicitly reconstructs only its
STARTFONT/version JSON number from f64, restoring its original semantics. Exact
integer comparisons and all serialized BDF bytes stay unchanged. The empty byte
assertion uses `is_empty()`. New compatibility numeric rows retain unbounded exact
integers, and parity continues comparing exact canonical strings/hashes.
**Reason:** Keep earlier validation at its original strength while avoiding a
production global JSON feature that silently alters test meaning. No float
precision tolerance or expected-output change is introduced.

## 2026-10-02 — YAFF is interoperable text plus versioned lossless metadata

**Question:** YAFF's portable metrics cannot distinguish absent from explicit-zero
vectors, retain all typed/raw source properties, mixed pixel depths, stable deleted
slots, or the complete native charset and font/container model.
**Choice:** Emit ordinary YAFF 1.0 labels, visible pixels and horizontal metrics,
plus version-1 `fontpixel.font`, `fontpixel.glyph` and `fontpixel.container`
JSON properties, using YAFF's allowed namespaced extension mechanism. Pixels
remain solely in the visible raster. Metadata serialization borrows the model,
is byte-bounded, and escapes non-ASCII/DEL source text to legal ASCII JSON.
Restore exact source fields only while their portable projection is unchanged;
portable edits take precedence, including raster resizing. Preserve empty sets,
empty strikes and zero-area dimensions. All optional derived serde support is
behind `yaff`; BDF/PCF-only builds gain no JSON/serde production dependency.
**Reason:** The mandated golden format must be exact and diffable while monobit
can still read its meaningful labels, pixels and horizontal spacing. A third-party
rewrite may discard extension-only data; document this explicitly instead of
silently weakening native roundtrip equality or treating original bytes as an
uneditable opaque font payload.

## 2026-10-02 — YAFF parsing rejects malformed structure and keeps unknown semantics

**Question:** monobit tolerates ambiguous legacy labels and supports experimental
stroke/vertical/kerning fields beyond this phase's native semantic model.
**Choice:** Accept legacy labels/offset/tracking and documented gray levels;
retain ordered properties, duplicate fields and comments. Normalize supported
horizontal fields with the specification's additive global/per-glyph metrics.
Retain unsupported vertical/kerning fields as metadata; reject path-only glyphs
rather than invent pixels. Invalid UTF-8/control/noncharacter input, malformed
numerics, inconsistent rasters, extension versions and resource overflow fail.
**Reason:** Preserve information without claiming a later-phase stroke renderer.
Semantic fields plus original property records are maintained independently, as
with earlier BDF provenance. Property/JSON/raster row limits and cumulative model
budgets precede substantial allocation; global metrics are parsed only once.

## 2026-10-02 — Native sfnt tables are a bounded layer, not a premature codec

**Question:** W2.1 owns common sfnt tables and reading while actual glyph/image
extraction and FontSet projection belong to later OpenType work items.
**Choice:** Expose `sfnt::{Sfnt,SfntBuilder}` and editable common-table APIs behind
an optional `sfnt` feature. Use Fontations read-fonts for typed reading, with
explicit bounds/count validation before expansion. Keep opaque table bytes
accessible. Reuse only assembly/checksum mechanics in the compatibility TTF
writer; retain all of its table-specific byte policies and do not register any
new native font format.
**Reason:** Give W2.2–W2.7 a real production foundation without pretending a
container parser is already a bitmap-font decoder. BDF/PCF-only production builds
remain independent of Fontations; full existing parity stays a regression gate.

## 2026-10-02 — Unicode cmap completeness and explicit legacy fallback

**Question:** A format-4 cmap's 16-bit length cannot represent every valid Unicode
mapping, and the existing compatibility path has historical truncation rules.
**Choice:** Native output always carries the complete scalar-to-glyph mapping in
format 12. Also emit a format 4 when the bounded segment/glyph-array encoding
fits, returning an explicit flag when omitted. Alias mappings and supplementary
scalars are retained; invalid scalars/IDs error. Native reading deliberately
selects a Unicode nominal subtable, never a symbol/Macintosh map; legacy and UVS
subtables remain available in the raw/Fontations interface.
**Reason:** Avoid silent loss while keeping the unchanged SPEC byte contract in
its own compatibility path. Nominal-map projection is not full cmap preservation.

## 2026-10-02 — OS/2 coverage is explicit rather than heuristic licensing risk

**Question:** “Functional code page” is left to the designer by OpenType; the
fontTools representative-character heuristic is translated from FontForge.
**Choice:** Use the permissive public Unicode-block data from fontTools, including
all supplementary bit-57 behavior, and an independently written optional
full-printable-repertoire code-page check over native charset tables. Unsupported
or uncompiled requested pages error. Keep raw OS/2 coverage fields editable for a
caller's documented policy. Do not copy FontForge code or its translation.
**Reason:** Provide deterministic defensible range calculation under the project's
license constraints, without confusing a single representative glyph or block
intersection with functional legacy coverage or shaping support.

## 2026-10-02 — Fontations lazy arrays need independent completeness checks

**Question:** Fontations accepts minimum table headers and returns empty arrays
for some truncated optional ranges; its bitflag readers also discard unknown bits.
**Choice:** Validate declared counts against complete array lengths before indexing
or expanding. Read preservation-critical raw flags at checked offsets. Keep a
libFuzzer-discovered truncated-format-4 regression and reserved-gasp-flag tests.
**Reason:** A typed reader is not a whole-font validator. Explicit preflight guards
prevent panics and prevent malformed arrays or unknown flag bits from silently
turning into apparently valid empty data.

## 2026-10-03 — W2.2 keeps native geometry and SPEC wire contracts explicit

**Question:** SPEC circles and Tong's round-dot export use different quadratic
approximations, while `ttf` also promises embedded strikes and general pixel-grid
reading assigned to later work items.
**Choice:** Share the exact SPEC merged-square tracer with a dependency-free
native outline API. Expose independent squares and Tong's four-quadratic dots;
use one shared dot glyph and typed composite components for native TrueType.
Keep the existing SPEC TTF serializer's point representation and bytes unchanged.
Do not register the native `ttf` format until its full read/write contract exists.
**Reason:** Native glyph-level equivalence and exact compatibility are distinct
requirements; neither should be implemented by silently substituting the other.

## 2026-10-03 — Editable glyf is unhinted geometry, not an interpreter

**Question:** Composite glyphs can carry bytecode, device rounding, metrics flags,
phantom-point attachment and ambiguous default offset scaling.
**Choice:** Retain bytecode and all defined semantic component flags. Resolve
ordinary point attachment, F2DOT14 transforms and explicitly scaled offsets;
otherwise use the recommended unscaled offset convention. Do not run instructions
or device grid fitting. Reject phantom-point attachment without the required
metrics/interpreter context. Recompute bounds and geometry maxima on output;
provide maxp generation only for unhinted glyphs, preserving caller control over
hint interpreter budgets. Apply per-glyph and cumulative byte/point/component/
instruction budgets, cycle/depth validation and visit budgets including empty
component graphs. Strengthen the compatibility parser's glyph-slice boundaries,
flag runs, cmap expansion and raster work without changing valid-font results.
**Reason:** Honest, bounded design-space behavior is safer than approximating
hinted outlines or treating a typed lazy reader as complete validation.
The compatibility reader also charges cumulative whole-font pixels, estimated
model/provenance bytes, expanded points and graph visits. Counting row overhead
covers zero-width, very tall metadata rectangles that evade an area-only limit.

## 2026-10-03 — CFF1 is explicit cubic geometry, with a strict Fontations boundary

**Question:** W2.3 requires a minimal CFF encoder and reading, while Fontations'
locked evaluator accepts legacy recovery and silently ignores several Type 2
arithmetic operators; native FontSet/raster and bitmap-strike work comes later.
**Choice:** Provide a feature-gated CFF1 table API and configurable shared cubic
dot subroutine. Reuse native bitmap tracing/origins and sfnt assembly. Preflight
INDEX/DICT/charset/FDSelect structures and Type 2 execution before Fontations
interpretation, applying per-glyph and cumulative work/geometry budgets. Read
SID and CID fonts, hints as unhinted geometry, local/global subrs and the normal
line/cubic/flex operators. Explicitly reject unsupported legacy arithmetic,
seac, endchar inside subrs, CFF2, and unrepresentable signed-16.16 arithmetic.
Retain fractions in design space rather than using FreeType-like integer draw
rounding. Emit canonical unhinted name-keyed CFF1; expose original bytes separately.
Keep `otf` registry hooks absent until W2.4/W2.7 finish the promised native codec.
**Reason:** Validated geometry is useful now without confusing a table parser with
a bitmap font decoder or silently approximating unsupported programs. Fontations
already contains the necessary interpreter, so no new dependency is needed.
The Type 2 numeric range is checked independently from DICT's 32-bit integers;
large widths, coordinates, displacements and custom-SID exhaustion error rather
than truncate. This is an explicit boundary, not a claim to execute arbitrary
PostScript or all legacy CFF1 programs.

## 2026-10-03 — Embedded table codecs and native registry scope

**Question:** W2.4 needs complete native bitmap extraction without claiming that
outline rasterization, collections, resource forks or color writing already exist.
**Choice:** Register OTB and bdat detect/read/write and color detect/read only.
Expose glyph-ID-based `sfnt::bitmap` APIs for attaching bitmap pairs to existing
outline fonts; native writes are bitmap-only sfnt. Keep TTF/OTF/vector-raster/TTC
hooks absent until their full native contracts are implemented. A source with
multiple bitmap families can require an explicit format. Preserve sparse physical
strikes; EBSC remains editable scaling instructions rather than invented pixels.
**Reason:** Table parsing and true FontSet codecs are different capabilities.
Errors and conversion diagnostics preserve this distinction and strict-mode loss.

## 2026-10-03 — Color coverage, sbix duplicates and composite arithmetic

**Question:** The model has Mono/Gray8 but no RGBA, and duplicate/composite behavior
is incompletely described in the public chapters.
**Choice:** Default color extraction is alpha coverage. Offer an explicit
alpha-weighted-darkness alternative and explicit Mono threshold; always report
color projection loss. Decode PNG and premultiplied BGRA; reject unsupported image
tags. sbix duplicates reuse the target's image placement, matching measured
FreeType behavior even when the duplicate's own offsets differ. Nonempty glyf
origins are separately added for the current glyph. Composite grayscale samples
combine with bitwise OR, not max; the 0x55/0xAA overlap is independently verified
as 0xFF by FreeType. Darkness projection of 32-bit BGRA composites is rejected
because projecting components first cannot reproduce RGB-space composition.
**Reason:** Avoid silently inventing color preservation or an unverified blend.
The first implementation used max for gray overlap; an explicit independent-tool
review caught this and added a permanent regression before acceptance.

## 2026-10-03 — Narrow PNG dependency duplication, no advisory exception

**Question:** Cached, verified png 0.18.1 requires miniz_oxide ^0.8 directly while
existing flate2 1.1.10 requires ^0.9; the repository denies duplicate production
versions by default.
**Choice:** Add optional png only to the color feature and one exact cargo-deny
bans exception for miniz_oxide =0.8.9. Keep flate2 current; neither advisory nor
license checks are relaxed. BDF/PCF-only and default normal dependency graphs
remain free of PNG/Fontations unless separately selected.
**Reason:** Prefer a maintained bounded PNG implementation over a new homemade
codec or an unrelated decompressor downgrade. This is an explicit dependency
policy decision; the exception can be removed when png updates its dependency.

## 2026-10-03 — Independent bitmap-tool limitations remain visible

**Question:** System FreeType 2.13.3 extracts non-byte-aligned gray rows differently
from OpenType/fontTools; bundled FreeType lacks PNG. OTS rejects bitmap-only fonts
and strips bitmap tables from otherwise accepted outline-bearing fonts.
**Choice:** Use the installed PNG-enabled system FreeType for color verification.
For width-3 bit-aligned depth-2/4 images, assert the exact known FreeType shifted
row result and independently require native/fontTools/spec packing equality.
Keep all byte-aligned native writer pixels strict. Record OTS's acceptance of
native common tables with valid outlines, bitmap stripping, and bitmap-only
rejection separately. monobit bdat+EBSC generation raises KeyError('EBLC'); test
monobit bdat without EBSC and independently test our shared EBSC API.
**Reason:** Do not alter valid pixels to mimic an external bug or portray OTS
container acceptance as bitmap-table validation.

## 2026-10-03 — WOFF containers remain separate from outline model projection

**Question:** W2.5 must read/write webfonts while general TTF/OTF raster and TTC
support are explicitly assigned to W2.7 and not yet registered.
**Choice:** Add real, public, bounded sfnt-to-WOFF1/WOFF2 codecs, retaining all
opaque tables and auxiliary blocks. Do not register partial FontSet codecs.
Treat collections as the W2.7 integration dependency and reject them explicitly.
**Reason:** Complete standalone webfont coding can be tested independently without
pretending that decoding a container already performs general outline rasterization.

## 2026-10-03 — WOFF2 transforms preserve font semantics and disclose wire changes

**Question:** WOFF2 reconstructs glyph encodings, invalidates signatures, and has
original-size fields that are advisory rather than allocation guarantees.
**Choice:** Implement glyf/loca and optional hmtx transforms directly with bounded
streams. Preserve original bounding boxes, instructions, components and overlaps;
remove DSIG with a loss report. Reject nonempty programs on zero-contour glyphs
for transformed output rather than silently discarding them. Offer explicit null
transforms for comparison/fidelity. Validate actual output sizes, not advisory
original sizes. Retain optional metadata/private data outside sfnt.
**Reason:** Honor the W3C functional-fidelity contract and explicit loss reporting,
without conflating canonical encoding changes with missing glyph information.

## 2026-10-03 — Normalize WOFF2 transform input for deployed decoder compatibility

**Question:** W3C makes original glyph/font lengths advisory, but Google WOFF2
(as used by OTS) bounds its output using totalSfntSize and FreeType consults the
original glyf length after reconstructing four-byte-aligned glyph records.
**Choice:** Normalize transformed input glyph records to four-byte boundaries,
upgrade short loca when necessary, then record those true normalized lengths.
Pad the compressed block to four bytes even without auxiliary blocks. Keep the
reader standards-compliant when valid reconstructed sizes differ from headers.
**Reason:** An initial unnormalized output had exact fontTools glyphs but OTS
failed and FreeType's last glyph was invalid; changing the advisory lengths to
the normalized input sizes fixed both independent consumers. This preserves
font functionality and makes output interoperable, rather than weakening tests.

## 2026-10-03 — Avoid optional hmtx transform for nonminimal long-metric counts

**Question:** fontTools 4.66.1 recompiles transformed hmtx to its minimum count
but leaves the exposed hhea count unchanged; legal sfnt fonts with redundant
trailing long metrics then fail its normal TTFont reader.
**Choice:** Preserve those hmtx bytes using the legal null transform. Continue
using transform 1 when arrays match and the source long-metric count is minimal;
our decoder reconstructs the full declared count even when it is nonminimal.
**Reason:** hmtx transformation is optional; preserving source tables avoids a
consumer bug without silently changing metrics or reducing glyf/loca coverage.

## 2026-10-03 — Keep external verification strict while avoiding oracle tail copies

**Question:** Stock fontTools' WOFF2 composite decoder repeatedly copies the
entire remaining byte stream for every component. The full-SC shared-dot font
has millions of components; its correctness run had not finished its first equivalence pass after an
approximately 20-minute total observation window.
**Choice:** Preserve that censored stock-run evidence. Add an isolated,
verification-only read-only memoryview wrapper around compositeStream, still
calling the original component decoder. Prove its reconstructed bytes identical
on simple, composite, Latin, hinted, affine, explicit-bbox and overlap fixtures;
then keep all full-SC assertions. Independently corroborate with Google's C++
decoder, FreeType, OTS and HarfBuzz. Use stock fontTools only for benchmarks;
measure Latin-dot same-input three-way medians separately from full-SC native/C++
(and full-SC CFF three-way) medians.
**Reason:** Remove only an oracle transport-copy bottleneck, without modifying
installed/reference code, weakening correctness checks, presenting an adapted
oracle as stock, or mixing unlike workloads in speedup ratios.

## 2026-10-03 — Bound retained multi-font results as well as worker count

**Question:** Per-font and worker limits alone do not bound a caller's very long
input list or the accumulated output/auxiliary blocks of a batch.
**Choice:** Add configurable batch font-count and aggregate input/output budgets
(default 1024 fonts and 256 MiB each for input and retained output). Check before
queue allocation; stop admitting new jobs on error, join in-flight jobs, and
return no partial output. Preserve deterministic input order for successful jobs.
**Reason:** A valid small-font array should not bypass resource accounting by
repetition. These guards do not change serialized bytes or any reader path.

## 2026-10-03 — Preserve WOFF1 physical ordering and signature validity boundaries

**Question:** Glyph/OTS interoperability alone does not detect WOFF1 §6's input
physical-table-order requirement. Using the general alphabetically ordered sfnt
builder would change a legal physical layout and could invalidate DSIG.
**Choice:** Keep tag-sorted directories and input physical payload order in both
WOFF1 directions with a bounded order-preserving assembler. Roundtrip well-formed
sfnt bytes exactly. Report nonzero non-table data loss and structural repairs;
remove a signature invalidated by normalization with a loss entry. Test a valid
reverse-physical-order font carrying DSIG, plus hidden data/checksum repairs.
**Reason:** Meet the normative packaging rule rather than mistaking identical
rendering for exact envelope fidelity. WOFF2 algorithms and timings are unchanged.


## 2026-10-03 — W2.5 review closes mandatory webfont conformance gaps

**Question:** Independent specification review found that candidate `cc5ba81`
normalized invalid WOFF1 source checksums, accepted missing final WOFF1 table
padding, and set WOFF2 head.flags bit 11 only when a table transform was used.
**Choice:** Reject invalid source table and whole-font checksums before WOFF1
packaging; continue reporting normalization only for checksum-valid structural
anomalies and remove any resulting stale DSIG. Report incoming WOFF1 checksum-adjustment repairs even without a DSIG.
Require the last WOFF1 table's complete zero padding separately from WOFF2's optional final block padding.
Set bit 11 for every WOFF2 output, including CFF and null transforms. Preserve
all earlier raw evidence as candidate observations and rerun affected validation
and representative final-code performance measurements in separate files.
**Reason:** WOFF1 §5 has an explicit MUST-reject checksum requirement; its later
advisory wording about correcting checksums is not a reason to weaken the strict
encoding boundary. WOFF1 §5/§6 mandates table padding, including the final table.
WOFF2 §5 requires the head flag independently of whether glyf/hmtx is transformed.
Byte comparisons now construct exactly the specification-mandated head change;
all other null-transform bytes remain exact, and no glyph/pixel assertions relax.
Sources: https://www.w3.org/TR/WOFF/#TableDirectory and
https://www.w3.org/TR/WOFF2/#table_format.

## 2026-10-03 — W2.6 scopes layout and subset preservation explicitly

**Question:** General OpenType tables can contain hidden glyph references, while
W2.6 specifically requires regional single substitutions and useful web subsets.
**Choice:** Construct and read bounded GSUB single substitutions directly; close
and compact glyph IDs together with every supported glyph-indexed table. Reject
unhandled layout/table forms by default rather than copying stale IDs. Retain
all name bytes, deterministic ordering, and diagnostics through sfnt/web wrappers.
Keep W2.7 collection and general raster-reader registration separate.
**Reason:** A small correct subset must remain shape-equivalent and must not
silently corrupt an opaque glyph-indexed table. Explicit errors preserve the
source when a broader codec is required.

## 2026-10-03 — Subsets preserve raw TrueType/CFF hinting semantics

**Question:** Re-encoding geometry is visually equivalent unhinted, but an
originally padded glyph bounding box can influence TrueType phantom points and
hint execution. CFF re-vectorization would also discard hints and subroutines.
**Choice:** Validate TrueType glyphs using the existing bounded reader, retain
selected original glyph records byte-for-byte, and patch only composite child
IDs. Retain conservative original maxp maxima and global bounds; rebuild loca.
Rebuild CFF offsets/selection while retaining raw programs, all subroutines,
private hints, widths, matrices, SIDs/CIDs and strings. Keep unsupported encodings
explicit rather than dropping them. Compare hinted external fixtures as well as
native unhinted regional fonts.
**Reason:** Subsetting changes coverage and IDs; it should not silently alter a
retained glyph's hinting program, original box, component flags or subroutine
semantics. Unused CFF subroutines/strings can remain, trading some size for fidelity.

## 2026-10-03 — Relocate even unused CID Top DICT Private blocks

**Question:** A valid FontTools CID font can retain a Top DICT Private entry in
addition to FD-specific Private dictionaries. Internal outline execution ignores
that top-level block, while OTS still validates its offsets.
**Choice:** Preserve and relocate that optional block and its local-subroutine
INDEX as well. A fresh external CID fixture exposed the stale-offset bug; its
source passes OTS, so rejecting or weakening the oracle was not appropriate.
**Reason:** A preserved dictionary containing a stale absolute offset is not
preserved valid data. Hint/outline equivalence alone did not detect this failure.

## 2026-10-03 — Canonicalize missing DFLT default language systems

**Question:** The external FontTools/Tong FEA compiler can produce a DFLT script
with language-specific features but a NULL DefaultLangSys. HarfBuzz shapes it,
while OTS rejects it under the OpenType DFLT requirement.
**Choice:** GSUB output adds an empty default language system for DFLT when it is
missing. Subsetting reports this as a repair, preserving every explicit language
and its feature selection. Existing native output already has that empty default.
**Reason:** This does not introduce default substitutions or change the measured
language fallback behavior; it produces conforming output from that tolerant input.

## 2026-10-03 — Empty layout has zero maximum context

**Question:** Additional all-glyph tests for every supplied Latin-only BDF found
`usMaxContext=1` even when no substitutions were generated, versus zero from the
reference. All regional fonts correctly require one input glyph of context.
**Choice:** Set the value from the actual presence of layout features. Add a
focused metadata regression and all three supplied Latin font matrices. Rebuild
and byte-compare all four full regional outputs against their completed external
oracle results; the regional bytes must remain exact. Reader sources are unchanged.
**Reason:** Make native metadata precise in the no-alternative case without
weakening the external assertion or relabeling older full-font evidence.

## 2026-10-03 — W2.6 review resolves source defaults before deduplication

**Question:** A valid Font/BDF 2.2 can put DWIDTH, DWIDTH1, VVECTOR and scalable
metrics in font-level defaults. Comparing only each glyph's optional fields
merged different regional advances, and later output borrowed the base face's
defaults for alternatives.
**Choice:** Resolve every optional glyph metric against its own source Font
before planning. Explicit and inherited equal metrics deduplicate, distinct
effective metrics remain distinct, and a missing alternate metric never borrows
an unrelated base default. Keep source bitmap bounds authoritative.
**Reason:** Bitmap-plus-metric identity must describe the effective glyph. API
regressions and independent BDF 2.2 / HarfBuzz / fontTools checks cover both
different defaults and equivalent explicit-versus-inherited alternatives.

## 2026-10-03 — W2.6 review preserves embedded vertical metrics and table pairs

**Question:** The variants writer emitted vhea/vmtx but horizontal-only embedded
bitmap records, silently losing source vertical metrics. Separately, requesting
only `--drop-table vhea` produced a font OTS rejected because vmtx requires vhea.
**Choice:** Write both directions in big sbit metrics when vertical layout is
present, including each effective advance and vertical origin. For an absent
pixel origin, use the ascent and integer cell midpoint (half-pixel midpoints
round down, as the bitmap metrics are integral); explicit origins stay exact.
Reject unpaired vhea/vmtx drops before output, requiring both exact tags to
acknowledge removal of the optional vertical metric pair.
**Reason:** Outline and bitmap paths must both preserve representable metrics,
and an acknowledged table loss must not leave an invalid dependent table.
The OpenType [EBDT specification](https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt)
defines both directions in BigGlyphMetrics; independent FreeType and OTS checks
verify the corrected output and atomic failure of unpaired removal.

## 2026-10-03 — Match Macao's registered OpenType language tag

**Question:** The candidate grouped both `zh-HK` and `zh-MO` under ZHH, but the
current Microsoft registry explicitly assigns traditional Macao Chinese ZHTM.
**Choice:** Map `zh-MO` and `zh-Hant-MO` to ZHTM. An explicit Hans script continues
to select ZHS, including in Macao. Hong Kong continues to select ZHH.
**Reason:** The promised registry-based BCP-47 conversion must distinguish the
registered regional tags. Source: Microsoft's
[language system registry](https://learn.microsoft.com/en-us/typography/opentype/spec/languagetags).

## 2026-10-03 — W2.7 uses exhaustive grid checks and explicit raster overrides

**Question:** The catalogue's importer samples at most 300 outlines and requires
20 samples, while PLAN requires all coordinates on a common grid and bounded
native OpenType integration.
**Choice:** Examine all expanded geometry, accept small provable grid fonts, and
require explicit ppem for non-grid outlines. Preserve physical bitmap strikes by
default; exact requested-size strikes precede per-glyph outline fallback. Keep
CFF cubic paths exact through center-sampling, rather than flattening them.
**Reason:** Sampling could miss a late anomalous component or reject valid tiny
fonts. Automatic grid recovery and reviewed arbitrary rasterization are distinct
contracts; unsupported variation/hinting must stay explicit.

## 2026-10-03 — Collections separate face packaging from bitmap projection

**Question:** A FontSet can represent independent faces or bitmap strikes, and
TTC shares tables rather than concatenating standalone sfnts.
**Choice:** Expose a bounded TTC v1/v2 container API and deterministic same-tag
byte sharing. Native TTC writes interpret each Font as one face. Extraction and
reassembly remove stale DSIG and recompute standalone checksums; native readers
annotate source face indices and preserve ordered face/strike expansion.
**Reason:** Preserve useful low-level fidelity without guessing face groupings
or claiming arbitrary OpenType layout survives bitmap projection.

## 2026-10-03 — WOFF2 collection scope includes associated transforms

**Question:** W2.5 deliberately deferred collection envelopes to W2.7; a TTC
wrapper cannot be treated as a standalone sfnt or compressed as opaque bytes.
**Choice:** Implement WOFF2 collection directories and all associated glyf/loca
transforms, preserving shared-table identity and font order. Keep hmtx untransformed
inside collections. Reject one-sided glyf/loca sharing under WOFF2's explicit rule.
**Reason:** A real collection codec must preserve face topology and not duplicate
originally shared table records or reuse context-dependent transformed metrics.
Source: https://www.w3.org/TR/WOFF2/#collection_dir_format.

## 2026-10-03 — TrueType raster origins include phantom metrics

**Question:** Stored outline x coordinates alone differed from FreeType when hmtx
LSB did not equal glyf xMin; scaled USE_MY_METRICS components amplified the error.
**Choice:** Apply the source glyph's horizontal phantom displacement and recursively
resolve marked component metrics before rasterization. Retain raw glyf bounds for
vertical metrics; CFF uses exact cubic extrema and VORG where present.
**Reason:** The effective outline origin includes side-bearing semantics. Fresh
independent FontTools/FreeType fixtures verify scaled and translated components,
including that inherited unhinted advances are not multiplied by the component scale.

## 2026-10-03 — Keep strike scaling metadata through native wrappers

**Question:** Combining preferred embedded families copied Fonts but discarded
FontSet-level EBSC instructions and raw-table provenance. TTC diagnostics also
needed the flattened output index rather than a face-local strike index.
**Choice:** Preserve bounded EBSC instructions for selected physical strikes;
retain face-local instructions under face-qualified collection properties. Report
omitted raw common/outline provenance explicitly. Offset diagnostic font indices
when flattening collection faces. Reserve the generated dot-helper name before
assigning source glyph names.
**Reason:** Wrapper integration must not silently lose existing bitmap scaling
metadata, misidentify the affected font, or introduce glyph-name collisions.
Focused regressions cover each boundary; changed reader code receives a fresh
full-duration fuzz run before handoff.

## 2026-10-03 — Native source glyph zero uses source metrics

**Question:** Independent W2.7 review found that reusing the regional exporter
also reused its synthesized `.notdef` rules for a real source glyph zero. This
shifted TrueType ink to x=1, ignored explicit vertical origins, and could silently
accept an unrepresentable origin. Native underline fields were also reset.
**Choice:** Apply generated-placeholder rules only to the regional synthetic
`.notdef`. Native source glyph zero follows the same shape, bearings, origin and
validation rules as other source glyphs. Native blank outlines encode their
vertical origin relative to the empty outline top at zero. Native post tables
scale and preserve source underline position and thickness, with checked range
conversion. The original regional variant output contract remains unchanged.
**Reason:** Sharing an exporter must not overwrite physical source-glyph metrics.
Failing-before/passing-after tests, plus independent FontTools/FreeType/OTS checks,
cover these cases. The review also makes the outline-only path enforce max_fonts,
as embedded and collection paths already did.

## 2026-10-03 — Bitmap detection defers only to enabled canonical codecs

**Question:** With `otb,vector-raster` enabled but no `ttf`/`otf` features,
outline-bearing embedded fonts became unrecognized. Enabling a web wrapper
implicitly enabled vector-raster and thus could break existing OTB detection.
**Choice:** Embedded detectors defer to TTF/OTF only when that exact canonical
codec is enabled. The format-agnostic vector-raster alias remains explicit.
**Reason:** Feature dependencies must not disable the only installed detector.
A narrow-feature failing-before regression covers both TrueType and CFF hybrids;
the all-feature case still resolves to the canonical outline container.

## 2026-10-03 — Strike-filtered diagnostics reference retained fonts

**Question:** Selecting a second embedded ppem kept its old font index in reports,
so diagnostics could refer to index 1 when only one Font was returned. Combining
multiple bitmap families had the same source-index versus output-index risk.
**Choice:** Preserve source strike indices through selection, remap retained
strike diagnostics to their output Font indices, and omit strike-scoped messages
for unselected or superseded strikes. Global conversion-loss reports remain.
**Reason:** Diagnostics must identify the actual returned font, including before
collection face indices are flattened. A two-size second-strike regression
reproduces the invalid index and verifies the mapping.

## 2026-10-03 — Native SVG font paths retain bounded raster provenance

**Question:** SVG 1.1 has no bitmap padding rectangle or alias identity, generic
curves need a reviewed raster size, and compatibility output deliberately follows
historical bdfparser omissions and formatting.
**Choice:** Keep compatibility bytes unchanged. Add an independent native codec
with standard SVG glyph paths, explicit private pixel-size/bounds/identity
attributes, and typed limits. Native reading still rasterizes the visible paths.
Generic fonts use exhaustive 4–64 ppem grid detection or explicit ppem. XML DTDs,
external resources, scripts and unsupported paint semantics are never executed.
**Reason:** Blank glyphs, negative bearings, aliases and padding must survive our
own output without pretending to be a general SVG document renderer. Every
unrepresented source field is diagnosed; native rendering and SPEC parity remain
separate, testable contracts.

## 2026-10-03 — SVG rasterization streams expanded glyph geometry

**Question:** Full Tong CJK separate-square SVG passed serialization but retaining
all expanded paths exceeded the reader's 256 MiB geometry/model budget.
**Choice:** Keep XML as bounded borrowed input, retain only glyph node references,
and parse/rasterize one glyph at a time. Automatic grid detection makes a bounded
first pass; native/explicit sizes need only the raster pass. Alias consistency
compares borrowed XML attributes rather than retaining another copy of every path.
Large textual dot fonts may require an explicit input-byte budget increase.
**Reason:** Raising the geometry budget would mask avoidable peak-memory growth.
A small-budget many-glyph regression and full CJK external checks cover the change.

## 2026-10-03 — Reject XML and arc expansion before allocation

**Question:** DOM node limits alone do not bound attributes per element before
roxmltree's duplicate-attribute scan, and SVG arc radius correction happens before
svgtypes returns normalized segments.
**Choice:** Add an allocation-free xmlparser token preflight enforcing attribute,
namespace, depth, node and bookkeeping budgets before DOM construction. Bound
resolved endpoints and conservative arc radius/subdivision expansion before
invoking svgtypes normalization. Keep DTDs and non-UTF-8 declarations explicit errors.
**Reason:** Post-parse limits are too late for adversarial attribute lists or
extremely anisotropic radii. Dedicated regression tests exercise both boundaries.

## 2026-10-03 — SVG review preserves separate paint and bounded alias identity

**Question:** Opposite-winding child paths cancelled despite SVG painting them
separately; alias grouping repeatedly cloned/reindexed accumulated mappings, and
loss locations could name a nonexistent glyph. Free-form style names were also
written as invalid SVG `font-style` descriptors.
**Choice:** Union independently rasterized child paths, charge every layer's
raster work, collect each alias group before model indexing, and use its retained
index in diagnostics. Charge expanded Unicode metadata before allocation, parse
fixed native metric fields without a token vector, reject SMIL elements, and
report unknown native glyph metadata. Retain exact style names in `fp:style`;
only valid SVG style descriptors are also written as `font-style`.
**Reason:** Supported SVG geometry and native metadata must remain faithful;
configurable resource limits must cover expansion and avoid quadratic work.
Independent failing-before regressions and fresh reader fuzzing cover the changes.

## 2026-10-03 — Explicit CLI input-size override for large SVG fonts

**Question:** Full CJK circular SVG output is valid but exceeds the CLI's common
64 MiB default input cap, with no way to reopen it in that CLI.
**Choice:** Add `--max-input-bytes` to inspection/conversion/rendering arguments
and carry it through file reading and SVG registry limits. Keep the default and
independent 256 MiB aggregate cap unchanged.
**Reason:** A safe default should remain overridable for known local full-font
outputs. Check the complete supplied Tong CJK circular font using this path.

## 2026-10-03 — Preserve exact square contours with a packed small-grid tracer

**Question:** Native square export converted packed bitmaps to strings and built a
BTreeMap with an allocation for each boundary vertex.
**Choice:** For at most 4,096 grid vertices, trace packed bits with a bounded dense
two-edge adjacency map. Preserve the SPEC bottom-to-top edge insertion order,
right-turn selection, collinear removal, contour rotation and conservative limits.
Large grids and the ragged/label compatibility adapter keep the general tracer.
**Reason:** Remove allocation and conversion overhead without changing ordering or
compatibility bytes. Exhaustive 3×3 patterns and edge/point-budget regressions
compare exact contours and errors; complete square-font outputs are byte-checked.

## 2026-10-03 — Reuse immutable variant plans under identical limits

**Question:** Producing multiple shapes/containers repeated source validation,
alternate deduplication, sorting and GSUB construction.
**Choice:** `Variants::prepare` returns an immutable reusable plan borrowing the
base font's metadata. It supports bounded caller-owned threads. Encodes must use
exactly the same layout/resource limits; changing limits requires preparing again.
The ordinary `encode` API keeps its behavior.
**Reason:** Avoid repeated planning and bitmap clones while preventing stale source
metadata or a stricter caller budget from being silently bypassed.

## 2026-10-03 — Reuse existing row glyphs in native default dot composites

**Question:** A literal circle component for every pixel makes the full regional
dot TTF and its quality-11 compression disproportionately expensive. Adding row
helper glyphs would violate the reference glyph-count requirement.
**Choice:** At the Tong defaults (100 units/pixel and 0.9 dot diameter), reuse up to
256 existing monochrome one-row patterns of width at most 64. Add no glyphs and
keep all glyph IDs, cmap, names and metrics. Selected row glyphs stay literal
circle composites, preventing cycles and limiting nesting to two levels. Match
only still-uncovered ink; use direct circles when no pattern or int16 translation
fits. Preserve the original per-glyph and aggregate literal-dot work budgets.
Other scales/diameters and the exact compatibility writer retain their paths.
**Reason:** Shorten the native composite stream without changing the displayed
pixels. Native TTF/WOFF2 bytes change intentionally; full external glyph, metric,
shaping, subset and raster tests must pass before integration.

## 2026-10-03 — Bounded multicore export, fixed quality, deterministic defaults

**Question:** Eight separate quality-11 CLI jobs exceeded available memory, while
one compression worker left single-font latency high. The user explicitly asked
for maximum practical performance and multicore use.
**Choice:** Keep quality 11. Use two Brotli workers by default for streams at least
128 KiB, retaining the serial path below that size and the explicit thread option.
The threshold depends on input bytes, never on host CPU count. Use thin LTO and
one codegen unit for release builds. A complete native `tong_export` example reads
faces and prepares plans in bounded parallel pools, then reuses each plan and the
already-written TTF/CFF bytes for WOFF2. Its conservative default is four font jobs.
**Reason:** Improve throughput while preserving deterministic options and honest
memory bounds. The example is a reference-policy API workflow, not a claim that
the Phase 6 generic `fontpixel batch` command is implemented. Five-run final
measurements and peak-RSS probes decide acceptance; single probes are not medians.

## 2026-10-03 — Pack alternating CFF axis segments without changing paths

**Question:** Separate hlineto/vlineto operators inflated square CFF data and the
quality-11 compression workload relative to the original Tong encoder.
**Choice:** Pack consecutive alternating horizontal/vertical line operands under
one Type 2 operator, with at most 40 operands per run. Split same-axis runs and
reset on moves, diagonal lines, curves and close. Keep quantization, geometry
budgets and the existing circle subroutine unchanged.
**Reason:** This is standard Type 2 encoding, below the 48-entry operand-stack
limit; it changes bytes but no outline. Regression tests cross run boundaries
and every reset case, including zero-length lines. Full external fontTools,
FreeType, OTS and HarfBuzz gates still apply to all affected CFF/WOFF2 outputs.

## 2026-10-03 — Reject experimental shared Brotli hasher setup

**Question:** Brotli's `favor_cpu_efficiency` option promises less repeated
prefix-dictionary work across compression threads.
**Choice:** Do not enable it. A new >128 KiB synthetic sfnt regression exercised
actual two/four-worker paths and triggered the dependency's internal
`orig_hasher == self.hasher_` assertion. The normal default remains false;
retain the test and rejected probe logs. Small-font tests alone would have
missed it because they use the serial threshold path.
**Reason:** A modest, noisy single-probe throughput change cannot justify a
writer panic. Quality 11, safe fallback and deterministic valid output prevail.

## 2026-10-03 — Extend dot reuse to tiny two-dimensional ink patterns

**Question:** Row-only reuse shortened dot TTFs but quality-11 compression still
kept the complete family above the 40-second target in engineering probes.
**Choice:** Reuse existing single-row/column patterns up to 64 pixels and tiny
2D patterns no larger than 4×4. Keep the 256-helper limit, original glyph count,
metrics, literal-ink work budgets, int16 translations and maximum depth two.
Match a bounded 64×64 remaining-ink mask top-down; consume each ink pixel once.
Larger extents fall back to direct circles. Selected helpers stay literal.
**Reason:** Reduce the data entering Brotli without lowering quality. Native
TTF/WOFF2 bytes and circle-contour ordering may change, never pixels or mapping.
The resulting webfont can be larger despite a much smaller sfnt; report this
tradeoff explicitly. Exhaustive 3×3 combinations with displaced multirow helpers
check exact expanded contours as a multiset; complete fonts also need independent
pixel, metric, shaping, OTS, subset and reader validation before acceptance.

## 2026-10-03 — Use shared-plan eight-font throughput for the Tong example

**Question:** Four simultaneous fonts with two/four compressor workers incurred
more CPU time than eight independent font compressions after data-size reduction.
**Choice:** The complete-family example defaults to min(available CPUs, 8) jobs
and one compression thread per font, still quality 11. Its explicit options remain
bounded. Individual library/CLI exports retain two compression workers for latency.
**Reason:** All eight shared-plan jobs fit the measured host at roughly 2.85 GiB
peak RSS and a first 39.1-second probe. This is distinct from the rejected original
eight-process CLI strategy, which repeated source parsing and exceeded memory.
Five-run medians, resource measurements and independent output hashes, not the
single probe, establish final performance. The generic batch CLI remains Phase 6.

## 2026-10-03 — Expose dot representation's explicit time/size tradeoff

**Question:** Pattern reuse speeds export and shrinks TTFs, but the first complete
probe increased aggregate WOFF2 bytes from 7.75 MB to 8.78 MB (dot files roughly
35–37% larger). Leaving callers no choice would hide a significant tradeoff.
**Choice:** Expose `DotReuse::{Literal, Rows, Patterns}` in native variants options
and `--dot-reuse literal|rows|patterns` on native variants/convert. The full-family
example accepts the policy too. Patterns remains the speed-oriented default;
literal retains the old per-pixel representation and rows is intermediate.
**Reason:** The user's performance priority is explicit, but quality/size tradeoffs
must remain inspectable and selectable. Every policy retains quality 11 by
default and identical expanded circle geometry, glyph IDs, metrics and metadata.
Tests compare all three policies and retain dependent helpers through subsetting.

## 2026-10-03 — Reject Tong exporter destinations that alias sources or each other

**Question:** Independent review found that passing the BDF directory's parent as
`tong_export`'s output directory returned success while `fs::copy` truncated all
20 input BDFs. Existing symlink or hard-link destinations could also overwrite an
input through another format's output path.
**Choice:** Before parsing or writing any font, compare open-file identities for
all input and existing output paths. Reject input/output aliases, output/output
aliases, non-file destinations and unresolved destination symlinks. Ordinary
reruns into separate existing output directories remain supported. The example
uses the already-locked `same-file` crate as a dev dependency for portable file
identity; it adds no library runtime dependency.
**Reason:** A successful bulk export must never silently destroy its inputs or
make one artifact overwrite another through an existing alias. Unit and actual
binary tests cover identical paths, cross-input/cross-format hard links, file and
directory symlinks, dangling links, and identical-byte 84-file reruns. This is a
preflight for stable filesystem paths, not a defense against hostile concurrent
filesystem replacement. The frozen pre-review timing evidence stays identified
with its original executable; final acceptance needs the guarded executable's
verification.

## 2026-10-03 — Check dense-outline vertex additions on 32-bit targets

**Question:** The new small-grid dispatch checked vertex-count multiplication but
computed width+1 and height+1 first. A caller can legitimately construct a
zero-height, maximum-u32-width bitmap with explicitly raised bitmap limits; those
additions overflow usize on 32-bit targets.
**Choice:** Centralize both checked additions and multiplication before choosing
the packed tracer. Overflow selects the existing general path. Regressions cover
maximum-usize arithmetic and the public zero-height/maximum-u32-width case.
**Reason:** Resource-limit overrides must not introduce arithmetic panics. Normal
small-grid dispatch and all contour bytes remain unchanged.

## 2026-10-03 — W3.1 console-font geometry and Unicode semantics

**Question:** HEX has no encoded height/baseline; PSF has glyph positions but no
baseline or character set in table-less fonts. External monobit behavior is
ambiguous for extended HEX and disagrees with the PSF1 sequence marker.
**Choice:** HEX defaults to height 16/descent 0 with explicit overrides and fixed
8/16/24/32 widths. PSF defaults to PSF2 and baseline 0; table-less positions are
legacy codes. Follow kbd's LE FFFE PSF1 marker and strict UTF-8 PSF2 grammar,
including sequence-only entries. Refuse malformed geometry instead of guessing.
Use loss reports for target metadata, alias identity, geometry normalization and
nonrepresentable mappings. Do not silently clip, truncate, or remap legacy codes.
**Reason:** Deterministic reversible native subsets, honest strict conversion,
and interoperable specification semantics take priority over oracle quirks.
The supplied catalogue has no HEX/PSF importer; test its BDF conversion boundary
and document the absent direct importer rather than inventing a passing test.

The native console readers derive point size = cell height at 72 dpi so the
existing BDF writer has a complete SIZE header. This introduces no claimed source
physical measurement, and is documented as an explicit conversion convention.

Table-less PSF explicitly sets charset `unknown`, because the generic BDF-oriented
missing-charset fallback is Unicode. Legacy code tags and unknown charset together
prevent downstream charset operations from accidentally inferring Unicode.

## 2026-10-04 — Include retained gzip bytes and HEX comments in console budgets

**Question:** Independent W3.1 review reproduced two boundary gaps: compressed
input was checked by the transport but not included in the console model's
aggregate charge, and HEX comment output bypassed the per-record cap.
**Choice:** Seed the console read budget from all bytes retained by the read
context (wire plus decoded input). Direct readers still charge their raw input
once. Check each generated comment record, including its `#`, before writing;
the LF delimiter is outside the text-record limit, matching the reader.
**Reason:** Independently valid transport and model allocations must not each
consume the same aggregate allowance. All record types must honor the selected
output record cap. Four regressions establish rejection and exact inclusive
boundaries for both codecs without changing valid font or mapping bytes.

## 2026-10-04 — W3.3 explicit atlas maps and safe C literals

**Question:** Image atlases and C arrays do not intrinsically carry character maps,
full metrics, or safe filesystem authority; the plan requires faithful roundtrips.
**Choice:** Versioned JSON/TOML atlas sidecars retain native model metadata while
catalogue-compatible maps retain their documented pixel/baseline rules. Reject any
visible unmapped image region. C uses a bounded non-executing literal parser,
authoritative font8x8 spans, explicit generic geometry/charset, and standard
Adafruit fields with complete loss reporting. Resolving sidecars stays in the
caller-supplied resource boundary. CLI resources are scoped to the primary folder.
**Reason:** No silent character guessing, hidden C execution, clipped pixels,
charset conversion, or uncontrolled path access is acceptable.

## 2026-10-04 — Schedule authorized font merge features without interrupting implementation

**Question:** The owner added a font merge library/CLI, WASM interface and a pure
client-side merge page while the existing implementation phase is in progress.
**Choice:** Preserve the complete owner message verbatim in
[`plan-addendum-merge.md`](plan-addendum-merge.md), including its opening and
closing scheduling instructions. Add W6.5/W6.6 to Stage 6, W8.3 to Stage 8 and
Stage 9 (W9.1–W9.3) after Stage 8 and before final wrap-up. Preserve every existing
plan item, phase order and acceptance gate. The narrowly scoped Stage 9 page is
an explicit addition to the original no-GUI scope; it does not authorize editing
or deploying the catalogue website. This change records future work only and
does not interrupt W3.2 or implement any merge feature now.
**Reason:** The full addendum retains all selection, spacing, fallback, metadata,
licensing/reserved-name, benchmark and browser acceptance details without losing
requirements in a summary. Native CLI and browser output must be byte-identical;
font processing remains client-side with no font uploads. Catalogue file URLs
must be relative or same-origin. All later implementations and stage reports must
follow the complete addendum, including its benchmark and browser gates.

**Source integrity:** Owner message `Sentinel_8661c50a2db88191a8062e1b3713b52b`,
retrieved by the coordinator. The exact UTF-8 text is 9,356 bytes, has no added
final newline, and has SHA-256
`f2ea3e31c183eebe1979b507f8a3748674b5dc47eabdfa8a04b9d2c9af1cd376`.

## 2026-10-04 — W3.2 FNT legacy encoding and FON container scope

**Question:** Windows byte charsets, historical ANSI variants and memory-only
flags have no unambiguous universal Unicode meaning; FON wraps executable headers.
**Choice:** Keep input codes legacy with explicit charset labels, preserve named
FNT metadata, use modern cp1252 for ANSI, and require an explicit page for OEM or
symbol Unicode conversion. Support raster v1/v2/v3 and NE writes/NE+PE reads.
Reject unsupported vector/color/ABC layouts instead of misreading their bytes.
Read executable containers only as bounded resources, never run embedded code.
**Reason:** Preserve original mappings and safety while allowing explicit exact
charset conversion. FONT resources are authoritative over advisory FONTDIR.

The writer preserves already encoded legacy CJK byte values without opening
large conversion tables. Actual Unicode conversion still requires the exact CJK
feature. Source FNT character-range endpoints are retained so absent leading or
trailing slots and relative break/default indexes are not silently retargeted.
Metadata names use byte-preserving Latin-1 because historical Windows strings
are not universally tagged with the glyph charset.

## 2026-10-04 — Bound FNT normalization work by exportable byte positions

**Question:** A final Tong SC probe hit the 256 MiB default aggregate budget
because the writer charged worst-case normalization diagnostics for all 38,653
source glyphs, although no more than 256 can be exported.
**Choice:** Keep per-source bitmap, metadata, mapping and omission charges;
charge normalization diagnostics for at most 256 glyphs, plus a font-level
reserve. Explicitly omit zero-advance mappings with loss entries because a
zero-width FNT table slot means an absent glyph.
**Reason:** The bound stays conservative without spuriously refusing ordinary
large CJK sources before their unmappable characters can be reported. A focused
38,653-supplementary-glyph regression and the supplied Tong SC probe pass.
The owner requested a safe handoff immediately afterward: this final change has
focused validation only, not renewed full/fuzz/performance acceptance.

## 2026-10-04 — Local W3.2 review: cumulative output budgets and fatal reports

**Question:** FON writing deducted prior encoded bytes but not retained source
models or accumulated diagnostics from the next font's budget. Container
properties bypassed metadata accounting, and FONT payloads were not checked
against the resource cap used by the reader. An entirely unrepresentable FNT
discarded its detailed omission report when returning `InvalidInput`.
**Choice:** Preflight all source/model/diagnostic reservations and container
properties against one aggregate budget. Give each FNT writer only the budget
left after other sources and retained output, and cap aligned NE resources and
the FONTDIR name offset before allocation. Return `ReportedErrors` with the
per-mapping omissions and a fatal `fnt.no_representable_glyphs` entry when no
slot survives; FON errors retain the actual font index and earlier diagnostics.
**Reason:** A multi-font operation must not repeatedly spend the same memory
allowance. Generated resources must honor their reader's resource limits, and
failed conversions still need actionable loss details without returning bytes.

## W3.3 CLI resources and output bundles (2026-10-04, before implementation)

The CLI grants each codec a `cap_std::fs::Dir` rooted at the primary input's
containing directory. Relative sidecars are opened through that directory handle,
with bounded reads; symlink traversal cannot grant access outside it. The explicitly
selected primary itself may be a symlink. Its spelling's parent establishes the
sidecar root. Successfully opened primary and sidecar file identities remain held
through conversion so output aliases (including hard links) can be rejected.

Bundle output preflights all paths and existing file identities, stages and syncs
all bytes before replacing anything, then commits with saved originals and rollback
on ordinary I/O failure. The primary is committed last. Output symlinks and symlink
parents are rejected. Newly created empty directories and staging files are removed
on failure. This is a recoverable multi-file operation, not a promise of filesystem
crash atomicity or isolation from another process modifying the same destination.
The single-file `persist` API remains for existing rendering/OpenType commands.

Directory-handle API: https://docs.rs/cap-std/4.0.3/cap_std/fs/struct.Dir.html

Existing single-file conversion deliberately accepts `convert a.bdf -o a.bdf`.
Keep that compatibility only for one input, one output, exactly the same spelled
path and no resolved sidecars. All other input/sidecar aliases are rejected.

The CLI's `cap-std 4.0.3` dependency introduces `winx` under Apache-2.0 WITH
LLVM-exception (an Apache-family permissive license allowed by PLAN §3).
`deny.toml` admits that exact expression. Version-pinned duplicate allowances
cover only io-lifetimes 2.0.4 and the Windows ABI crates required by cap-std's
current adapters; cargo-deny advisories, sources and all other duplicate checks
remain active. The library retains zero filesystem dependencies.

## 2026-10-04 — Native image metadata and portable C declarations

Native JSON/TOML sidecars retain stable glyph slots, aliases, sequences, complete
metrics and ordered properties. All nested metadata rejects unknown fields so
strict conversion cannot silently erase a future extension. TOML wraps the same
JSON schema to retain unsigned values and heterogeneous property lists. TGA
output is uncompressed L8: the dependency's cross-scanline RLE output failed
independent Pillow decoding. Reader/encoder peaks are budgeted separately, with
schema-aware metadata accounting and format-specific scratch reservations.

C input uses a bounded literal lexer; no expressions, macros or source code are
executed. Real declarations/comments select a family, and Unicode range comments
belong to their following declaration. An explicit array name selects exactly one
declaration across included resources. C writes report stable-ID reordering and
retain per-glyph omissions when automatic output-range inference fails. Explicit
ranges keep their documented empty-cell generation behavior.

## 2026-10-04 — KBITX grayscale, scalar identity and XML boundaries

Retain exact Gray8 by default; catalogue-compatible reduction requires an explicit
threshold. Keep native scalar records and inert kerning/feature/PUAA XML. Permit
the standard external DOCTYPE as inert syntax, without entity resolution; reject
internal DTDs/entities. Numeric name IDs canonicalize before duplicate precedence
is applied, and output reports retained-metadata reordering. Pixel em height maps
to point size at 72 dpi for reproducible BDF conversion. Writers preflight escaped
XML, WIB and model allocations against the same limits as the reader.

## 2026-10-04 — Pebble v1 follows the runtime layout

The official Pebble `text_resources.c` and `text_resources.h` establish v1's
six-byte header, absent hash table, u16 codepoints/u16 word offsets and eight-byte
glyph headers with advance at byte seven. V2/v3 use byte offsets and five-byte
glyph headers. Implementations follow those runtime structures.

## 2026-10-04 — Report incomplete Pebble RLE rows as loss

TUMBLED v1.11's 28-point bold font contains 7,829 incomplete final RLE rows.
The runtime and catalogue render only complete rows. Decode that same geometry,
discard residual pixels with per-glyph `pbf.read.rle_tail` loss diagnostics and
let strict conversion prevent output. Nonempty zero-width RLE remains invalid.

## 2026-10-04 — PBF writes remain readable under identical limits

Cap generated bytes by both the writer output limit and reader input limit.
Share allocation-free reader budget accounting with the writer, including
alias-heavy mapping tables, glyph storage, rasters and preserved properties.
Successful writes must decode under the same ResourceLimits; fuzz uses that
contract without increasing its reread budget.

## 2026-10-04 — Explicit HBF charsets and bounded bitmap bundles

Preserve legacy codes and explicit charset identity, validate GB/JIS/KSC GL/GR
ranges, and never substitute GBK or prefix CNS planes. Use dense ranks to skip
forbidden bytes. Resolve bounded bitmap resources through ReadContext and return
a header plus bitmap assets through OutputBundle. Gray8 requires an explicit
threshold, packs directly into output rows, and reports loss. Compare complete
target metadata structures for unsupported fields. This preserves glyph meaning,
keeps work proportional to actual glyphs and enforces one resource allowance.

monobit 0.54 remains an independent oracle. Its writer omits HBF_END_FONT and
emits absolute bitmap paths that its own loader cannot reliably reopen. The
crosscheck preserves raw output and changes only the missing terminator and
verified same-directory basename; codes, metrics and pixels remain untouched.

## 2026-10-04 — Explicit P2 layouts and native DOS decompression

Headerless raw/HZK bytes have no content detector: callers declare a preset or
complete geometry. CLI options configure the reader and writer consistently,
including transposition, padding and bit order. CPI retains legacy code-page
slots and multiple strikes. CPX import decodes bounded NRV2B natively without
running its executable stub. CPX export uses native compression and an original
DOS stub. Final acceptance verifies UPX decompression and executes the external
FreeDOS MODE loader in an emulator, checking return state and guarded memory.
The independent CPX review and bounded-source fuzz evidence are linked from
`reports/phase-4.md`.
The CLI defaults `.cpx` to CPX and `.cp` to standalone CP,
and otherwise defaults to FONT. FONT.NT and DRFONT are explicit choices.

## 2026-10-04 — Preserve PF2 variants and exact CJK mapping

GRUB joining variants use published high-bit tags. Retain those raw identities
without pretending that they are Unicode scalars; validate the underlying scalar
and permitted flags. FONTX2/HZK preserve legacy source codes and require exact
declared charsets for Unicode conversion. CJK tables remain an optional feature.
All 25 HZK/ASC presets can be overridden with explicit geometry. Interoperability
uses independently decoded pixels and font coordinates where external tools
normalize glyph bounding boxes.

## 2026-10-04 — Macintosh resource identities and indexed coverage

Support the listed resource-fork, dfont, suitcase, MacBinary and Apple containers
through the existing resolver/output bundle interface. Preserve opaque FOND and
other resources once at collection level. Classic FONT IDs encode family and
size; size-zero records are names, not rasters. Indexed `fctb` resources project
to Gray8 ink coverage using documented RGB luminance and explicit default port
colors, with loss diagnostics. Keep original records for unchanged native
roundtrips. Synthetic and Lisa-compressed resources fail explicitly. Unicode
codes require explicit charset conversion before writing legacy FONT/NFNT.

## 2026-10-04 — Independent P2 validation and performance conditions

Each worker owns its codecs, specification, tests and external comparisons; the
coordinator owns shared registration and CLI integration. Freeze manifests bind
final source to fuzz evidence. A unified benchmark checks mappings and pixels,
then measures complete in-memory reads/writes and 400 independent reads on
one/two/four threads. Thread creation and warmup precede timing. Five release
runs are taken only after task-owned fuzz/build/oracle processes have stopped;
debug smoke checks and concurrent measurements are not performance evidence.

The CLI now exposes a local `bdf` feature as well as its library dependency
feature. BDF-based P2 integration tests require it explicitly, while the raw to
NFNT container test also runs without a BDF reader. Clap parameter groups are
disabled for the flattened P2 argument structs so equal Rust type names cannot
create duplicate command-line groups.

## 2026-10-04 — Keep the phase 5 survey as concrete required scope

**Question:** The original 46 IDs do not cover all general bitmap formats in the
versioned independent tool catalogues.
**Choice:** Add 53 canonical wire families from the reproducible W5.1 survey,
for 99 planned IDs. Group aliases and related native variants under one ID,
while recording existing-family extensions separately. Register unimplemented
entries honestly as planned; do not advertise codecs merely because their
feature or catalogue slot exists. Format-specific native readers and writers,
loss reports, independent validation, fuzz and performance remain required.
**Reason:** The original task explicitly requires this survey and its resulting
implementations. Missing upstream writers or freely redistributable historical
artwork are not reasons to omit an otherwise documented general format.

The boundary is the pinned monobit release/master, Bits’N’Picas catalogue and
the listed original developer sources, not every unknown or future format.
Generic compression/transport utilities and pure-outline formats are classified
separately from bitmap wire families; game-specific private formats remain
excluded by PLAN §1.3. See `reports/w5.1-survey.md` and its coverage check.

### 2026-10-04: Native directory font inputs and outputs

**Issue:** The survey includes image sets and console fonts whose native entry
point is a directory. Inventing a header file would not implement that format.
**Decision:** Extend the caller-controlled resource API with explicit directory
inputs and bounded, nonrecursive listing. Existing resolvers remain compatible
through a default unsupported listing method. Contexts budget listing calls,
entry metadata and subsequently resolved bytes; the CLI uses the same scoped
directory capability for enumeration and reads. Directory detection requires an
explicit format until a detector can inspect resources without guessing from
an empty byte buffer. Writers may return a directory OutputBundle with validated
assets strictly beneath its entry point, persisted through the existing staged
transaction. Single-file convenience and gzip transport do not flatten a
directory. No synthetic primary asset is added to native directory formats.
**Reason:** This preserves real wire layouts, deterministic traversal and the
library's filesystem-independent authority boundary for future Python/WASM use.

The CLI also indexes source/output file identities in hash sets, preserving a
separate resolution count for the existing single-file in-place exception. On
Unix it records device/inode metadata instead of retaining a descriptor per
glyph; inode reuse can only cause a conservative refusal to overwrite, and
cannot bypass the identity check. Staged files share one scoped directory handle
per parent. This avoids quadratic alias checks and descriptor exhaustion for
large image sets. Directory writes never use the single-file in-place exception.

Existing output font directories are checked against the complete new asset
tree before staging. Extra existing files or subdirectories cause a refusal,
leaving all existing contents intact. Matching assets can still be replaced
through the rollback-capable transaction. This prevents a smaller replacement
font from silently retaining old glyph files and avoids deleting unrelated user
files as an implicit side effect of conversion.

## 2026-10-04 — Apple and Qt native compatibility boundaries

Lisa compression must satisfy the original consumer's in-place expansion cursor
constraint. An explicitly compressed write fails if no safe stream is generated;
Preserve can choose the uncompressed native representation with diagnostics.
QPF2 defaults to Gray8, which the original Qt 4.8.7 renderer actually consumes;
packed Mono remains an explicit wire option because that renderer ignores its
format tag. NFNT BinHex validates all three CRCs and retains fork/header semantics
while canonicalizing whitespace and compression. Native Mac envelope structure
takes priority over coincidental bare-resource magic in a filename.

## 2026-10-04 — Native RISC OS coordinate evidence

The public PRM does not fully disambiguate paired 12-bit coordinate byte order.
NetSurf's BSD-licensed TTF2f producer establishes a continuous little-endian
24-bit pair, with independent signed 12-bit x/y values. The implementation uses
that layout and original signed-coordinate fixtures. An unrelated width/height
nibble mask error in the old producer is recorded and is not copied. The source
archive and its exact hashes remain in external verification evidence, outside
the production library.

## 2026-10-04 — Cumulative CLI input and gzip envelope budgets

Previously, individually bounded decodes could accumulate unbounded retained
models across many CLI inputs. The CLI now reserves existing models, diagnostics
and vector merge capacity before opening the next input. Model accounting walks
only the newly admitted model, counts allocation capacity and duplicate mapping
indexes, and includes deleted glyph slots. Admission is atomic and happens
before moving the new font into the aggregate. Conservative B-tree accounting
bounds owned storage; it is not a process-RSS estimate.

A one-byte raw font also demonstrated that a bounded native write could acquire
a larger gzip envelope that its identically configured reader would reject.
Built-in registry writers now expose their typed budgets through a private
adapter. Common transport limits can reduce those budgets but cannot enlarge
them. A bounded gzip sink checks encoded, decoded and simultaneously retained
bytes; native profile validation runs again with compressed bytes and retained
diagnostics reserved. The replay uses the same configured writer, preserving
headerless/raw and other native profiles. Ordinary uncompressed output does not
repeat serialization. The dedicated performance adapter records this gzip cost
separately. Strict-loss checking precedes compression and output persistence.

## 2026-10-04 — Prebuilt raster order is independent of numeric endianness

An external ELF32 build of NeXT's original producer emitted little-endian
Prebuilt headers with unchanged MSB-first mono rows, including 9- and 13-pixel
rows crossing byte boundaries. Gray output preserved each sample's value and
position. Default native decoding/encoding therefore uses MSB-first pixel
groups for both integer byte orders. LSB-first remains an explicit compatibility
option with separate CLI read/write switches; it is not inferred from the header.
The old producer promotes requested depths above one to eight and accepts only
16 gray input levels, so these runs do not claim native producer coverage of
packed depth-2/depth-4 output. The `SWAPBITS` suffix macro is not bitmap evidence.

## 2026-10-04 — Cross-format detection requires native evidence

Adding new codecs exposed false matches against existing valid fonts. Detection
now checks SymbOS record extents, ChiWriter layout extents, and RISC OS header
version/structure instead of short ambiguous prefixes. Xerox probes validate
complete native tables before adding a Possible candidate. Windows FON examines
FONT resources rather than treating every NE/PE executable as a Windows font;
this lets actual OS/2 GPI resources select their native reader. C checks bounded
actual declarations, so a native-family name in a HEX comment cannot override
that font's content. Cheap preliminary C checks avoid tokenizing unrelated large
inputs. Detector entry points are also exercised by the affected fuzz targets.

Actual dual interpretations remain ambiguous: a valid Palm/NFNT resource, or a
generic PNG that could be an atlas or SFont, requires an explicit input format.
Filename extensions remain hints and never break a content-evidence tie. Before
and after review evidence is kept separately in `reports/w5.detector-review*`.

## 2026-10-04 — Bound sparse GPI descriptor tables before allocation

The GPI descriptor count follows the inclusive 16-bit code span, while each
record can carry preserved native extension bytes. Compute and validate their
checked product, output cap and cumulative memory charge before reserving the
dense table. The allocation is fallible. This keeps two distant source codes
from requesting gigabytes despite a small output budget; accepted native bytes
remain unchanged. See `reports/w5.gpi-budget.json` and its two boundary cases.


## 2026-10-04 — Preserve each benchmark invocation and hand off at phase 5

The coordinator rejected the first phase-5 quiet run after archive verification
found that replacing suffixes collapsed dotted case IDs and overwrote raw logs.
The recorder now appends stream suffixes, refuses existing files, and checks
unique paths and all stream hashes before publishing. Real subprocess regressions
cover the collision and tampering; a fresh full preparation and five-run campaign
replace the rejected evidence without editing it. Native codec/timing workloads
are unchanged.

The user requested stopping local development after phase 5 and transferring
phase 6 to online dot with six Astra/xhigh/fast workers. The handoff preserves all
Git refs, original inputs and complete Tong baseline, while excluding toolchain
installs/build caches and duplicate historical generated outputs. The new worker
prompt limits the next task to W6.1–W6.6 and requires truthful reporting when fast
mode is unavailable or cannot be verified.

## 2026-10-04 — Workspace recovery and reconstruction

At about 18:33 UTC the active project/toolchain disappeared from the available
workspace. The cause is unknown. Original baseline archive was re-downloaded,
SHA-verified and restored in a fresh parent; no overlay onto old files occurred.
The owner explicitly authorized rebuilding/rewriting missing phase-6 work and
using all six available worker slots. Save each coherent reconstruction checkpoint
externally before further major changes; local Git is not an external backup.

Recovered source may be exact when a prior hash matches, or reconstructed from
retained patches/specifications. Record the distinction and rerun validation.
Do not manufacture lost logs, restore missing commit IDs as if their objects
survived, or reuse historical pass counts as current acceptance. Toolchain access
is incomplete and current reconstructed Rust code has not been compiled/tested.

Normalized metadata edits, optional variants naming overrides and explicit embedded
UPM/reference-strike/rounding controls preserve prior defaults. Catalogue omissions
must be fixed; implicit FontTools defaults such as fsType are not user license
instructions and must not be blindly copied. Document remaining differences.
