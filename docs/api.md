# Rust native API — reconstructed recovery guide

**Recovery status (2026-10-04):** this phase-6 documentation/source reconstruction is uncompiled and untested. Historical observations are not current acceptance. See [recovery status](recovery-status.md) for known gaps and catalogue metadata/metric work.

This is a semantic reconstruction. The recovered crate-root examples are in `api-rustdoc.md`; the model
contract is in [bitmap-model.md](bitmap-model.md). Reconcile this guide with recovered source
before integration; no examples were executed during recovery.

## Registry, model and reports

Format IDs are strings such as `bdf`, `pcf`, `yaff`, not an invented Format enum.
`fontpixel::registry()` installs enabled native codecs in `Registry<FontSet>`;
`Registry::new()` alone installs catalogue metadata. Library defaults are
bdf,pcf,yaff; `all-formats` enables all 99 format IDs. Other independent features
include cjk-charsets, compat-bdfparser and serde. `png` renders PNG, while `image`
is the atlas codec; sfnt enables table-level operations, with codec dependencies
enabled by their corresponding features. Disabled formats fail explicitly.

`fontpixel::read(&bytes, &ReadOptions)` reads standalone bytes;
`registry.read(&Input::new(&bytes).with_name(name), &options)` adds filename context.
An explicit `ReadOptions.format` bypasses recognition, but headerless geometry
still requires a typed codec configuration. Structural detection outranks suffixes;
ties remain errors. Use `set_<codec>_read_options`/`set_<codec>_write_options` and
actual native option types. Setters that return Result validate immediately.

Read returns `Loaded<FontSet> { fonts, report, format }`.
`Loaded<FontSet>::into_single` retains diagnostics/identity with the selected font;
`FontSet::single` and `into_single` refuse zero or multiple fonts. Glyph IDs remain
stable; removal leaves holes. File-order iteration and effective last-duplicate
lookup are distinct from ordered codepoint traversal. Use transactional
add/replace/edit methods to preserve indexes and call update_bounds explicitly
after geometric edits.

`ReportEntry` has level, impact, stable code, location and message. Use has_loss,
not warning severity/text. `StrictLoss`/`ReportedErrors` retain structured reports.
Carry reader reports through `registry.write_with_report`; strict refusal occurs
before output assets are returned. No library API implicitly persists files.

Current reconstructed source also exposes `metadata::NameOverrides` and
`metadata::apply_names(&mut FontSet, &NameOverrides, &ResourceLimits) -> Result<Report>`.
It stages explicit normalized name/notice replacements across strikes, keeps raw
source records separate, and audits intentional changes as non-loss information.
A family/style change clears a stale full/PostScript name unless explicitly
replaced; identical values generate no audit. Empty notices may clear normalized
text without deleting provenance or establishing permission. This API is
uncompiled/untested in the current reconstruction; catalogue metadata/default/metric
equivalence remains an independent open requirement.

## Resources and limits

Input.with_resources borrows a ResourceResolver. Bound returned allocation
capacity and enforce authority/symlink constraints. Directory input uses
Input::directory and bounded nonrecursive list, not a fabricated primary asset.
`Written { output: OutputBundle, report }` exposes primary, is_directory and all
assets. into_single refuses to discard sidecars. Output paths are relative and
collision checked; gzip wraps only a standalone asset.

ResourceLimits defaults: 64 MiB primary and per sidecar, 256 MiB cumulative input
and decompression, 1,024 resource operations, 1,000,000 glyphs, 16,777,216 pixels
per glyph, 64 MiB per table. Native codecs also bound model/geometry/output storage.
Common writer limits can tighten typed limits, not enlarge them. These are owned
allocation/work budgets, not RSS guarantees.

Font/FontSet retained_usage counts headers/capacities/indexes/deleted slots without
cloning. Font.cloned_usage preflights a second owned strike; the source remains
charged. The exact Rust 1.99.0 commit/target audit permits a proven B-tree occupancy
bound and initialized outer glyph-Vec clone length. Unverified builds/custom-std
indicators retain the prior conservative charges. The complete proof and guard are
preserved in [bitmap/model document](bitmap-model.md). The actual 256 MiB limit is unchanged.

