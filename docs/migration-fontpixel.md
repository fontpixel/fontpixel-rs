# Catalogue migration findings — reconstructed recovery document

**Recovery status (2026-10-04):** this phase-6 documentation/source reconstruction is uncompiled and untested. Historical observations are not current acceptance. See [recovery status](recovery-status.md) for known gaps and catalogue metadata/metric work.

Status: reconstructed from retained W6.2 findings. The original complete guide
is missing; the inventory has now been regenerated from the unchanged supplied
manifest. Field coverage is not a semantic replacement claim, and the migration
is not accepted.

## Open correctness gaps

The latest coordinator finding says the catalogue comparison omitted bitmap
family, style and copyright. Implicit FontTools defaults and metric differences
still need an inventory. Therefore the earlier 173-font/8-ZIP result establishes
only the checks actually performed, not full semantic equivalence. Reconcile all
name records, missing versus explicit fields, global/strike/glyph metrics, styles,
copyright/license data, table flags/defaults and packaging against the original
producer before signing off the replacement. Do not invent an intentional
difference to explain an unexamined mismatch.

## Reference inventory retained from the earlier audit

- `reference/fontpixel/ingest/manifest.toml`: 442 active `[[family]]` and 71
  `[[todo]]` entries, 513 combined; 41 active family keys.
- Active `convert` values: absent/none 115, otb 133, ttf 178, kbitx 7,
  spritesheet 8, font8x8 1. `run.py` also supports pbf and t-mat-8x8, absent
  from active entries.
- 124 declared instances in 64 families; 12 axis instances across Pixelify Sans,
  Bitcount, Doto and Handjet; six Bitcount per-glyph-offset instances.
- 24 families use `grid_units`; one uses `pixel_offset`.
- Eight sprite families, 12 map objects, 1,175 explicit cells: 1,156 char cells
  and 19 name-only cells.
- Known license keys: spdx, name, name_en, confidence, file, note, note_en.
  Known variant overrides: weight, size, display. Additional run-only fields:
  name_en, authors_en, form, merge_subsets; family_name, foundry, glyph_offsets.
  Pending-item fields include source, reason, reason_en.
- Original manifest SHA-256:
  `fcf8b612cbb197298291bf3c16de03fb88ddba6bd7fb8176c532e95bfbbf874e`.

The exact current key lists were regenerated in
[migration-manifest-inventory.json](migration-manifest-inventory.json) from that
same manifest SHA. This is a new data-derived inventory, not a recovered copy of
the original JSON or evidence that each mapping has passed conversion tests.

## Every active family parameter

Host policy means the catalogue retains the parameter; it is not silently discarded
or claimed as a fontpixel codec option. The table covers all41 active keys.

| Parameter | Native mapping or retained host responsibility |
| --- | --- |
| `added` | Catalogue chronology; host metadata |
| `aliases` | Catalogue search/display aliases; host metadata |
| `all_glyphs` | Explicit glyph inclusion policy; reconcile unencoded/native mappings, not an assumed CLI flag |
| `author` | Author attribution in catalogue/README; retain source notices separately |
| `authors` | Ordered author records; host attribution |
| `convert` | Select native format importer/rasterization; unsupported variable/hinted cases retain fallback |
| `convert_take` | Host source glob selection before native conversion |
| `default_variant` | Host default download/display variant |
| `description` | Host localized catalogue description |
| `description_en` | Host English description |
| `export_name` | Explicit family/output naming policy; normalized family overrides do not rewrite original XLFD automatically |
| `form_verified` | Host provenance/review flag, not native inference |
| `forms` | Host language/form classification |
| `grid_units` | Explicit vector-raster grid override, resolved per source when a table |
| `hinting` | Original hint interpreter fallback; native rasterizer does not execute hint programs |
| `homepage` | Host source/reference URL |
| `instances` | Resolve per-file instance lists; map names/weights explicitly and retain axis/offset fallbacks |
| `license` | Nested host licence assessment and notices; see keys below |
| `license_files` | Host selection/copy of licence documents |
| `license_from` | Host separate file/archive selection relative to source/root |
| `license_take` | Host licence archive member patterns |
| `merge` | Catalogue grouping policy; do not equate it automatically with native glyph selection/merge |
| `name` | Host family display identity; explicitly map native names when exporting |
| `name_zh` | Host Chinese display identity; no automatic localized-name synthesis |
| `pixel_offset` | Explicit global raster offset; not per-glyph offset support |
| `ppem` | Explicit native raster size, resolving filename tables before conversion; automatic grid detection can still fail |
| `provenance_url` | Host provenance link |
| `provenance_url_en` | Host English provenance link |
| `repository` | Host source repository metadata; native codec does not fetch it |
| `slug` | Host stable catalogue/output directory identity |
| `source` | Host input-root path resolution |
| `source_formats` | Host input provenance/classification; actual format selection uses native detection/explicit options |
| `source_kind` | Host source-kind metadata |
| `sprite_maps` | Translate atlas mapping geometry/cells explicitly; strict native validation differences remain |
| `take` | Host checked source copy/glob selection |
| `take_from` | Host prefixed copy/glob groups to prevent basename collisions |
| `variants` | Host per-filename display/size/weight overrides |
| `vibes` | Host search/style tags |
| `zip` | Host single archive extraction and selection |
| `zip_take` | Host archive member patterns, preserving original default policy |
| `zips` | Host ordered multiple archives |

