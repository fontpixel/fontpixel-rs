# Unit F acceptance: vector and bitmap-TTF residual metadata

Base: main `f8593de` (units A–E merged). Branch `strict-f-residual-metadata`.
Scope: close E's and D's residual catalogue lists with explicit, typed,
budgeted options, or classify each remaining field as original-lossy, derived
or consumer limitation with file:line evidence. `WORK` =
`local-work/phase6-local-20261004`, `F` = `local-work/strict-f`. Python line
numbers refer to `WORK/performance-work/reference/fontpixel/pipeline/opf/`;
fontTools line numbers to `local-work/oracles/lib/python3.12/site-packages/fontTools/`
(the oracle's pinned version).

## Constraints

- C1 Defaults unchanged: no-option output byte-identical (unit tests and the
  181 default-profile catalogue downloads vs `WORK/full-correctness-v2/catalogue-native/downloads`).
- C2 No default cap raised, no `#[allow]` added, no report entry removed.
- C3 Lossy original behaviour is never a default. An explicit option that
  overwrites a source value reports `Loss` (strict rejects); deriving an absent
  value is not loss.
- C4 Source COMMENT/copyright text is never deleted.
- C5 Every new owned allocation is charged before allocation to the caller's
  ledger (variants: the shared `working::Budget`; embedded: D's writer-limit
  arithmetic; CLI: argument strings and built tables).

## Checklist

### Shared
- [x] S1 Name-record parser moved from D's `p3_formats/embedded_metadata.rs` to
      `crates/fontpixel-cli/src/name_records.rs` (`number`, `record_text`,
      `record_hex`, `Records::{table, argument_bytes}`); `--otb-name-record*`
      and `variants --name-record*` both call it (D's messages unchanged; the
      hex-flag hint names the flag set). Tests: `name_records::tests::*`,
      D's `embedded_metadata` CLI tests unchanged and passing.
- [x] S2 `sfnt::cmap::FullRecords {Always, NonBmpOnly, WhenNeeded}` +
      `encode_with_records`; `encode`, `encode_records(bool)` and
      `encode_with_budget` bytes unchanged; `EmbeddedCmapUnicodeFull` is now an
      alias of it. `WhenNeeded` keeps U+FFFF (format 12 is written whenever
      format 4 cannot hold the map). Test
      `cmap::tests::record_policies_keep_every_mapping_and_the_default_bytes`.
- [x] S3 `sfnt::os2::{check_embedding_flags, check_selection_flags,
      check_code_page_ranges}` used by embedded (same messages) and variants.

### Vector (`variants::Options::metadata: MetadataOptions`, `variants` CLI + batch)
- [x] V1 Exact name table: `--name-record P/E/L/ID=TEXT`, `--name-record-hex`,
      `--name-language-tag` (D syntax) fill the existing `Options::names`.
      Loss `variants.metadata_name` when the table drops a source name the
      default would write (copyright/license/URL without overrides, family
      without `--family`); compared without allocation.
- [x] V2 `--fs-type`, `--fs-selection` (OS/2 v3: bits 7–9 rejected),
      `--break-char`, `--code-page-ranges A,B` (reserved bits rejected).
- [x] V3 `--created` / `--modified` (each requires the other).
- [x] V4 `--fixed-pitch true|false`.
- [x] V5 `--max-zones 1|2` (TrueType only), `--gasp keep|omit`,
      `--cmap-unicode-full always|non-bmp|when-needed`.
- [x] V6 `--glyph-order planned|codepoint`, `--dot-glyph last|after-notdef`
      (TrueType dot only). Renumbering remaps cmap, composites, hmtx/vmtx,
      post names and embedded strikes; it is rejected when the plan has GSUB
      regional alternates (documented limitation, not needed by the catalogue).
- [x] V7 Tests `tests/strict_f_variants.rs` (6): explicit defaults byte- and
      report-identical; every control reaches its field + Info audit; renumbered
      outlines/advances identical; validation errors; Loss for dropped notice;
      ledger thresholds (exact one-byte-short probe; renumbering and exact names
      raise the minimum). Unit `variants::metadata::tests`. CLI
      `tests/strict_f_cli.rs::variants_metadata_flags_*` (CLI = batch bytes,
      strict rejects the dropped notice, every validation message).
- [x] V8 Budget: renumbering keys/positions/reordered source array and the
      remapped cmap are reserved on the export ledger before allocation; the CLI
      reserves argument strings and the built table
      (`OutlineMetadataArguments::reserved_bytes`, exact boundary test) before
      reading inputs and subtracts them from every later limit.

### `.notdef` on `convert --to ttf/otf/woff/woff2/ttc`
- [x] N1 `--notdef preserve|empty`, `--notdef-advance`, `--notdef-lsb` set
      `vector_raster::WriteOptions::outline.notdef`; rejected for non-OpenType
      and SVG output; `variants.notdef.replaced` loss for a source `.notdef`.
- [x] N2 `strict_f_cli::convert_notdef_policy_reaches_ttf_and_reports_replacement`
      (default = explicit preserve bytes, metrics, strict rejection, batch).

### Bitmap TTF (`EmbeddedMetadataOptions`, `--otb-*`)
- [x] B1 `outline_tables: EmptyGlyf` (`--otb-outline-tables empty-glyf`): one
      zero `glyf` byte (fontTools `_g_l_y_f.py:179-184`: OTS/Windows reject an
      empty table), short `loca`, `maxp` 1.0; outline-derived head bounds,
      hmtx LSBs and hhea extrema are zero; `max_zones` (`--otb-max-zones`,
      requires empty-glyf, default 1).
- [x] B2 `notdef: Empty{advance_width}` (`--otb-notdef empty
      --otb-notdef-advance N`): no glyph-0 bitmap, exact hmtx advance; Loss
      `embedded.notdef_replaced` for a source `.notdef`.
- [x] B3 `caret_slope`, `line_extremes: Zero`, `vertical_line_metrics:
      MirrorHorizontal` (`--otb-caret-slope R,R`, `--otb-line-extremes`,
      `--otb-vertical-line-metrics`); a replaced nonzero retained vertical record
      is Loss.
- [x] B4 `underline: Option<[i16;2]>` (`--otb-underline P,T`); Loss
      `embedded.underline` when it replaces a source underline metric.
- [x] B5 `hmtx: CompactTrailing` (`--otb-hmtx compact-trailing`).
- [x] B6 cmap `when-needed` (S2).
- [x] B7 `glyph_order: Codepoint` (`--otb-glyph-order codepoint`) and
      `EmbeddedIndexLayout::Spanning` (`--otb-index-layout spanning`,
      `sbit::encode_spanning`: zero-length index-format-1 entries for absent IDs;
      decoded strikes identical). Strike order stays the FontSet order; the
      adapter passes inputs in the original's pixel-size order.
- [x] B8 Tests `tests/strict_f_embedded.rs` (8): explicit defaults byte- and
      report-identical; each control's table effect with unchanged decoded
      bitmaps; Loss cases incl. strict registry rejection; validation errors
      (also color); exact writer-budget growth = `working_bytes` (outline tables
      `2(n+1)+33`, reorder `n·(8+2+24)`). CLI
      `strict_f_cli::otb_bitmap_ttf_flags_*` (bytes = batch, rejections).
      D's `phase6_embedded_options` (13) and CLI `embedded_metadata` (2) pass.

### Adapter and parity
- [x] P1 `benches/phase6_catalogue.py --profile explicit` now emits the full
      sfnt policy for bitmap TTF (previously no `explicit` options reached
      `ttf` tasks) and vector tasks; values and citations in the code and in
      `docs/migration-fontpixel.md#explicit-sfnt-profile`. Name records are
      computed from `name_records()` with the `setupNameTable` platform rule;
      text Mac Roman cannot encode raises instead of guessing the ltag path.
      `benches/phase6_catalogue_test.py` 10 tests.
- [x] P2 Release `--all-features` build at `e4f83b7`
      (`F/binaries.sha256`); native explicit batch `F/catalogue-native`
      (173/173), validator `F/catalogue-native-proof.json`, comparison
      `F/comparison.json` (tools in `F/tools/`).
- [x] P3 Classification `F/classification.json` (`F/tools/classify.py`, every
      rule re-checked on the record; zero unclassified) and
      `F/recompute-dropped-glyphs.json`.
- [x] P4 Default profile `F/catalogue-native-default`: all 181 downloads
      byte-identical to `WORK/full-correctness-v2/catalogue-native/downloads`.

### Gates
- [x] G1 Focused tests (above plus `variants`, `variants_ledger`,
      `variants_metadata`, `subset`, `outlines`, lib, CLI bins/projection_cli);
      `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`;
      `cargo clippy --locked -p fontpixel-cli --all-targets --no-default-features
      --features bdf -- -D warnings` (also `bdf,otb` and `bdf,opentype`);
      `cargo fmt --all --check`. No `#[allow]` added.

## Results

| | Unit E | Unit F |
| --- | ---: | ---: |
| Unresolved font files | 132 | 73 |
| Failed artifacts | 1 | 1 (consumer-01) |
| Unresolved archives | 3 | 3 (BDF members' COMMENT) |
| Vector TTF pass | 0 / 42 | 42 / 42 |
| Bitmap TTF pass | 0 / 21 | 17 / 21 |
| WOFF2 pass | 0 / 41 | 0 / 41 (checksum only) |
| BDF/PCF pass | 40 / 68 | 40 / 68 |

Validity: the catalogue validator runs OTS on every vector TTF and WOFF2
(all native pass) but not on bitmap TTFs; `F/tools/ots_bitmap.py` ran OTS
on the 21 explicit-profile bitmap TTFs (empty glyf, caret 0/1, mirrored
vertical records, spanning index): 21/21 native and 21/21 original pass
(`F/ots-bitmap-ttf.json`). FreeType rasters of every encoded glyph match
(validator `encoded_raster_metrics`). Workspace tests
(`cargo test --workspace --all-features`): 1,574 passed, 0 failed.

Field occurrences by rule, E → F: app-01 104 → 0; app-02/app-03/repr-03
287 → 8; app-04 535 → 3; app-07 312 → 0; repr-02/repr-03 21 → 4; repr-03
712 → 8; app-06 56 → 56; repr-01 41 → 41.

### Every remaining field

- original-lossy, COMMENT (56 = 28 BDF × `headers.comment` + COMMENT raw
  records): `parsers/bdf.py` stores no COMMENT and `ingest/bdfwrite.py:16-54`
  writes none; native keeps the notices (E's classification; the 3 archives
  inherit it).
- derived, checksum (41 WOFF2 × `head.checkSumAdjustment`): every other field,
  outline and raster of these fonts matches and their TTFs pass. Observed:
  the original's WOFF2 head adjustment differs from its own TTF's
  (`tom-thumb-6px-square`: TTF 2962985748, WOFF2 2694500144), while native
  WOFF2 carries its TTF's value. Same rule as E (repr-01); the encoder-side
  mechanism was not traced further.
- original-lossy, unencoded glyphs (23 = 4 files): `ttfexport.py:65-71` builds
  the glyph order from encoded code points only, dropping `.null`/
  `nonmarkingreturn` (3x5-block ×2), `nonmarkingreturn` (Capsigrany) and two
  unnamed glyphs (Tinypixel). Native keeps them; dropping source glyphs is not
  offered as an option. Consequences, each re-checked without the dropped
  glyphs: `maxp.numGlyphs`, `hhea.numberOfHMetrics` and `OS/2.xAvgCharWidth`
  equal the original (`F/recompute-dropped-glyphs.json`); unencoded glyph
  records equal except the dropped ones; EBLC names equal after removing them,
  differing only in `endGlyphIndex`/`indexTablesSize`.
- consumer limitation (1): `spleen-64px-round.woff2`, FreeType error 98
  (consumer-01, unchanged).

Lossy explicit options the profile uses, all reported and rejected by
`--strict`: replaced source `.notdef` (3x5-block ×2, Capsigrany, Tinypixel),
post names omitted (all bitmap TTFs), Cozette underline replaced by 0,
dropped source name fields, SWIDTH projection (E).

### Notes

- Variants renumbering with GSUB alternates is rejected rather than remapping
  GSUB lookups.
- The exact-name Loss audit (variants) compares Mac 1/0/0 records only for
  ASCII text; a non-ASCII notice carried solely by a Mac record is reported as
  Loss (conservative).

### Merge notes (files outside the obvious unit-F surface)

- `crates/fontpixel/src/sfnt/cmap.rs`: `FullRecords`, `encode_with_records`,
  `encode_records_with_budget` now `pub(crate)` with an enum; format-12 writing
  split into `format12`. `encode`/`encode_records`/`encode_with_budget` bytes
  unchanged (subset path untouched).
- `crates/fontpixel/src/sfnt/os2.rs`: shared `check_*` functions.
- `crates/fontpixel/src/sfnt/bitmap.rs`: `encode_spanning` and private
  `encode_layout`; `encode`/`encode_with_budget` unchanged.
- `crates/fontpixel/src/formats/embedded.rs` (write_impl strike loop,
  `common_design_tables`, new `reorder_by_codepoint`, `apply_line_metrics`) and
  `embedded/metadata.rs` (new enums/fields, `working_bytes`, `prepare`,
  `audit`); `EmbeddedCmapUnicodeFull` became a type alias.
- `crates/fontpixel/src/variants/export.rs` (`encode_planned`: renumbering,
  table controls, `audit_metadata`; `Notes::info`), new `variants/metadata.rs`.
- `crates/fontpixel-cli/src/p3_formats/embedded_metadata.rs` (parser moved to
  `name_records.rs`, new `--otb-*` flags), `main.rs` (`mod name_records`,
  `mod outline_metadata`, `VectorArguments` notdef flags), `opentype.rs`
  (`variants` reserves metadata arguments, passes `names`/`metadata`).
- Struct literals gained `..Default::default()`:
  `tests/phase6_embedded_options.rs::tong_like`,
  `fontpixel-cli/tests/embedded_metadata.rs`.
- `w6.1-cli.md`/`api.md` were not extended; `--help` documents every flag and
  `docs/migration-fontpixel.md` has the option table.
- The reduced-profile clippy failure noted in the pause file (`batch.rs`
  `with_source_scope`) no longer reproduces on this base.
