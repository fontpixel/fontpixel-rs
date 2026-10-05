# Unit B acceptance checklist — subset internals on one shared ledger

Derived from `WORK/owners/w65/subset-budget-design.md` (inventory table, implementation
plan, required regression matrix), `subset-batch1-status.md`, `WORK/owners/w64/NEXT-LEDGER-DESIGN.md`
(planner/export/subset bullets) and the "Subset internals" item under "Known limitations
accepted for phase 6" in `docs/w6.1-writer-budget.md`.

Status legend: `[ ]` open, `[x]` done (evidence in final report), `[-]` not done (reason given).

## A. One ledger, caller allowance

- [x] A1 `sfnt::subset::subset` builds exactly one root `Budget` from the caller's remaining
      allowance (`limits.sfnt.resources.max_total_bytes`) and delegates to a crate-private
      `subset_with_budget(data, options, &Budget)`; every nested codec call made by subset
      receives that same `&Budget`. No subset path calls a public codec wrapper that builds a
      fresh full-limit `Budget` (container, cmap, GSUB, glyf, CFF, metrics, OS/2, post,
      bitmap, EBSC, builder).
- [x] A2 Borrowed caller storage (input sfnt bytes, codepoint/drop selections) is not charged
      again.
- [x] A3 Every lease is admitted before its allocation and is declared before (so dropped
      after) the storage it covers, including on error paths.
- [x] A4 The returned `glyph_mapping`, output bytes and report stay charged until `subset`
      returns; afterwards the caller owns them (CLI already charges output/report).
- [x] A5 Resource failures remain `Error::ResourceLimit`; no limit/cap default is raised.

## B. Stage coverage (inventory table)

- [x] B1 Sfnt parse directory/tag/span (already covered; keep).
- [x] B2 cmap selected decode + full-source validation/work cap; encode (already covered; keep).
- [x] B3 GSUB decode/closure/subset/encode (already covered; keep) + DFLT repair report entry.
- [x] B4 keep set + composite dependency queue + old/new mapping tree.
- [x] B5 glyf: full decode-validation of every glyph (flags, coordinates, instructions,
      components, graph cycles/depth, cumulative caps) with all per-glyph temporaries
      (endpoints, instruction copy, flags, Fontations point/flag arrays, components) charged;
      subset retains only composite dependency metadata, not simple geometry; graph
      states/depth arrays charged.
- [x] B6 raw glyf/loca subset written into exact pre-sized ledger buffers (no record copy),
      hints/instructions/bboxes preserved byte-for-byte.
- [x] B7 CFF parse retained vectors (selectors, identifiers, locals, widths, subfonts) and parse
      temporaries (DICT entries/operands/duplicate check, charset, encoding, FDSelect) charged.
- [x] B8 CFF subset: no per-item INDEX copies, no length-only template buffers; exact output
      size computed with checked arithmetic and admitted before allocation; raw charstrings,
      subroutines, private DICT data, FD dicts and unused CID Top DICT Private relocation
      preserved; explicit nondefault encoding still rejected; selected-program validation kept.
- [x] B9 metrics: full decoded metric array, compact selected array, encoded header/metric
      buffers charged (h and v).
- [x] B10 OS/2: coverage computed without a temporary codepoint set copy (or charged); encoded
      buffer charged.
- [x] B11 post: full-name validation of every glyph name kept; selected names and encoder
      temporaries (indices, custom-name map, borrowed list) and output charged.
- [x] B12 embedded bitmap (EBDT/EBLC, bdat/bloc): per-strike records/decoded/active arrays,
      every decoded pixel buffer (incl. Gray8 canvas + Mono overlap), retained strike trees,
      encoder per-glyph temps, group offset arrays and output charged; existing local
      work/pixel/visit/model limits unchanged; full validation of unselected images kept.
- [x] B13 EBSC/VORG/LTSH/hdmx parsed/filtered/output storage charged with exact sizes.
- [x] B14 Final assembly: copied global tables, replacement tables and builder tree nodes stay
      charged while the final sfnt output is admitted and written.
- [x] B15 Report entries (DSIG loss, explicit drop loss, DFLT repair) charged: vector capacity
      and code/message strings.

## C. Behaviour preservation

