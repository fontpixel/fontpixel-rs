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
- [ ] S1 Name-record CLI parser factored out of D's `p3_formats/embedded_metadata.rs`
      into one module used by `--otb-name-record*` and the new variants flags.
- [ ] S2 `sfnt::cmap::FullRecords` (Always / NonBmpOnly / WhenNeeded); D's
      `encode_records(bool)` and default bytes unchanged.
- [ ] S3 OS/2 fsType / fsSelection validation shared by embedded and variants.

### Vector (`variants::Options::metadata`, `variants` CLI + batch)
- [ ] V1 Exact name table from `--name-record P/E/L/ID=TEXT`, `--name-record-hex`,
      `--name-language-tag` (D syntax); Loss for source notices dropped.
- [ ] V2 OS/2 `--fs-type`, `--fs-selection`, `--break-char`, `--code-page-ranges`.
- [ ] V3 head `--created` / `--modified`.
- [ ] V4 post `--fixed-pitch`.
- [ ] V5 maxp `--max-zones` (TrueType), `--gasp keep|omit`,
      `--cmap-unicode-full always|non-bmp|when-needed`.
- [ ] V6 Glyph order / dot helper position options.
- [ ] V7 Tests: default unchanged, effect, validation, strict Loss, budget boundary.

### `.notdef` on `convert --to ttf/otf`
- [ ] N1 `--notdef preserve|empty`, `--notdef-advance`, `--notdef-lsb` (batch too).
- [ ] N2 Tests.

### Bitmap TTF (embedded OTB metadata)
- [ ] B1 Outline table profile (empty glyf/loca + maxp 1.0) with outline-derived
      extents/bearings and explicit maxZones.
- [ ] B2 Empty `.notdef` with design advance (Loss when replacing a source .notdef).
- [ ] B3 EBLC line metrics: caret slope, min/max extremes, mirrored vertical.
- [ ] B4 post underline override (Loss when replacing source values).
- [ ] B5 hmtx trailing-advance compaction.
- [ ] B6 cmap `when-needed`.
- [ ] B7 Glyph order / index subtable layout.
- [ ] B8 Tests.

### Adapter and parity
- [ ] P1 `benches/phase6_catalogue.py --profile explicit` emits every option for
      bitmap TTF and vector tasks with values from the original's policy inputs.
- [ ] P2 Native explicit run + validator; before (E: 132 unresolved) → after per rule.
- [ ] P3 Every remaining field classified (original-lossy / derived / consumer limitation).
- [ ] P4 Default profile byte-identical to v2.

### Gates
- [ ] G1 Focused tests; clippy (all features; reduced bdf profile); fmt.

## Results

(filled in at the end)
