# Unit D acceptance: embedded-bitmap sfnt (OTB/bdat) metadata API and CLI

Base: main `6297998`. Scope: explicit, typed, budgeted metadata controls for
native OTB/bdat output, CLI/batch flags, and a parity proof against the 20
export-rs Tong OTBs. Color (CBDT/sbix) output rejects every nondefault control.

## Non-negotiable constraints

- C1 Default output is byte-identical: `EmbeddedWriteOptions::default()` and a
  CLI run without the new flags produce the same bytes and the same report as
  `6297998`. Proven by unit tests and by regenerating the 20 default Tong OTBs
  and comparing their SHA-256 with the `native` identities recorded in
  `full-correctness-v2/tong-batch-default-proof.json`.
- C2 No default cap is raised; no `#[allow]` is added; no existing loss/report
  entry is removed or downgraded; strict mode still rejects every loss.
- C3 Every new owned table/string is charged: the explicit name table (retained
  capacity, Arc header, encode scratch, encoded output) is charged inside the
  writer against `limits.sfnt.resources.max_total_bytes`; CLI-owned argument
  strings and the built table are exposed through
  `p3_formats::WriteArguments::reserved_bytes` for the caller ledger (main.rs
  hookup listed for the coordinator, since main.rs belongs to unit E).
- C4 No lossy behaviour of export-rs or the catalogue becomes a default. Every
  option is explicit; omitted source notices, replaced names and omitted post
  glyph names are reported as `Loss`; applying any control adds an `Info` audit.
- C5 No family-name inference in core (Tong "Mono" is decided by the adapter).

## Library API checklist (`fontpixel::formats::embedded`)

`EmbeddedWriteOptions.metadata: EmbeddedMetadataOptions` (Default = native):

- [x] M1 `names: Option<Arc<NameTable>>` complete replacement (any platform,
      encoding, language, name ID incl. 3/6/13/14/16/17, language tags). Validated:
      platform/encoding pairs, UTF-16 well-formedness, PostScript name syntax,
      language index range, BCP-47 syntax of tags, u16 offsets. Encoded without
      string deduplication (records in canonical key order, duplicates stable).
- [x] M2 `head_flags` (bit 15 rejected), `lowest_rec_ppem` (>0),
      `timestamps: Option<[i64; 2]>` (created, modified LONGDATETIME).
- [x] M3 OS/2: `os2_version` (3 or 4 only: same 96-byte layout), `vendor_id`
      (printable ASCII), `panose`, `unicode_ranges` (policy: coverage default,
      exact bits, or non-BMP-only bit 57), `break_char`, `embedding_flags`
      (fsType; reserved bits rejected), `selection_flags` (fsSelection),
      `average_width` (nearest default / floor).
- [x] M4 post: `post_names` keep/omit (format 3), `fixed_pitch` override.
- [x] M4a cmap: `cmap_unicode_full` always (default) / non-BMP-only: omit the
      redundant (0, 4) record for BMP-only maps (`sfnt::cmap::encode_records`;
      `cmap::encode` and `encode_with_budget` unchanged). Added after the parity
      run showed export-rs writes no (0, 4) record for its 4 BMP-only Latin OTBs.
- [x] M5 EBLC/bloc: `width_max` bitmap width (default) / horizontal advance;
      `index_layout` canonical (default) / shared-metric runs (export-rs layout:
      >=N identical-metric consecutive GIDs -> index 2/image 5, rest -> index 3/
      image 2, 16-bit offsets split before 64 KiB). Horizontal-only small metrics
      required; zero vertical metrics in image-5 runs are reported.
- [x] M6 Diagnostics: one `Info` audit when any control is set; `Loss` for each
      source name field (IDs 0,1,2,4,5,6,8,13,14) whose text is absent from the
      explicit table, and for post-name omission when source names exist.

## CLI / batch checklist (`p3_formats`)

- [x] F1 `--otb-name-record P/E/L/ID=TEXT` (repeatable; Unicode/Windows UTF-16,
      Mac Roman for 1/0, strict encoding), `--otb-name-record-hex P/E/L/ID=HEX`,
      `--otb-name-language-tag TAG`; any record => complete replacement.
- [x] F2 `--otb-head-flags`, `--otb-lowest-rec-ppem`, `--otb-created`,
      `--otb-modified`.
- [x] F3 `--otb-os2-version`, `--otb-vendor`, `--otb-panose`, `--otb-unicode-ranges`,
      `--otb-break-char`, `--otb-fs-type`, `--otb-fs-selection`,
      `--otb-average-width`.
- [x] F4 `--otb-post-names`, `--otb-fixed-pitch`, `--otb-cmap-unicode-full`.
- [x] F5 `--otb-width-max`, `--otb-index-layout`, `--otb-index-min-run`.
- [x] F6 Rejected for non-otb/bdat outputs and missing features; batch spellings
      are the underscore names (arrays repeat); help text with an example.

## Parity proof checklist

- [x] P1 `benches/phase6_tong.py batch --profile parity` (default profile
      unchanged) regenerates the 20 OTBs with explicit per-file options.
- [x] P2 Field-by-field comparison with `phase6_tong_metadata.bitmap_metadata`
      (global tables, names, cmap, all strikes/GIDs, raw timestamps) plus FreeType
      pixel checks, for all 20 files.
