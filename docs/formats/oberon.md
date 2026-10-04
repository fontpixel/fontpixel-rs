# Project Oberon bitmap fonts

W5.B research recorded before implementation, 2026-10-04.

Primary source: [Project Oberon bitmap fonts authority](https://people.inf.ethz.ch/wirth/ProjectOberon1992.pdf). Supplementary executable format evidence is MIT monobit 0.54, module `oberon`; the installed release fingerprints and exact external-oracle scope are recorded in `reports/w5.b-crosscheck.json`.

Identifier 0xdb and abstraction byte, family/variant bytes, 16-bit line height and signed bounding extrema, then count of 4-byte half-open character runs. All 10-byte glyph boxes precede bitmap data: signed advance, x, y, width and height. Bitmap rows are bottom-to-top with LSB-first pixels. Screen and printer names share this bitmap wire layout. Nonzero abstraction denotes a different non-bitmap format. The [original Fonts module](https://people.inf.ethz.ch/wirth/ProjectOberon/Sources/Fonts.Mod.txt) independently confirms separate runs, boxes, and rasters; it is used as format evidence, without copying implementation code.

Legacy 8-bit character domain with 16-bit exclusive run ends (including 256), signed-word metrics, one strike. The 1992 implementation has 16 run slots and 256 characters; the 2019 RISC implementation has 128 display slots and reads the low byte of the same word fields. The codec targets the documented word-based bitmap file, not that later renderer’s low-byte truncation. Preserve family/variant and header extrema; unencoded glyphs, Unicode domain conversion, names, extra metrics and source metadata changes must be audited.

All reads budget retained input, cumulative fonts/glyphs, table allocations, decompressed pixels and diagnostics before allocation. Every successful write is reopened under identical limits and compared field by field. Source resources and external fonts are never fetched implicitly. Synthetic fixtures are original MIT artwork; third-party tools remain outside the repository.
