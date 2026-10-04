# FIGlet FIGfont v2

Primary specification consulted before implementation: [The FIGfont Version 2 FIGfont and FIGdriver Standard](https://github.com/cmatsuoka/figlet/blob/master/figfont.txt), Cowan/Burton/Chappell/Chai (1991–1997, freely distributable). The official FIGlet executable is an external oracle only; no implementation code is copied.

| Wire data | Model |
|---|---|
| flf2a plus hardblank, height, baseline | bitmap cell height, ascent=baseline, descent=height-baseline |
| maximum line length, old/full layout, direction, comments, code count | retained `figlet.*` properties plus exact full source bytes |
| ASCII 32–126 then Ä Ö Ü ä ö ü ß | Unicode mappings in required order |
| signed code tags (decimal, octal, hexadecimal) | nonnegative Unicode-tagged source numbers; negative values retain a Legacy two's-complement code and signed `figlet.code` property |
| each row's trailing run of its endmark | removed before measuring equal-width rows |
| all remaining non-space bytes | ink, including the source hardblank, per PLAN |
| source text including CRLF, comments, art, tags | exact `figlet.source` Bytes provenance; never blindly reused after edits |

FIGfont is byte-oriented; each byte denotes one sub-character. This preserves old 8-bit art without guessing UTF-8; UTF-8 multibyte art occupies multiple bitmap columns. The hardblank would render as a blank in FIGlet but is non-space in the requested bitmap projection: its conversion is an explicit read Loss. Character-shaped art and smushing semantics are not implemented by the bitmap renderer; source bytes retain them for consumers and loss diagnostics identify the projection. Negative code -1 is forbidden. Duplicate code tags remain separate source glyphs. The 102 required slots must exist, although their width can be zero. Header and glyph rows are bounded before allocation; no split of the entire input into lines is required.

The writer emits `#` for ink, ASCII space for blank, `$` hardblank marker, `@` endmarks, full-width/full-height layout, and exactly 102 required glyphs followed by ordered code tags. It aligns glyphs into a common baseline cell and preserves horizontal advance with blank columns where possible. Negative left bearings or advance smaller than the ink's right edge fail instead of clipping. Missing required slots become zero-width blanks, and all resulting geometry/identity/metadata changes are explicit Loss entries. Source comments and signed tags are retained; arbitrary original art, hardblank barriers, smushing, direction and source spelling changes are audited and cannot silently disappear. Gray8 requires an explicit threshold. Canonical output is reread under identical resource limits before success.

Independent verification uses monobit 0.54 and official FIGlet 2.2.5/chkfont (BSD-3-Clause, external tools only). Monobit rejects traditional octal code tags with ValueError, and negative code-tag records with a Props-assignment TypeError; the monobit fixture omits that record. Native tests cover negative/octal parsing, and official FIGlet maps input Z to code -2 through an external control file and renders both source and native-written records identically. Monobit also collapses zero-area rectangles; comparisons normalize only those rectangles and compare advances and all ink coordinates exactly.