- [x] P3 Every remaining difference classified: derived (checkSumAdjustment) or
      original-lossy with justification; nothing waived generically.
- [x] P4 Catalogue bitmap-sfnt: counts of fields the options can close vs fields
      outside this unit (glyf/loca, glyph set, LSB, extents, vert line metrics).

## Tests

- [x] T1 Default bytes/report identical for OTB and bdat, explicit default too.
- [x] T2 Each option round-trips through decode of the written tables.
- [x] T3 Every validation error; color rejection; CLI non-otb rejection.
- [x] T4 Budget: name-table encode exact boundary and one byte short; writer
      minimum budget grows by exactly the extra spare capacity of the table.
- [x] T5 Strict rejects copyright replacement and post-name omission; existing
      loss entries remain.
- [x] T6 Shared-run index layout readback equals canonical decode (pixels,
      horizontal metrics) and matches the export-rs byte structure.

## Results (2026-10-05, commit on branch `strict-d-embedded-metadata`)

Evidence directory: `local-work/strict-D-evidence/` (outside the repository).
Binary: `fontpixel-all-features` (same `--all-features` build as frozen v2/v3).

### Default unchanged (C1)

`phase6_tong.py batch` (default profile) with the new binary: all 84 artifacts
byte-identical to `full-correctness-v2/tong-batch-default/export`, and the 40
OTB/PCF SHA-256 values equal the `native` identities recorded in
`tong-batch-default-proof.json` (`default-identity.json`). The default
`jobs.toml` is unchanged, and the complete batch stdout (all 84 reports) is
identical to v2 after path normalization (`report_identity.py`).

### Tong parity (P1-P3)

`phase6_tong.py batch --profile parity --parity-timestamps <reference otb dir>`
adds per-OTB options: 11 Windows name records (strings read from export-rs
`otb.rs` `name_table`), vendor TONG, PANOSE 2,11,5,(9 Mono / 3), head flags 3,
lowestRecPPEM 14, break char 32, Unicode ranges non-bmp, floor average, post
omit, fixed pitch (Mono by the export-rs family-prefix rule, decided in the
adapter), cmap (0,4) non-bmp, widthMax advance, shared-runs index layout
(minimum run 8), and created/modified copied from each reference file.

`benches/phase6_tong_otb_parity.py` (complete `phase6_tong_metadata`
inventory + FreeType pixel gate) over the 20 OTBs (`tong-batch-parity-otb-*`):

- every table except `head` is byte-identical to export-rs; `head` differs only
  in `checkSumAdjustment`; the table directories are identical except table
  data offsets (native writes table data in tag order, write_fonts in its
  recommended order); both adjustments are valid for their own bytes;
- name records, cmap records/GIDs, all global fields, every strike's size
  fields, index subtables and all 633,028 physical glyph records are equal;
  FreeType pixels/advances equal for 633,028 encoded glyphs;
- unresolved: none. The only differing field is `head.checkSumAdjustment`,
  classified derived (placement order of identical tables).

Of the 18 differing global fields plus names/strikes/cmap of the default run,
all are now equal except the derived checksum. Without `--parity-timestamps`,
`head.created/modified` differ in 20 files (export-rs wall clock, volatile).

Original-lossy behaviour reproduced only under explicit options, and reported
by the native writer as `Loss` (strict would reject): source BDF copyright
(ID 0) replaced by export-rs' shorter notice and FOUNDRY (ID 8) omitted
(40 `embedded.metadata_name`); post format 3 drops glyph names (20
`embedded.post_names`). Every default-run report entry remains.

### Catalogue bitmap sfnt (P4, quick run)

`catalogue_bitmap_parity.py`: the 21 native `--to otb` catalogue tasks rerun with
option values read from each original download. Closed field categories (files):
OS/2 achVendID/fsSelection/fsType/usBreakChar/version (21 each), head
flags/lowestRecPPEM/created/modified (21), name_records (21), post
formatType/extraNames/glyphOrder/mapping (21), post.isFixedPitch (15), EBLC
widthMax values (8; the remaining spleen difference is strike order).
Remaining, outside this unit's options: glyf/loca tables and maxp 1.0 outline
fields (21), glyph set/.notdef (numGlyphs 4, numberOfHMetrics 19), bitmap-derived
head/hhea extents and bearings, xAvgCharWidth (5, glyph-set driven), cmap
records (21: fontTools writes format 4 only), post underline (2), mirrored vert
line metrics and strike order, checkSumAdjustment.

### Not closed / notes

- main.rs (unit E) hookup, optional: the CLI already subtracts
  `WriteArguments::reserved_bytes` from the embedded writer's budget inside
  `p3.apply`; to also cover the read phase add, before the convert group loop:
  `conversion_charge(&mut retained, p3.reserved_bytes(limits.max_total_bytes)?, limits.max_total_bytes)?;`
  (method exists only with otb/bdat features; wrap in the same cfg).
- Native API example `phase6_tong_export` has no parity profile (not owned);
  the parity profile is the batch CLI one.
- Pre-existing reduced-profile clippy failure (not this unit):
  `batch.rs:408 with_source_scope` dead code under
  `-p fontpixel-cli --no-default-features --features bdf[,otb|bdat|color]`.
- Docs `migration-tong.md`/`migration-fontpixel.md` "Option: none" rows for OTB
  metadata are stale; left for the docs owner.