## Rendering, charset and OpenType

Mono is packed MSB-left; Gray8 is coverage. Top-left/y-down pixel access differs
from baseline/bottom-left y-up bounds and bearings. Conversions require thresholds:
128 for usual grayscale coverage, 1 for compatibility 0/1/2 labels.
Font.draw/draw_codepoints/draw_all return Rendered with bitmap and report. Render
options preserve sixteen direction pairs, advance/cell/ink modes, signed spacing,
explicit missing-glyph behavior and limits. render::to_svg/to_png are image export;
the SVG font codec is separate.

CharsetCodec provides exact generated maps. Legacy codes require FontCharset.
Unknown low-level charset resolution has explicit known()==false identity fallback;
applications must not label it exact support. CJK requires the feature. iso2022cn
is a glyph-code GB2312 GL mapping, not an ISO-2022 byte-stream decoder.

vector_raster projects native outlines into the bitmap model, with automatic 4–64
ppem grid recovery or explicit raster size. Variable/CFF2 instantiation and hint
execution remain external. Use Sfnt/table APIs for table-preserving operations.
Variants supports base, language and script alternatives, prepared plans and typed
shape/flavor/naming/limits. Reusing a plan requires unchanged layout/resource limits.
subset preserves supported GSUB single substitutions, composite dependencies,
supported bitmap strikes and raw retained hints; unknown glyph-indexed tables,
unsupported layout/non-Unicode/UVS mappings fail unless explicitly supported.
Exact drop_tables records loss. WOFF/WOFF2 packaging can preserve auxiliary metadata
without bitmap projection; TTC/OTC collection operations preserve face structure.
WOFF2 defaults quality 11 and two bounded workers; encode_many also bounds retained
bytes/count. Literal/rows/patterns dot encodings preserve geometry.

## Merge API

`merge(&[MergePart::new(&font, Selection::parse("western")?)], &MergeOptions {
family: "Example".into(), ..Default::default() })` returns `MergeResult {font,report}`.
Parts borrow a single Font. Selection unions positives then applies exclusions;
negative-only starts all Unicode scalars. Western contains geometric. Legacy,
invalid and unencoded mappings are reported rather than guessed. First eligible
present part wins; fallback is opt-in.

dy is positive up. Ascent/descent use shifted source metrics or derived bounds,
with explicit nonnegative overrides. Auto is mono only if all parts explicitly
declare mono/cell. Mono takes the unique modal positive selected Han advance;
ties/no Han fail. Half widths cannot be silently rounded. Narrower even advances
are centered by blank advance space, not bitmap scaling. allow_misaligned preserves
incompatible source metrics with diagnostics. GeometricWidth::Half is default;
Full preserves supplied geometric metrics like CJK and does not change source
selection or create a fullwidth glyph.

For changed nonzero device advances, scalable advances rescale proportionally;
inexact values round nearest, ties away from zero, with scalable_rounding loss.
Zero device advance preserves independent SWIDTH with scalable_zero_advance note.
Reports include contribution/fallback/missing interval/block summaries, spacing,
metrics, licences and diagnostics. Missing coverage includes every selected scalar
even if unassigned, rather than only the union of supplied mappings.

Family is required and known reserved-name substrings are checked case-insensitively.
PartLicense can supplement notices/permissions. Recognition is bounded metadata
assessment. Unknown/prohibited licence states reject strict mode. Full ordered
source notices plus merge provenance remain in names. The intended phase-6 BDF
notice contract remains a known recovery gap: retain multiline notices as COMMENTs
plus a flattened property, with bdf.write.notice_line_breaks loss, and refuse that
boundary in strict mode. Current source inspection did not find that writer path;
it must be restored and tested before this behavior is claimed. Report supports complete Display,
dependency-free to_json and optional serde.

The SPEC compatibility facade remains separate. Python/WASM must expose all native
capabilities plus separately compatible APIs; neither a parity runner nor a few
bulk calls constitute those bindings.
