# Unit C acceptance checklist: variants planner/export and outline primitives

Scope: `crates/fontpixel/src/variants.rs`, `src/variants/**`, `src/outline/**`,
their tests, and minimal budget-aware entry points in table codecs that export
calls. Base commit: 6297998.

Sources: `docs/w6.1-writer-budget.md` (known limitation "Variants planner and
export"), `WORK/owners/w64/NEXT-LEDGER-DESIGN.md` ("Planner/export/subset"),
`WORK/owners/w64/READONLY-AUDIT.zh.md` (P1 variants table), batch02/04 frozen
records, `variants-alias-fail-fast.PROPOSED.patch`, `zero-area.PROPOSED.patch`.

Hard constraints (all items): no default limit/cap raised; strict/loss/report
behaviour unchanged and no report entry removed; no `#[allow]`; no guessed
source credit; resource failures are `Error::ResourceLimit`; default outputs
byte-identical.

Status key: [x] done, [~] done with a stated limit, [ ] not done.

## A. Previously proposed patches

- [x] A1. The alias fail-fast patch had already landed at 6297998: lazy
  `effective`, glyph and cmap admission before insertion, and both regressions
  in `tests/variants.rs`. It was kept as is, with its error order unchanged.
- [x] A2. The zero-area patch had already landed at 6297998: `bbox` and the
  outline early returns, plus `tests/phase6_zero_area.rs`. It was kept.

## B. One ledger per operation

- [x] B1. Planning, export and both encode paths charge one
  `working::Budget` ("variants working bytes") whose maximum is
  `limits.sfnt.resources.max_total_bytes`. The CLI already reduces that value
  by retained sources and reports. Nested outline, glyf, CFF, cmap, GSUB and
  builder calls share this ledger; none starts a second full-size ledger.
- [x] B2. `variants::encode` keeps the plan's lease live during export.
- [x] B3. `PreparedVariants` records the plan's admitted retained bytes. Each
  `encode` builds its own ledger, charges the plan once, then exports.
  `retained_bytes()` and `encode_allowance()` expose the per-call allowance. The
  docs state that concurrent calls each take their own allowance and that no
  process-wide total is implied.
- [x] B4. Ledger exhaustion is `Error::ResourceLimit`; tests check every
  entry point.

## C. Planner

- [x] C1. Records hold an admitted name plus the borrowed source glyph and its
  face. Equality and signatures use `ledger::View`, which borrows the bitmap and
  resolves metrics. Each bitmap is cloned once, with admission, into the final
  glyph order.
- [x] C2. All of the following are admitted before allocation:
  exact-capacity names and labels, cmap, ordered `(signature, record)`
  buckets (a `BTreeSet`, not a `HashMap`), substitution maps, order and ID
  arrays, GSUB lookups, systems and their route arrays, feature keys and
  lists, scripts and language trees, the identity mapping, the subset model
  (`Gsub::subset_with_budget`) and the encode check (`encode_with_budget`).
- [x] C3. Maps, the unpruned GSUB model and the mapping are dropped before the
  encode check. Order, names and records are dropped once the final glyphs
  exist. Charges are released only by real drops.
- [x] C4. The plan output is unchanged: the same names, order, cmap and GSUB
  (existing tests, the new name-order test and the sentinels in F5).

## D. Export

- [x] D1. Mono bitmaps are borrowed through `Cow`. Gray8 threshold output and
  embedded-bitmap copies are admitted before allocation.
- [x] D2. The following are admitted: the sources array, metric and extent
  arrays (exact capacity), and the dot-row catalogue (pattern tree, keys,
  masks, rows, selection, admitted scratch key, unstable in-place sort). Per
  glyph, origins, remaining masks and components are admitted too.
- [x] D3. Geometry comes from `contours_with_budget`,
  `pixel_contour_with_budget`, and the new `glyf::Glyph::pixels_with_budget` and
  `cff::Glyph::from_bitmap_with_budget`. Accumulated TrueType/CFF glyphs keep
  their leases (`Nested`) until encoded. The old cumulative point, component and
  command caps are kept as extra checks.
- [~] D4. cmap, GSUB and the builder use their existing `_with_budget` APIs.
  glyf, CFF, hhea/hmtx/vhea/vmtx, maxp, head, OS/2 (with the code-page scratch),
  post, name, gasp and EBDT/EBLC run under source-derived conservative bounds in
  `variants/tables.rs`, which cover scratch plus output. Each output is shrunk
  under the scratch charge, and then only its actual capacity stays charged
  while later tables and the builder output live. The CFF charstring length
  model is exact; a unit test checks it against the writer. Limit: these bounds
  follow the current encoder sources and are listed as merge points for
  native ledger APIs.
- [x] D5. `encode_single` charges glyph copies, names (tree and strings),
  mapping, notes and the final report merge. The 1024/2x/64 estimate check is
  kept.

## E. Outline primitives

- [x] E1. In `square`, `spec` and `IndependentContours` (the W6.4 wiring, now
  verified), the packed grid, edges, outgoing, used/walk, contour points and the
  contour list are admitted before allocation; so are `pixel_origins` and
  `pixel_contour`. Point totals are admitted before any contour slot.
- [x] E2. Public `contours`, `pixel_origins`, `pixel_contour`, `Glyph::pixels`
  and `Glyph::from_bitmap` keep their signatures and results.

## F. Tests

- [x] F1. Planning has an exact threshold and keeps byte-identical output
  (`tests/variants_ledger.rs`).
- [x] F2. Exact peak and one-byte-short tests cover packed square (analytic
  bytes), the general square tracer, square-separate and dot (analytic bytes),
  origins and single contours. A further test shows that point admission comes
  before contour allocation (`outline/mod.rs` `ledger_tests`).
- [x] F3. Exact threshold, byte identity and one-byte-short tests cover square
  TTF/OTF, square-separate, dot literal/rows/patterns, dot OTF, embedded
  TTF/OTF, prepared encodes (including concurrent calls) and `encode_single`
  (square, dot, CFF, with gray and name loss notes retained).
- [~] F4. A counting `GlobalAlloc` harness (`strict/alloccheck-C`) lives
  outside the workspace because the workspace forbids `unsafe`. The measured
  heap never exceeds the minimal ledger for any fixture case or the full Tong
  cases (see G1). It is not a `cargo test` inside the workspace.
- [~] F5. Existing variants, variants_metadata, outlines, zero-area,
  vector_raster, glyf, cff, subset, nfnt, embedded_bitmap, sfnt and lib tests
  pass, as do CLI `cli` and `phase6_cli_recovery`. All CLI sentinels are
  byte-identical to 6297998 at the default limits: square-separate TTF/OTF, dot
  literal/rows/0.8, embedded TTF/OTF, Small regional+embedded, and convert
  TTF/OTF/dot. A same-label suffix case (Small zh-Hant=TC, zh-HK=KR, plus a
  second script face, square TTF and dot OTF at 2 GiB) also matches.
  Full Tong Large regional square/dot TTF/OTF is byte-identical at
  its minimal `--max-total-bytes`, but **no longer fits the CLI default of
  256 MiB** (G1).
- [x] F6. `cargo clippy --locked -p fontpixel --all-targets --all-features -- -D warnings`
  is clean, as are the default-feature and `bdat`-only profiles.
  `cargo fmt --all --check` is clean.

## G. Reporting

- [x] G1. Full Tong Large regional export (SC base; TC as zh-Hant and zh-HK; JP;
  KR; Latin script; documented flags; default options). L is the minimal CLI
  `--max-total-bytes`. W is the library ledger at L, that is, L minus the
  identity slice and the retained sources/report.

  | case | baseline L (6297998) | new L | new W (plan+export) | measured heap | default W left |
  | --- | --- | --- | --- | --- | --- |
  | square TTF | 262,635,981 (250.46 MiB) | 397,039,584 (378.64 MiB) | 143,638,544 | 132,478,001 | 15,034,416 |
  | square OTF | (same source bound) | 508,272,748 (484.72 MiB) | 254,871,708 | 239,533,120 | 15,034,416 |
  | dot TTF | (same source bound) | 362,734,963 (345.93 MiB) | 109,333,923 | 98,621,375 | 15,034,416 |
  | dot OTF | (same source bound) | 389,283,764 (371.24 MiB) | 135,882,724 | 117,102,369 | 15,034,416 |

  The baseline needed only source retention, because export was not charged.
  Baseline peak RSS was 232 MB for sources only, 372 MB for square TTF, 332 MB
  for dot TTF and 550 MB for square OTF. So export really used 100–318 MB
  above sources even before this change. Through the library with
  `ResourceLimits::default()` (256 MiB, sources not charged), all four fit:
  their minimal totals are the W column, the largest being 254.9 MB of
  268.4 MB. The CLI default leaves 15.0 MB after its 252 MB source reservation,
  so the strict ledger rejects the default full-Tong CLI export. Coordinator
  decision needed.
- [x] G2. `working.rs` additions are `Lease::split` (sfnt/test only) and the
  test-only `Budget::peak`; the `reserve` body records the peak under
  `cfg(test)`. Codec merge points: `glyf::Glyph::pixels_with_budget`,
  `cff::Glyph::from_bitmap_with_budget`, and the `variants/tables.rs` bounds for
  `GlyfTable::encode`, `cff::Font::encode`, `metrics::encode`,
  `os2::code_page_ranges`, `Os2Table::encode`, `PostTable::encode`,
  `NameTable::encode`, `GaspTable::encode`, `Statistics::unhinted_maxp` and
  `bitmap::encode`. `Gsub::subset_with_budget`, `cmap::encode_with_budget` and
  the GSUB decode check still use the unverified `Budget::reserve_tree`
  formula. `tables::code_pages` uses one fixed 4096-byte scratch bound for
  the charset-codec key strings, not a source-derived size. Merge hotspot:
  `Budget::reserve` now records the test-only peak. `ledger::tree_nodes`
  copies `model::budget::tree_node_bound`, which could be made `pub(crate)`
  and shared.
