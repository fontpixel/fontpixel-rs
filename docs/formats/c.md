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

W5 registry detection also requires declaration evidence: native identifiers in
comments, string literals or preprocessor text do not identify a font. A cheap
marker/initializer-shape filter runs before bounded token and declaration scans,
so unrelated large BDF/HEX text does not allocate C tokens. Native structs and
byte-array declarators supply positive evidence; the generic `fontpixel-c` or
native `fontpixel-u8x8` comment must accompany a real byte array. A real quoted font8x8 include remains
a Possible entry point, but detection never resolves it. Token and declaration
memory share the input context's cumulative budget; no glyph model is built.
The native reader and writer semantics are unchanged.

If automatic output-range inference has no encodable glyph, the error retains
all per-glyph omission diagnostics and adds `c.write.no_representable_glyphs`.
No output is returned. An explicit inclusive range may still produce empty cells,
with losses for both omitted source glyphs and added empty positions.

## U8g2 and LVGL (W5.5, evidence recorded before implementation)

Primary references: U8g2's [font format](https://github.com/olikraus/u8g2/wiki/u8g2fontformat)
and BSD-2-Clause `csrc/u8g2_font.c`; LVGL v8.3.11 MIT
`src/font/lv_font_fmt_txt.{c,h}` and
[lv_font_conv specification](https://github.com/lvgl/lv_font_conv/blob/master/doc/font_spec.md).
They are read as format/consumer references; new parsers and encoders are original.

U8g2 arrays accept concatenated C byte strings or literal bytes. Octal and hex
escapes are decoded without compiling source. The 23-byte header specifies LSB
bitfield widths, biased signed metrics, two run lengths and repeated run pairs.
ASCII and BMP Unicode records have checked jumps; the modern Unicode block jump
table is validated. Writes emit genuine U8g2 compressed arrays including Unicode
blocks. Native record lengths are u8, Unicode is BMP and pitches/offsets signed
8-bit; unrepresentable values are errors, never wrapped.

LVGL reads designated native structures, literal arrays, all four cmap types,
1/2/3/4/8 bpp packed raster and RLE with optional preceding-row XOR filtering.
Pixel coverage is normalized to Gray8 except 1 bpp. 3 bpp follows the native
consumer's 3-to-4 bpp coverage expansion. Glyph zero is reserved. Advances in
8.4 fixed point are projected with the native rounding and reported when fractional.
Pair/class kerning cannot be expressed by the core model and produces an explicit
Loss diagnostic; the parser does not evaluate callbacks. Writes support
1/2/4/8 bpp with typed Plain/Rle/RleXor options and native sparse/contiguous cmaps.
Outputs include `lvgl.h` and use the LVGL v8 public font ABI. Native v9 differences
that change semantics require separate explicit support, not guessed fields.

Only literal data and recognized storage/version preprocessor scaffolding are
accepted. Generated LVGL guards use a bounded conditional selector with the
v8.3.11 version tuple, `LV_VERSION_CHECK`, comparisons/boolean operators and
literal 0/1 `LV_FONT_*` feature defines; unknown symbols or directive nesting
beyond 32 fail. Headers are never loaded by this selector. No include execution, macro expansion, arbitrary expressions or dynamic
C execution occurs. Shared C traversal remains bounded and preserves the earlier
families. Both new writers read their native output under the same resource limits
and compare the projected model to issue metadata/metric/mapping/pixel losses.

### Survey additions (recorded before their implementation)

NetBSD `wsdisplay_font`: primary `sys/dev/wscons/wsconsio.h` and
`sys/dev/wsfont/spleen5x8.h` in NetBSD's repository. Native name, firstchar,
numchars, encoding, width, height, stride, bitorder and byteorder are interpreted.
The small constant grammar additionally accepts single-byte character literals
and checked addition/subtraction for ordinary declarations such as `128 - ' '`;
this is arithmetic on literal data only. Platform-dependent KNOWN bit/byte order
is rejected. ISO positions are Unicode; other native encodings remain explicitly
named legacy codes. Writer emits MSB-left, left-to-right rows with explicit native
metadata and a fixed cell, reporting cell/baseline normalization.

U8glib: BSD-licensed `csrc/u8g_font.c`, 17-byte font header, legacy records 0/1/2.
Format 1 is compact nibble metrics (its y field has bias two); record widths of
255 indicate missing slots. Format 0/2 uses six metric bytes and byte-aligned
rows; format 2 is native two-bit grayscale, mapped to 0/85/170/255. U8x8: `csrc/u8x8_8x8.c` defines the modern four-byte header (first, last,
tile-width, tile-height), followed by row-major 8x8 tiles with vertical LSB-first
columns. The pre-2019 two-byte U8x8 format is an explicit reader option, never
heuristically confused with glyph pixels. These legacy 8-bit arrays do not name
an encoding; caller-selected charset defaults to Unicode numeric positions for
consistency with upstream codepoint-oriented drawing APIs. Writer options expose
that choice and report mappings it cannot retain.

LVGL binary: `lv_font_conv/doc/font_spec.md` and LVGL 8.3
`lv_font_loader.c`. Little-endian sized `head`, `cmap`, `loca`, `glyf` and optional
`kern` tables, with MSB-first glyph bitstreams, are a separate typed container.
No sfnt assumptions are applied. Binary offsets, section boundaries, signed
metric bits, compression and cmap semantics are validated against the consumer.

### Validated native range and projection details

U8g2's consumer uses a signed 8-bit local glyph height: this codec rejects heights
above 127 and nonzero-width/zero-height records, avoiding unsafe native draw
loops. Native Unicode accelerators must preserve lookup of every glyph. Legacy
U8glib compact metrics follow actual positive x/advance nibbles and y bias two,
not an assumed two's-complement nibble. U8x8's tile product must fit its native
u8 counter. LVGL source output fits the normal (non-large) fmt_txt descriptor
fields; its reader also accepts the large descriptor dimensions. Non-null custom
callbacks, fallback fonts, subpixel rendering and ambiguous macros fail explicitly.
Kerning pairs/classes and fractional advances are validated before their losses
are reported. Unknown encodings and unrepresentable native field values are errors.

LVGL binary glyph zero is a reserved undefined slot, as in the official loader.
Binary conversion shares native bitmap and cmap semantics with the source reader;
its intermediate literal representation is internal, bounded cumulatively, and
never compiled or executed. Intermediate representations count against aggregate
limits, so successful writes guarantee same-limit reads but do not promise to
succeed at the theoretical minimum file-size budget.

Original fixture and consumer checks are in `tests/crosscheck/w55.py`; runtime
sources are fetched at pinned upstream commits or the LVGL v8.3.11 tag and remain
outside the repository. The test harness compiles project-authored output and fixed upstream test fonts,
never arbitrary input fonts.
