# Unit E acceptance: catalogue parity through explicit options

Scope: explicit bitmap projection (FONT/XLFD, SIZE, bounded property edits,
existing SWIDTH), PCF writer metadata precedence, vector `.notdef` policy, CLI
and batch wiring, and a new catalogue adapter profile. Defaults never change.

Design sources: `WORK/owners/w66/CATALOGUE-BITMAP-PROJECTION-PROPOSAL.txt`
(§4 API, §5 provenance/strict, §6 budget/transaction, §7 a–f),
`WORK/owners/w63/CATALOGUE-ACTUAL-FIX-PROPOSALS.txt` (item 2, `.notdef`),
`docs/LOCAL-PAUSE-20261005.zh.md` §5.

## Checklist

### Library: `fontpixel::projection`
- [ ] E1 `BitmapProjection::default()` changes nothing (no model change, empty outcome).
- [ ] E2 `font_name`: sets `names.xlfd` and resyncs `names.xlfd_fields` with the
      reader's rule (leading `-`, split on `-`, empty fields kept); an existing
      `FONT` property is kept in sync. Rejects empty text, line breaks, NUL,
      length above the table-byte bound.
- [ ] E3 `size { point_size: Rational, resolution: Vector }`: sets
      `metrics.point_size`/`metrics.resolution`; positive values only; BDF SIZE
      follows through `reconcile`.
- [ ] E4 Bounded typed property edits `Set{name, Integer|Text}` / `Remove{name}`:
      at most 64 edits, one edit per property name, token names only; rejects
      `COMMENT`, every notice-like name (COPYRIGHT/NOTICE/LICENSE/LICENCE),
      internal `bdf.`/`pcf.` provenance records, `FONT` set (use `font_name`),
      and names owned by normalized fields that are not integer metrics
      (names, charset, spacing, default char). Integer-metric keys
      (PIXEL_SIZE, FONT_ASCENT, FONT_DESCENT, …) keep the normalized model field
      in sync.
- [ ] E5 Impacts: replacing an existing different value = Loss (strict
      rejects); adding an absent value = Repair; removing a record = Loss;
      equal value = no change, no entry.
- [ ] E6 Full audit: one typed change record per changed field/property record
      (strike, field, before, after) plus per-change report entries; SWIDTH
      audit unchanged.
- [ ] E7 Transaction: validate and stage everything for every strike first;
      the commit only moves staged values (no allocation, no fallible step);
      any validation or budget failure leaves the `FontSet` equal to before.
- [ ] E8 Budget: all staged strings, replacement property lists, change
      records and report entries are charged to `max_working_bytes` before
      allocation; `ProjectionOutcome::retained_bytes` covers the audit.

### PCF writer
- [ ] E9 `PcfWriteOptions::metadata_precedence`, default
      `MetadataPrecedence::NormalizedEdits` (byte-identical existing output).
- [ ] E10 `ExplicitProperties`: existing POINT_SIZE / RESOLUTION_X /
      RESOLUTION_Y / RESOLUTION properties win over projected model size;
      missing ones are still generated from the projected model; FONT stays in
      sync with an explicit font-name edit; each kept property that disagrees
      with the projected model is reported as Loss. Source snapshots untouched.

### Vector `.notdef`
- [ ] E11 `NotdefPolicy::{Preserve (default), Empty{advance_width, left_side_bearing}}`
      in design units, in a separate new file; Empty writes an empty outline at
      GID 0 with exactly those hmtx values; replacing a source `.notdef`
      (single-font export path) reports Loss; replacing the generated box
      reports nothing lossy.
- [ ] E12 Hookup lines in unit C's files (variants export, opentype CLI) kept
      minimal, in a separate commit, and listed for the coordinator.

### CLI / batch
- [ ] E13 `convert`: `--font-name`, `--point-size`, `--resolution X,Y`,
      `--set-property NAME=VALUE` (repeatable), `--remove-property NAME`
      (repeatable), `--pcf-metadata-precedence`; batch `[tasks.options]`
      equivalents; projection diagnostics join the report before strict.
- [ ] E14 `variants`: `--notdef empty --notdef-advance N [--notdef-lsb L]`.
- [ ] E15 Marked slot for unit D's embedded flag group.
- [ ] E16 Without the new options CLI output is byte-identical (tested).

### Catalogue adapter
- [ ] E17 New `--profile explicit` (old profile remains default): BDF and
      rebuilt-PCF tasks get `swidth_ppem`, `font_name`, `point_size`,
      `resolution`, PCF `explicit-properties`; Proggy PCF download converts
      the source PCF directly (no charset→BDF detour); vector tasks get the
      empty `.notdef` with `max(unit*px//2, 1)` advance and `vendor = "????"`.
- [ ] E18 Native batch run with the new profile under
      `local-work/strict-e/`, validated against the original Python downloads;
      before/after counts per field and classification rule; every remaining
      difference classified as original-lossy (Python file:line), derived, or
      consumer limitation, or attributed to the owning unit.

### Gates
- [ ] Focused tests, `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`,
      `cargo fmt --all --check`. No `#[allow]`, no default-cap change, no
      deleted report entries, no COMMENT/notice removal.