The124 resolved instances use `instances[].name` for output stem/name,
`instances[].weight` for explicit weight metadata, `instances[].axes` for variable
instantiation and `instances[].glyph_offsets` for per-glyph outline adjustments.
The latter two require the original/external fallback. Instance declarations may
be a list or filename-to-list table; resolving the table is host responsibility.

The seven nested licence keys are `license.spdx`, `license.name`,
`license.name_en`, `license.confidence`, `license.file`, `license.note` and
`license.note_en`. Preserve identification, assessment, selected document and
localized explanatory text; editing a native licence string does not establish
permission. Per-file variant overrides `weight`, `size` and `display` remain
explicit host mapping. Sprite-map keys are `baseline`, `cell_size`, `cells`,
`characters`, `foreground`, `name`, `omit_blank` and `pixel_size`; preserve
geometry and named/unencoded cells while reconciling the stricter native rules.
Pending entries retain `source`, `reason` and `reason_en`.

## Pipeline boundaries and known substitutions

Map BDF/PCF parsing through the native registry and explicit charset APIs. Map
OTB/bitmap and vector export through the corresponding codec or typed OpenType
API. Keep catalogue orchestration, download/archive extraction, licence selection,
family/variant grouping, naming, generated README, ZIP membership, metadata
indexes, cache/stamp policy and site publication in the host application.

Native variable-font instancing, hint interpreter execution and per-glyph outline
offsets were not implemented. Keep the explicit original/external fallback for
the axis/hinted/per-glyph cases. Global grid and pixel-offset options exist; they
do not imply the unsupported per-glyph operation. Native sprite-sheet input
strictly validates both dimension divisibility, overlaps and unmapped visible
pixels; do not silently adopt the original producer's permissive behavior.

No `reference/fontpixel/pipeline/rust` or other `opf_vector` Rust source/binary
was supplied. Only `vectorize.py` and `_build_vector_ttf_reference` were present.
The old wrapper signature is `build_ttf(glyph tuples, pixel_size, ascent, descent,
shape, name records, OS/2 ranges, timestamp)`; each glyph tuple is
`(cp, bbw, bbh, bbx, bby, dwidth, rows)`. This is an ABI mapping, not a source
audit or benchmark of the missing production extension. Label any comparison
against the pure fontTools oracle precisely.

## Known output differences requiring explicit policy

- Catalogue vectors use `floor(1000 / pixel_size)` design units per source pixel,
  with actual em equal to that value times pixel size. The old `round` shape
  corresponds to dot diameter 1.0; native Tong-style dot defaults are 0.9.
- Old downloads disable WOFF2 glyf/hmtx transforms and choose quality 9 only when
  sfnt is larger than 1,500,000 bytes, otherwise 11. Native defaults enable
  transforms and use quality 11. Configure typed options intentionally.
- Old bitmap TTF has empty glyf and upem 1000; native bitmap-only OTB has a
  different table/profile arrangement. This is not automatically equivalent for
  every consumer. Metadata/default/metric comparisons remain open as above.
- Old BDF ingestion takes the first duplicate; native effective lookup takes the
  last file-order glyph while retaining all mappings. Preserve the desired
  ingestion policy explicitly.
- Native legacy codes remain tagged and use an explicit charset; do not mistake
  them for Unicode or silently copy an old repair/remapping heuristic.
- Reader, transformation and writer loss reports must all survive. Strict mode
  rejects accumulated loss before exposing output assets.

`Font.names.family/style/copyright` and relevant typed native metadata options
must be mapped deliberately. Their existence does not establish that the earlier
catalogue adapter actually populated them. The missing bitmap metadata check is
a concrete open migration issue.

The current reconstructed source adds `metadata::NameOverrides` and
`metadata::apply_names`, plus convert naming flags (`--family`, `--style`,
`--full-name`, `--postscript-name`, `--font-version`, `--copyright`, `--license`,
`--license-url`, `--foundry`). These are source-inspected and uncompiled/untested.
They change normalized names with a structured audit; original raw properties,
XLFD and provenance remain separate. Explicit family/style changes clear stale
derived full/PostScript names unless replacements are provided. Do not infer
weight, permissions, raw-record rewriting or complete catalogue parity from them.
