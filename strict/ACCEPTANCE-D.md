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

- [ ] M1 `names: Option<Arc<NameTable>>` complete replacement (any platform,
      encoding, language, name ID incl. 3/6/13/14/16/17, language tags). Validated:
      platform/encoding pairs, UTF-16 well-formedness, PostScript name syntax,
      language index range, BCP-47 syntax of tags, u16 offsets. Encoded without
      string deduplication (records in canonical key order, duplicates stable).
- [ ] M2 `head_flags` (bit 15 rejected), `lowest_rec_ppem` (>0),
      `timestamps: Option<[i64; 2]>` (created, modified LONGDATETIME).
- [ ] M3 OS/2: `os2_version` (3 or 4 only: same 96-byte layout), `vendor_id`
      (printable ASCII), `panose`, `unicode_ranges` (policy: coverage default,
      exact bits, or non-BMP-only bit 57), `break_char`, `embedding_flags`
      (fsType; reserved bits rejected), `selection_flags` (fsSelection),
      `average_width` (nearest default / floor).
- [ ] M4 post: `post_names` keep/omit (format 3), `fixed_pitch` override.
- [ ] M5 EBLC/bloc: `strike_width` bitmap width (default) / horizontal advance;
      `index_layout` canonical (default) / shared-metric runs (export-rs layout:
      >=N identical-metric consecutive GIDs -> index 2/image 5, rest -> index 3/
      image 2, 16-bit offsets split before 64 KiB). Horizontal-only small metrics
      required; zero vertical metrics in image-5 runs are reported.
- [ ] M6 Diagnostics: one `Info` audit when any control is set; `Loss` for each
      source name field (IDs 0,1,2,4,5,6,8,13,14) whose text is absent from the
      explicit table, and for post-name omission when source names exist.

## CLI / batch checklist (`p3_formats`)

- [ ] F1 `--otb-name-record P/E/L/ID=TEXT` (repeatable; Unicode/Windows UTF-16,
      Mac Roman for 1/0, strict encoding), `--otb-name-record-hex P/E/L/ID=HEX`,
      `--otb-name-language-tag TAG`; any record => complete replacement.
- [ ] F2 `--otb-head-flags`, `--otb-lowest-rec-ppem`, `--otb-created`,
      `--otb-modified`.
- [ ] F3 `--otb-os2-version`, `--otb-vendor`, `--otb-panose`, `--otb-unicode-ranges`,
      `--otb-break-char`, `--otb-fs-type`, `--otb-fs-selection`,
      `--otb-average-width`.
- [ ] F4 `--otb-post-names`, `--otb-fixed-pitch`.
- [ ] F5 `--otb-width-max`, `--otb-index-layout`.
- [ ] F6 Rejected for non-otb/bdat outputs and missing features; batch spellings
      are the underscore names (arrays repeat); help text with an example.

## Parity proof checklist

- [ ] P1 `benches/phase6_tong.py batch --profile parity` (default profile
      unchanged) regenerates the 20 OTBs with explicit per-file options.
- [ ] P2 Field-by-field comparison with `phase6_tong_metadata.bitmap_metadata`
      (global tables, names, cmap, all strikes/GIDs, raw timestamps) plus FreeType
      pixel checks, for all 20 files.
- [ ] P3 Every remaining difference classified: derived (checkSumAdjustment) or
      original-lossy with justification; nothing waived generically.
- [ ] P4 Catalogue bitmap-sfnt: counts of fields the options can close vs fields
      outside this unit (glyf/loca, glyph set, LSB, extents, vert line metrics).

## Tests

- [ ] T1 Default bytes/report identical for OTB and bdat, explicit default too.
- [ ] T2 Each option round-trips through decode of the written tables.
- [ ] T3 Every validation error; color rejection; CLI non-otb rejection.
- [ ] T4 Budget: name-table encode exact boundary and one byte short; writer
      fails cleanly with a `ResourceLimit` when the explicit table cannot fit.
- [ ] T5 Strict rejects copyright replacement and post-name omission; existing
      loss entries remain.
- [ ] T6 Shared-run index layout readback equals canonical decode (pixels,
      horizontal metrics) and matches the export-rs byte structure.
