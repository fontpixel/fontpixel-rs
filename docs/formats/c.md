# C bitmap arrays (W3.3)

## Evidence, fields and ambiguities (2026-10-04, recorded before code)

Primary sources: supplied MIT `opf/ingest/header8x8.py` and its tests;
https://github.com/dhepper/font8x8 (public-domain data); Adafruit's BSD-licensed
https://github.com/adafruit/Adafruit-GFX-Library/blob/master/gfxfont.h and
`Adafruit_GFX.cpp` drawChar format consumer. Original parser implementation only;
no GPL code or arbitrary C evaluation.

* font8x8: char font8x8_NAME[N][8], eight LSB-left bytes per glyph. A declared
  `unicode points U+START - U+END` range is authoritative over row comments (the
  upstream Greek diaeresis comments are wrong). Without range use row U+ comments;
  unmapped rows cause a loss diagnostic. Quoted local includes use bounded safe
  relative resources, cycles are deduplicated. Conflicting duplicate glyphs fail.
* t-mat: 96 eight-byte MSB-left cells, row comments `decimal (0xHEX)` must agree
  and cover U+0020–U+007F exactly. Comments may contain literal brace characters.
* generic: a flat or nested uint8_t/unsigned-char literal initializer, selected by
  optional array name; explicit width, height, first code, charset and bit order.
  Rows are top-down, byte aligned; width beyond 8 is supported. Outer dimension is
  verified rather than used for huge allocation. Geometry is never guessed from
  unrelated C arrays. Writer emits portable uint8_t font[N][row_bytes*height].
* Adafruit GFX: GFXfont's bitmap pointer, glyph pointer, inclusive first/last and
  yAdvance; GFXglyph = bitmapOffset u16, width/height/xAdvance u8, xOffset/yOffset
  i8. Packed bits are MSB-first CONTINUOUS within each glyph, with next glyph
  byte-aligned. BBX x=xOffset, y=-(yOffset+height). yAdvance is line height, not
  ascent. Empty glyphs retain advance. GFXfont extents are u16, not Unicode beyond
  BMP. Sparse output fills absent slots with empty glyphs and reports additions.

Parser is a bounded lexer for literals, punctuation, identifiers, comments and
quoted strings. It does not compile, execute, expand macros, follow system headers,
or evaluate expressions. Unterminated comments/strings, excessive nesting, missing
rows, out-of-range fields, incomplete initializers and nonliteral bitmap values
fail explicitly. Requested declaration is selected; ambiguous candidates fail.

## Metadata and losses

Header families use the catalogue's 8x8 baseline (ascent 7/descent 1) and exact
copyright conventions. Generic baseline is explicit (default height); unknown
charset means legacy numeric slots, never guessed Unicode. Writes do not silently
reassign arbitrary cmap entries by order. Generic and GFX targets report font names,
properties, nonrepresentable metrics, aliases, sequences, missing encodings,
charset changes, bit-depth reduction and cell normalization. GFX limits are checked
before serialization. Unknown C source comments/macros are not font metadata.
Generic output includes a small JSON option comment so its geometry, mapping origin,
charset and bit order can be read back without guessing; ordinary C compilers ignore
it. This is an extension, not a claim arbitrary C arrays carry such metadata.

Automatic family selection inspects declarations and real comments. A native
`fontpixel-c` option comment selects generic geometry even when the symbol starts
with `font8x8_`; text inside string literals does not select a format or supply
options. A font8x8 Unicode range belongs to the declaration following its comment,
with preceding declarations forming a boundary. Explicit array selection applies
across includes and must resolve to exactly one declaration. C output reports a
stable glyph ID reorder even if the total number of slots is unchanged.

If automatic output-range inference has no encodable glyph, the error retains
all per-glyph omission diagnostics and adds `c.write.no_representable_glyphs`.
No output is returned. An explicit inclusive range may still produce empty cells,
with losses for both omitted source glyphs and added empty positions.
