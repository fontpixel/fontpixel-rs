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

## A. Previously proposed patches

- [ ] A1. Alias fail-fast patch: confirm landed at 6297998 (lazy `effective`,
  pre-insertion glyph and cmap admission, both regressions present).
- [ ] A2. Zero-area patch: confirm landed at 6297998 (`bbox` and outline
  early returns, `tests/phase6_zero_area.rs`).

## B. One ledger per operation

- [ ] B1. Planning, export and `Variants`/`PreparedVariants` encode charge one
  `working::Budget` whose maximum is the caller's remaining allowance
  (`limits.sfnt.resources.max_total_bytes`, which the CLI already reduces by
  retained sources and reports). No nested helper starts a second full-size
  ledger.
- [ ] B2. `Variants::encode` keeps the retained plan's charge live during export.
- [ ] B3. `PreparedVariants` owns the plan and records its retained bytes. Each
  `encode` call gets its own ledger that charges the retained plan once plus its
  own working storage. The per-call allowance is exposed and documented; there
  is no claim that one total bounds concurrent calls.
- [ ] B4. Ledger exhaustion returns `Error::ResourceLimit`.

## C. Planner

- [ ] C1. Equality and signature checks for alternatives borrow the source
  bitmap plus resolved metrics. Bitmaps are cloned only for retained plan glyphs,
  and the clone is admitted first.
- [ ] C2. Records, names (exact string capacity), cmap, signature buckets,
  substitution maps, glyph IDs, layout (systems, feature IDs, lookups, scripts,
  identity mapping, subset model, encode check) are admitted before allocation.
  Internal signature buckets use an ordered table, not `HashMap`.
- [ ] C3. Temporary maps are dropped before the GSUB subset/encode check, so
  their credit is released by real drops only.
- [ ] C4. Plan output is byte-identical: the same glyph order, names, cmap and
  GSUB.

## D. Export

- [ ] D1. Mono bitmaps are borrowed rather than cloned. A Gray8 threshold output
  and the embedded-bitmap copy are admitted before allocation.
- [ ] D2. Sources, metrics arrays, extents, the dot-row catalogue (patterns,
  masks, rows, selected), per-glyph origins, remaining masks and components are
  admitted.
- [ ] D3. Outline geometry is produced by the `_with_budget` primitives on the
  export ledger. The accumulated TrueType/CFF glyph storage keeps its leases
  until the tables are encoded. The existing cumulative point/component/command
  caps stay as additional checks.
- [ ] D4. Every table encoder call is admitted before it runs: either through
  an existing `_with_budget` API (cmap, GSUB, builder) or through a documented
  conservative bound for its scratch and output. Table bytes stay charged while
  later tables and the builder output are live.
- [ ] D5. `encode_single` (the vector-raster TTF/OTF path) charges its glyph
  copies, names, mapping and report to the same ledger. The existing estimate
  check is kept.

## E. Outline primitives

- [ ] E1. Packed grid, edges, outgoing, used/walk, contour points and the
  contour list are admitted before allocation in `square`, `spec` and
  independent-contour tracing, as well as `pixel_origins` and `pixel_contour`.
  Point totals are admitted before each contour is allocated, not checked
  afterwards.
- [ ] E2. Public `contours`/`pixel_origins`/`pixel_contour` keep their
  signatures and results.

## F. Tests

- [ ] F1. Exact-capacity and one-byte-short tests for planning.
- [ ] F2. Exact-capacity and one-byte-short tests for square, square-separate
  and dot outline primitives.
- [ ] F3. Exact-capacity and one-byte-short tests for export (square TTF/OTF,
  dot TTF with each reuse policy, embedded bitmaps, prepared encode,
  encode_single). The minimum output is byte-identical to the default output,
  and one byte less is `ResourceLimit`.
- [ ] F4. An allocator cross-check confirms that the measured live heap during
  plan and export does not exceed the minimum admitted ledger for the fixtures.
- [ ] F5. Existing variants, variants_metadata, outlines, zero-area and
  vector_raster tests pass. The listed CLI sentinel outputs (full Tong Large
  square/dot TTF/OTF plus extra shapes, embedded and convert cases) are
  byte-identical to the 6297998 baseline.
- [ ] F6. `cargo clippy --locked -p fontpixel --all-targets --all-features -- -D warnings`
  and `cargo fmt --all --check` are clean.

## G. Reporting

- [ ] G1. Report the minimum `--max-total-bytes` for the full Tong Large SC
  regional square/dot TTF/OTF default exports against the 256 MiB default.
- [ ] G2. List every `working.rs` addition and every codec merge point.
