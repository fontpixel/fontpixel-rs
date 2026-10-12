# Rust native API — reconstructed recovery guide

**Status (2026-10-05):** the reconstructed phase-6 source compiles; `scripts/check.sh` passed (3,453 tests, clippy `-D warnings`, cargo-deny) and `cargo doc` with `RUSTDOCFLAGS=-D warnings` is clean. Historical observations are not current acceptance. See [recovery status](recovery-status.md) for known gaps and catalogue metadata/metric work.

This is a semantic reconstruction. The recovered crate-root examples are in `crates/fontpixel/docs/api-rustdoc.md`; the model
contract is in [bitmap-model.md](bitmap-model.md). Reconcile this guide with recovered source
before integration; the crate-root examples run as doctests in `scripts/check.sh`.

The rewritten [api_quickstart.rs](../crates/fontpixel/examples/api_quickstart.rs)
constructs its own small font and performs no filesystem I/O. Its model/name,
atomic-error, rendering/charset and merge checks require no format feature.
YAFF, PCF, atlas sidecars, OTB metric controls, regional/subset and WOFF2 portions
are guarded by their actual Cargo features. The example compiles under
`cargo clippy --all-targets`; `scripts/check.sh` does not run it. The source-reference checklist is [api-example-contracts.json](api-example-contracts.json).

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
text without deleting provenance or establishing permission. This API compiles
and has unit/integration tests; catalogue metadata/default/metric differences
are documented in [migration-fontpixel.md](migration-fontpixel.md#catalogue-download-differences).

`projection::apply_bitmap_projection(&mut FontSet, &BitmapProjection, max_working_bytes)`
applies explicit target-application metric conventions between reading and writing.
The default projection changes nothing. `BitmapProjection::scalable_from_device(units, ppem)`
sets every live glyph's horizontal scalable advance (BDF `SWIDTH` x), including
unencoded and duplicate-code records, to `round_half_even(DWIDTH x * units / ppem)`
in exact integer arithmetic; vertical and all other metrics stay unchanged. The whole
set is validated and staged before an allocation-free commit, so an error leaves it
unchanged. `ProjectionOutcome` lists every changed glyph (strike, ID, glyph-level and
effective prior value, device advance, new value). Replacing an effective source value
is `Loss` (strict mode rejects it); deriving an absent one is `Repair`. The CLI exposes
it as `convert --swidth-ppem N [--swidth-units U]` and batch `options.swidth_ppem`.

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

When `WriteOptions.transport_limits` is `Some(common)`, plain registry output
also checks the aggregate serialized asset lengths against
`common.max_output_bytes`. This includes primary, sidecar and directory assets;
it counts byte lengths, not spare allocation capacity or report storage. Checked
addition rejects overflow, and a sum exactly at the limit is allowed. This check
runs after combined error/strict diagnostics and before returning assets. A codec
without its own native output cap may finish encoding under its resource budget
before this check; it is not a universal preallocation or peak-memory guard.
`None` preserves existing typed writer limits without adding a plain aggregate
cap. A typed per-asset cap remains distinct. `TransportLimits::default()` still
has a 64 MiB output cap, applied only when supplied as common options or used by
the existing transport path; gzip envelope/replay budgets and defaults are unchanged.

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

## Explicit output names and embedded metrics

Current `variants::Options` exposes family, style, legacy_style, full_name,
postscript_name, version, vendor and notice fields. With generated names, style
sets typographic subfamily ID17, legacy_style sets ID2 (default Regular), and
full_name/postscript_name override IDs4/6 exactly. An exact supplied `names` table
takes precedence over generated-name controls. These choices do not establish
the original catalogue's implicit FontTools defaults; that comparison is open.

`formats::embedded::EmbeddedWriteOptions` controls OTB/bdat design metrics:

- `units_per_em: Some(value)` explicitly selects 16..=16384; None keeps automatic
  scaling.
- `EmbeddedReferenceStrike::First` is the default; `LargestPpem` chooses the
  greatest effective y-ppem, with the last original input winning ties and the
  same descending priority for missing glyphs. `Index(n)` selects a zero-based
  source then falls back in original order. Changing this policy leaves the codec's
  existing physical ordering unchanged: OTB uses source order, while bdat retains
  its canonical `(ppem_y, ppem_x)` sort.
- `EmbeddedRounding::NearestTiesAwayFromZero` is the default pixel-to-design rule;
  `NearestTiesEven` uses exact rational arithmetic. This does not change the
  separate OS/2 average-width rounding rule.

Install these through `Registry::set_embedded_write_options`. The shared CLI
writer options expose the same controls for `convert` and `merge`:
`--otb-upm 16..16384`, `--otb-reference first|largest|N` and
`--otb-rounding half-away|ties-even`. `N` is a zero-based index into the FontSet
passed to the writer; it chooses metric priority without dropping strikes or
changing the codec's existing physical ordering. Batch tasks pass `otb_upm`, `otb_reference` and `otb_rounding`
through their scalar `options`. Explicit flags require OTB/bdat output and the
corresponding enabled feature; color and other outputs reject them, even when
the requested value matches a default. Vector `--upm` remains a separate control.
See the [CLI examples](w6.1-cli.md#conversion-naming-and-metric-controls).
These paths and the feature-gated Rust quickstart compile; `scripts/check.sh`
does not run the quickstart.

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
source notices plus merge provenance remain in names. Reconstructed BDF source now
contains the notice projection: multiline COMMENTs plus a flattened property,
with bdf.write.notice_line_breaks loss and strict refusal. This supersedes the
earlier missing-source finding; it compiles and `crates/fontpixel/tests/phase6_notice_metadata.rs` covers it.
Report supports complete Display,
dependency-free to_json and optional serde.

The SPEC compatibility facade remains separate. Python/WASM must expose all native
capabilities plus separately compatible APIs; neither a parity runner nor a few
bulk calls constitute those bindings.