- [x] C1 Default outputs byte-identical (subset bytes, mapping, report) for TTF, CFF OTF and
      bitmap-only sources; verified against a pre-change baseline on Tong TTF/OTF/OTB exports
      and repository fixtures.
- [x] C2 All unsupported-table rejections, explicit drop loss, DSIG loss, UVS/legacy cmap
      rejection, vhea/vmtx pair guard and required/glyph table drop guards unchanged.
- [x] C3 No parser bypass: unselected glyf/CFF/bitmap/cmap/post data still fully validated.
- [x] C4 Public standalone codec APIs keep their signatures and behaviour.

## D. Required regression matrix (tests)

- [x] D1 Compressed broad cmap with tiny selection validates whole source; bad unselected gid
      errors (existing, keep).
- [x] D2 Exact-capacity and one-byte-short root allowance through the public API, and a second
      already-live lease on the shared Budget reducing capacity (crate-level test).
- [x] D3 GSUB alias/closure/precedence/DFLT repair/remap under constrained memory.
      (Alias expansion and duplicate-subtable precedence: existing `sfnt::gsub` ledger
      module tests; closure, pruning, DFLT repair and remap at the exact public boundary:
      new `gsub_closure_and_dflt_repair_survive_the_exact_allowance`.)
- [x] D4 Composite dependency closure, cycle/depth errors, full flag/coordinate validation of
      unselected glyphs, hint/raw-byte preservation; large simple glyph temporaries counted.
      (Cycle and reserved-flag errors are tested; depth and coordinate overflow keep the
      same shared validator body and existing `tests/glyf.rs` coverage.)
- [x] D5 Custom post names and embedded strikes; vertical metrics; EBSC/VORG/LTSH/hdmx remaps.
- [x] D6 Large unchanged global table + replacements + final output simultaneously counted,
      without charging borrowed input twice.
- [-] D7 CFF raw program/FD/private/subroutine identity, no placeholder retained, exact
      output growth bound: done (new CFF test; existing variants CID test; byte identity of
      30 raw-CFF subsets vs. the pre-change implementation). Not done: a dedicated fixture
      whose FDs alias one Private block. No such fixture exists in the repository; the
      writer handles each FD's Private independently, exactly as before (byte-identical).
- [x] D8 Rejections/loss/guards (C2) still covered.
- [x] D9 Focused tests: subset, sfnt module tests, variants subset tests, CFF tests; clippy
      all-features `-D warnings`; `cargo fmt --all --check`.

## E. Reporting

- [x] E1 Peak working budget for subsetting a full Tong TTF (and OTF/OTB) to ASCII, vs the
      256 MiB default (255 MiB CLI working allowance).
- [x] E2 Update the "Subset internals" known-limitation text in `docs/w6.1-writer-budget.md`.
- [x] E3 List every `working.rs` addition and every edit to EBLC/OS-2 writers (unit D overlap).

## Evidence (2026-10-05)

- Byte identity: a scratch harness (not committed) subset 6 Tong exports (TTF square/dot,
  OTF square/dot, OTB SC/JP) and every `tests/data/embedded-bitmap` and `tests/data/compat`
  TTF to `A`, ASCII, ASCII+U+4E00..51FF, all mapped scalars and empty (unsupported tables
  dropped explicitly when needed, so drop-loss reports are exercised): 255 cases (510 files:
  output bytes plus mapping/report dumps), all SHA-256 identical to the pre-change build. 30 raw `Cff::subset`
  outputs over `tests/data/cff/*.cff` (5 glyph orders each) are identical too.
- Coverage cross-check: the same harness with a counting global allocator, 240 cases: the
  real heap peak of each subset call never exceeds the minimal ledger allowance.
- Every allowance below the minimum fails with `Error::ResourceLimit` (binary searches in
  the harness and in the new tests); one byte short is the subset ledger's own error.
- Peak allowance (minimal `max_total_bytes`) vs the 255 MiB CLI working allowance:
  Tong TTF Large-square to ASCII 5.89 MiB (all 52.9), Mono Small-dot TTF 8.07 (51.9),
  OTF Large-square 6.00 (50.7), OTF Mono Small-dot 5.92 (57.1), OTB SC 28.6 (85.9),
  OTB Mono JP 28.5 (85.9).
