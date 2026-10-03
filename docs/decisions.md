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
