# FreeBSD VFNT0002 console fonts

W5.B research recorded before implementation, 2026-10-04.

Primary source: [FreeBSD VFNT0002 console fonts authority](https://raw.githubusercontent.com/FreeBSD/freebsd-src/main/sys/sys/font.h). Supplementary executable format evidence is MIT monobit 0.54, module `vtfont`; the installed release fingerprints and exact external-oracle scope are recorded in `reports/w5.b-crosscheck.json`.

32-byte big-endian header: 8-byte magic, byte cell width/height, padding, glyph count and four map counts. Packed fixed-cell glyph table followed by normal-left, normal-right, bold-left and bold-right maps. Each 8-byte map record has Unicode start, u16 glyph index and u16 length-minus-one. Normal/bold fallback and default glyph zero are native semantics. Right halves join with their matching left halves into double-width model glyphs.

Unicode mappings and aliases, normal/bold strikes, default glyph and raw map associations must survive. Writer splits double-width cells, deduplicates rasters and emits sorted ranges. Unsupported widths/style counts and mappings must be explicit errors or itemized loss, never silently dropped.

All reads budget retained input, cumulative fonts/glyphs, table allocations, decompressed pixels and diagnostics before allocation. Every successful write is reopened under identical limits and compared field by field. Source resources and external fonts are never fetched implicitly. Synthetic fixtures are original MIT artwork; third-party tools remain outside the repository.

The [native lookup routine](https://raw.githubusercontent.com/FreeBSD/freebsd-src/main/sys/dev/vt/vt_font.c) resolves bold separately for each half, and a bold mapping to index zero also falls back to the normal map. Each returned index is 16 bits, including a run's computed offset. The reader reflects effective bold mappings, retains the complete original pool/maps in set metadata, and exposes unused physical rasters as unmapped regular glyphs. Unmodified native model data can be rewritten byte-exactly; edits regenerate sorted maps with complete loss auditing. Native kernel dimensions (1..128), pool byte limit (2 MiB), mapping-record count (65536 per map) and glyph-pool count (131072) are independently checked in addition to caller limits.
