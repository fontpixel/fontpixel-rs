# Unit B acceptance checklist — subset internals on one shared ledger

Derived from `WORK/owners/w65/subset-budget-design.md` (inventory table, implementation
plan, required regression matrix), `subset-batch1-status.md`, `WORK/owners/w64/NEXT-LEDGER-DESIGN.md`
(planner/export/subset bullets) and the "Subset internals" item under "Known limitations
accepted for phase 6" in `docs/w6.1-writer-budget.md`.

Status legend: `[ ]` open, `[x]` done (evidence in final report), `[-]` not done (reason given).

## A. One ledger, caller allowance

- [ ] A1 `sfnt::subset::subset` builds exactly one root `Budget` from the caller's remaining
      allowance (`limits.sfnt.resources.max_total_bytes`) and delegates to a crate-private
      `subset_with_budget(data, options, &Budget)`; every nested codec call made by subset
      receives that same `&Budget`. No subset path calls a public codec wrapper that builds a
      fresh full-limit `Budget` (container, cmap, GSUB, glyf, CFF, metrics, OS/2, post,
      bitmap, EBSC, builder).
- [ ] A2 Borrowed caller storage (input sfnt bytes, codepoint/drop selections) is not charged
      again.
- [ ] A3 Every lease is admitted before its allocation and is declared before (so dropped
      after) the storage it covers, including on error paths.
- [ ] A4 The returned `glyph_mapping`, output bytes and report stay charged until `subset`
      returns; afterwards the caller owns them (CLI already charges output/report).
- [ ] A5 Resource failures remain `Error::ResourceLimit`; no limit/cap default is raised.

## B. Stage coverage (inventory table)

- [ ] B1 Sfnt parse directory/tag/span (already covered; keep).
- [ ] B2 cmap selected decode + full-source validation/work cap; encode (already covered; keep).
- [ ] B3 GSUB decode/closure/subset/encode (already covered; keep) + DFLT repair report entry.
- [ ] B4 keep set + composite dependency queue + old/new mapping tree.
- [ ] B5 glyf: full decode-validation of every glyph (flags, coordinates, instructions,
      components, graph cycles/depth, cumulative caps) with all per-glyph temporaries
      (endpoints, instruction copy, flags, Fontations point/flag arrays, components) charged;
      subset retains only composite dependency metadata, not simple geometry; graph
      states/depth arrays charged.
- [ ] B6 raw glyf/loca subset written into exact pre-sized ledger buffers (no record copy),
      hints/instructions/bboxes preserved byte-for-byte.
- [ ] B7 CFF parse retained vectors (selectors, identifiers, locals, widths, subfonts) and parse
      temporaries (DICT entries/operands/duplicate check, charset, encoding, FDSelect) charged.
- [ ] B8 CFF subset: no per-item INDEX copies, no length-only template buffers; exact output
      size computed with checked arithmetic and admitted before allocation; raw charstrings,
      subroutines, private DICT data, FD dicts and unused CID Top DICT Private relocation
      preserved; explicit nondefault encoding still rejected; selected-program validation kept.
- [ ] B9 metrics: full decoded metric array, compact selected array, encoded header/metric
      buffers charged (h and v).
- [ ] B10 OS/2: coverage computed without a temporary codepoint set copy (or charged); encoded
      buffer charged.
- [ ] B11 post: full-name validation of every glyph name kept; selected names and encoder
      temporaries (indices, custom-name map, borrowed list) and output charged.
- [ ] B12 embedded bitmap (EBDT/EBLC, bdat/bloc): per-strike records/decoded/active arrays,
      every decoded pixel buffer (incl. Gray8 canvas + Mono overlap), retained strike trees,
      encoder per-glyph temps, group offset arrays and output charged; existing local
      work/pixel/visit/model limits unchanged; full validation of unselected images kept.
- [ ] B13 EBSC/VORG/LTSH/hdmx parsed/filtered/output storage charged with exact sizes.
- [ ] B14 Final assembly: copied global tables, replacement tables and builder tree nodes stay
      charged while the final sfnt output is admitted and written.
- [ ] B15 Report entries (DSIG loss, explicit drop loss, DFLT repair) charged: vector capacity
      and code/message strings.

## C. Behaviour preservation

- [ ] C1 Default outputs byte-identical (subset bytes, mapping, report) for TTF, CFF OTF and
      bitmap-only sources; verified against a pre-change baseline on Tong TTF/OTF/OTB exports
      and repository fixtures.
- [ ] C2 All unsupported-table rejections, explicit drop loss, DSIG loss, UVS/legacy cmap
      rejection, vhea/vmtx pair guard and required/glyph table drop guards unchanged.
- [ ] C3 No parser bypass: unselected glyf/CFF/bitmap/cmap/post data still fully validated.
- [ ] C4 Public standalone codec APIs keep their signatures and behaviour.

## D. Required regression matrix (tests)

- [ ] D1 Compressed broad cmap with tiny selection validates whole source; bad unselected gid
      errors (existing, keep).
- [ ] D2 Exact-capacity and one-byte-short root allowance through the public API, and a second
      already-live lease on the shared Budget reducing capacity (crate-level test).
- [ ] D3 GSUB alias/closure/precedence/DFLT repair/remap under constrained memory.
- [ ] D4 Composite dependency closure, cycle/depth errors, full flag/coordinate validation of
      unselected glyphs, hint/raw-byte preservation; large simple glyph temporaries counted.
- [ ] D5 Custom post names and embedded strikes; vertical metrics; EBSC/VORG/LTSH/hdmx remaps.
- [ ] D6 Large unchanged global table + replacements + final output simultaneously counted,
      without charging borrowed input twice.
- [ ] D7 CFF raw program/FD/private/subroutine identity, alias private blocks, no placeholder
      retained, exact output growth bound.
- [ ] D8 Rejections/loss/guards (C2) still covered.
- [ ] D9 Focused tests: subset, sfnt module tests, variants subset tests, CFF tests; clippy
      all-features `-D warnings`; `cargo fmt --all --check`.

## E. Reporting

- [ ] E1 Peak working budget for subsetting a full Tong TTF (and OTF/OTB) to ASCII, vs the
      256 MiB default (255 MiB CLI working allowance).
- [ ] E2 Update the "Subset internals" known-limitation text in `docs/w6.1-writer-budget.md`.
- [ ] E3 List every `working.rs` addition and every edit to EBLC/OS-2 writers (unit D overlap).
