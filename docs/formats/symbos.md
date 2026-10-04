# SymbOS FNT

W5.B research recorded before implementation, 2026-10-04.

Primary source: [SymbOS FNT authority](https://www.symbos.org/download.htm). Supplementary executable format evidence is MIT monobit 0.54, module `symbos`; the installed release fingerprints and exact external-oracle scope are recorded in `reports/w5.b-crosscheck.json`.

The two-byte header encodes height, small/medium/big profile, ordinary versus 255-character range, and either a starting code or extended metadata. Medium records are 16 bytes; small records are 9 bytes; big records use a relative word-offset table. Legacy codes and unused rows are retained.

No baseline/bearing/name fields. Writer normalizes to a cell and reports resulting geometry changes; oversized ink is rejected rather than clipped. Unicode-as-legacy and grayscale threshold are explicit choices. Canonical default range is 32..127; other byte ranges are explicit options.

All reads budget retained input, cumulative fonts/glyphs, table allocations, decompressed pixels and diagnostics before allocation. Every successful write is reopened under identical limits and compared field by field. Source resources and external fonts are never fetched implicitly. Synthetic fixtures are original MIT artwork; third-party tools remain outside the repository.

The [original developer's current structure specification](https://github.com/Prodatron/symdoc-developer/blob/13f125a73925a76b9016de5ebe874b591b8aec2b/SymbOS-DesktopDataRecords.txt) establishes all three profiles. Medium uses 16-byte records (up to 15 rows), small uses 9-byte records (up to 8 rows), and big has up to 48 rows with a relative word-offset table. For each big glyph, a total-width byte precedes column width/data pairs and a zero-width terminator; each non-final column is eight pixels. The target address is offset-word address plus one plus the unsigned word. Header bit 6 selects big, otherwise bit 5 selects small; bit 7 selects codes 1..255. In 255-character files the second header byte is writing-system/WYSIWYG metadata, not the first code. Ordinary ranges end at code 127. The implementation uses these format facts, not GPL assembly code. The monobit comparison is restricted to medium; original independent wire fixtures exercise the additional profiles.

Without a native magic signature, detection returns only `Possible` after an allocation-free full structural walk. It validates every fixed record width/count or every big-font relative pointer, column, raster extent and terminator, and requires the final referenced record to end at EOF. Explicit short-range policy is respected for medium/small. It neither consults file extensions nor claims certainty; genuinely complete dual-format inputs can remain ambiguous.
