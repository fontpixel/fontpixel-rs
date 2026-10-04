# Bits'N'Picas 1.x (`kbits`)

Format research before implementation, 2026-10-04. Primary source: the
[Bits'N'Picas project](https://github.com/kreativekorp/bitsnpicas), specifically
`KbitsBitmapFontImporter.java` and `KbitsBitmapFontExporter.java`. These MPL
sources were consulted only to derive the following wire specification; no
implementation code is copied. Reader SHA-256:
`908c6b14db259458aaff62e2392fadfe3ed4ca34e6cee5aa31eab9d18d1cbf0d`;
writer SHA-256:
`c9131135ca12b73d279ca7a14f29ffd2ded1d0b5b37e0d9b60a5451f065c872e`.

All integers are signed big-endian 32-bit values. Header: `KBnPbits`, version
1, em ascent/descent, line ascent/descent, line gap, x-height. Records have
four-byte tags: `name` has version 1, name ID, Java modified UTF-8 string
(unsigned 16-bit byte count); `char` has version 1, Unicode number, advance,
x bearing, ascent, row count, then each row's column count and Gray8 samples;
`fin.` ends the stream. Glyph y origin is ascent minus row count. Ragged rows
are right-padded with zero and reported as normalization loss. Unknown record
types cannot be skipped safely and are fatal. UTF-16 surrogate pairs in
modified UTF-8 are decoded; malformed sequences are rejected.

Native header metrics and numeric name records use ordered `kbits.*`
properties. The writer emits rectangular rows and preserves Gray8 exactly;
monochrome promotion is reported. Extra codes, names, metadata and metrics
without native representation are diagnosed. Duplicate Unicode records are
retained by the Rust model but consumers which index by code may coalesce them.
Negative source character numbers are retained as native glyph properties.
Unencoded glyphs without such provenance and legacy-coded glyphs fail on write;
the caller must supply an explicit Unicode mapping. Native names include Java's
encoded NUL and supplementary Unicode surrogate pairs.

Source metadata depth is bounded before walking or comparing caller-authored
property lists. Readback auditing includes every model field, glyph slot,
alias and Unicode sequence; the fuzz target independently checks same-limit
readability and full equality for every lossless successful write. Bits’N’Picas
2.2.2 and the frozen MIT catalogue provide independent raster/metric crosschecks.
