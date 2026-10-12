# bdfparser 3.x → 4.x: behaviour changes

bdfparser 4.x (PyPI `bdfparser`, npm `bdfparser`) is a thin package over the
fontpixel compatibility layer: Python `fontpixel.compat.bdfparser` and
JavaScript `fontpixel/compat/bdfparser` (plus `/cjk` and `/cli`). The API is
bdfparser 3.0's (SPEC-v3); font data now lives in a Rust model instead of
Python dicts or JavaScript Maps. This page lists every known behaviour change,
so it can be pasted into the bdfparser 4.0.0 changelogs.

Evidence: the unmodified bdfparser 3.x test suites run against the layer.

| suite | passed | failed | skipped | not loaded |
|---|---:|---:|---:|---:|
| bdfparser (Python) 3.x, default | 1,755 | 8 | 10 | 0 |
| bdfparser (Python), slow tests only (`BDFPARSER_SLOW=1`) | 10 | 0 | 0 | 0 |
| bdfparser-js 3.x, default (Node 24, vitest 5) | 1,178 | 11 | 3 | 2 |
| bdfparser-js, `--slow` | 1,181 | 11 | 0 | 2 |

Every failure, every suite that does not load and every other documented
difference is below. The exact lists that CI enforces are
`crates/fontpixel-py/tests/compat_bdfparser/expected_failures.txt` and
`crates/fontpixel-wasm/tests/compat_bdfparser/expected_failures.txt`; the
detailed evidence is in the two `KNOWN_DIFFERENCES.md` files next to them.

## Both languages

### Bitmap pixel values are only `0`, `1` and `2`

**What changed.** A `Bitmap` holds pixel values `'0'`, `'1'` and `'2'`
(SPEC-v3 §1.3: background, ink, glow). 3.x stored any character; 4.x rejects
other values when the bitmap is built or edited: `Bitmap(['13'])`,
`bitmap.replace('2', '3')`, and `bytepad()` on such a bitmap fail.

- Python: `ValueError` (`InvalidInputValueError`).
- JavaScript: `Error: Invalid character "3" in bitmap row N (only '0', '1' and
  '2' are pixels)` or `Bitmap rows must be rectangular labels 0, 1, 2`.

**Why.** Bitmaps are native label rasters shared with the rest of fontpixel;
an out-of-spec label cannot be stored, and a refused edit leaves the bitmap
unchanged.

**Instead.** Keep extra per-pixel labels in your own structure (for example a
list of strings, or several bitmaps), and convert to `0`/`1`/`2` before
building a `Bitmap`.

Reference tests: Python `test_bitmap.py::TestBitmapAlter::test_replace`,
`TestBitmapPad::test_bytepad`, `TestBitmapPad::test_bytepad4`; JS
`bitmap.test.ts` "Bitmap alter replace", "Bitmap pad bytepad", "bytepad4",
`svg.test.ts` "Bitmap.draw2canvas v2 / v3-rc signature ... still works" (it
builds `new Bitmap(['13'])`; the old `draw2canvas` signature itself works).

### Header pairs are one value

`dwx0`/`dwy0`, `swx0`/`swy0`, `dwx1`/`dwy1` and `swx1`/`swy1` are each one BDF
keyword in the model. After setting only one component, the other reads as
`None` (Python) / `null` (JavaScript; 3.x: `undefined`), and
`headers = {'dwx0': None, 'dwy0': 4}` draws with spacing 4 where 3.x used no
global spacing. Set both components when you mean a pair. (No failing test.)

### Glyph meta lists always have 17 slots

`font.glyphs[cp] = [...]` (JS `font.glyphs.set(cp, [...])`) with fewer than 17
values is stored padded to 17. `write_pcf`/`writePcf` then refuses it with
3.x's `glyph meta must be a 17-slot list`; other writers see the padded list
(3.x failed with an index error). Always assign complete 17-slot lists. (No
failing test.)

### Drawing is bounded by memory

`draw()`, `drawall()` and `drawcps()` admit a result image by its memory size
(`ResourceLimits.max_total_bytes`, 256 MiB by default) instead of having no
bound; the default `drawall()` of a 38,000-glyph CJK font (22.3 Mpx) works. A
result larger than the budget raises a resource-limit error instead of
allocating. (No failing test.)

## Python only

### Private internals are gone

These tests use bdfparser's private implementation, which has no counterpart
when the data lives in Rust:

- `font.unencoded[i]['meta'] is glyph._store` (object identity with the
  private storage list): `test_edit.py::TestAttached::test_negative_codepoint_is_normalised`
  (every assertion before it passes, including the codepoint normalisation).
- monkeypatching `Font._is_gl` to count charset detections:
  `test_encoding.py::test_tottf_resolves_charset_once`,
  `test_hardening.py::TestFontCodecCache::test_cached` and
  `::test_invalidated_by_props_and_override`. Charset resolution is still done
  once per font state (cached in Rust, cleared by `setprop`, `delprop`,
  `encoding`, `addglyph`, `removeglyph` and moves).
- `from bdfparser.pcf import _swap_units`:
  `test_pcf.py::test_reader_swaps_partial_scan_units`. PCF decoding is the
  native codec.

**Instead.** Use only the public API; there is no replacement for private
helpers.

### Message details

- The `linelimit` error prints `None` for a negative requested codepoint.
- A lone surrogate returned by a `cp2char` converter prints as
  `Glyph (codepoint N)` in the `glyphbycp` miss warning.
- `write_ttf`'s "value out of range for a TrueType field" message groups the
  values as 3.x's pack calls do; only a value 3.x itself cannot reach could
  print differently.

## JavaScript only

### Bitmaps are rectangular

**What changed.** `new Bitmap(['1111', '1', '1'])` (rows of different
lengths) throws `Bitmap rows have different lengths`. A glyph whose hex rows
have different widths (`F` then `FF` for `BBX 12 1`) still draws with
`draw(1)` (padded, warned once), but `draw(2)` (rows as stored) throws instead
of returning the ragged `['1111', '11111111']`.

**Why.** A raster is rectangular; ragged rows are outside the SPEC-v3 input
domain.

**Instead.** Pad rows to one width before building a bitmap; use `draw(1)` for
glyphs with irregular hex rows.

Reference tests: `hardening.test.ts` "§11.4 parsing and drawing overlay keeps
its size, aligns at the top-left, clips (K04)" (the overlay warnings pass;
the ragged operand throws), `parse.test.ts` "glyph section fewer hex rows than
BBX height: padded when drawing, warned once".

### The CJK charset tables are always present

**What changed.** 4.x always contains the exact CJK tables (GB2312, GBK,
GB18030, Big5, Big5-HKSCS, Shift_JIS, EUC-JP, EUC-KR, Johab, ...), generated
from CPython's codecs. `getCodec(...).exact` is always `true`, there is no
"tables are not loaded" warning, no `TextDecoder` / `iconv-lite` fallback,
and `cp2char(0x8862, 'big5hkscs')` gives the table result. Importing
`bdfparser/cjk` is accepted and does nothing beyond registering a marker.

**Why.** The tables are compiled into the WebAssembly module; exact
conversions are what SPEC-v3 §11.1 requires.

**Instead.** Nothing to do; drop any iconv-lite workaround.

Reference tests: `encoding.test.ts` "v3 charsets multi-byte charsets without
bdfparser/cjk: restricted TextDecoder, one warning" and "... iconv-lite instead
of TextDecoder".

### gzip-compressed PCF no longer needs `DecompressionStream`

`readPcfAsync`, `load_bytes_async` and `$Font` decode gzip in WebAssembly and
succeed in runtimes without `DecompressionStream`, where 3.x rejected.
Reference test: `pcf.test.ts` "more edge cases readPcfAsync without
DecompressionStream".

### Boolean property values are integers

`font.props.flag = true` is stored as Python stores a `bool` (an integer):
`tobdf` writes `FLAG 1` and `writePcf` writes the property as 1. 3.x wrote
`FLAG "true"` in BDF and dropped it from PCF. Use a number or a string
explicitly. Reference test: `pcf.test.ts` "more edge cases writer: unusual
model values" (all its other assertions pass).

### PCF reader error texts

Malformed PCF files are rejected by the native reader. Three messages differ
from 3.x: an overflowing table offset reports `PCF table range overflow`
(3.x: "... truncated"), a bad METRICS format word reports
`unsupported PCF table format 0x20e` (3.x: "Unsupported PCF metrics format"),
and an overlong property count reports `PCF count overflow` (3.x:
"... truncated"). Every error still mentions PCF and none is a `RangeError`.
Match on the error type, not the text. Reference test: `pcf.test.ts` "readPcf
malformed input gives clear errors".

### Arrays read from views are copies

`font.headers`, `font.props`, `font.glyphs`, `font.unencoded` and
`font._propnames` are live views of the model, but array values read from them
are copies: `font.headers.comment.push('x')` or
`font.unencoded[0].meta[16].push('00')` does not change the font. Assign the
whole value instead (`font.headers.comment = [...font.headers.comment, 'x']`).
`Object.getOwnPropertyDescriptor(font.headers, 'comment')` reports an accessor
instead of a data property. (No failing test.)

### Large integers

An integer property set beyond 2^53 − 1 (a `bigint`) is kept exact and reads
back as a `bigint`. Parsed unquoted values outside −2^31 … 2^32 − 1 stay
strings, as in 3.x. (No failing test.)

### Packaging and loading

- 4.x is an ES module package backed by a WebAssembly module (the `fontpixel`
  npm package). There is no IIFE/UMD bundle or `dist/` build; the
  bdfparser-js tests that rebuild 3.x's own bundles with tsup
  (`build.test.ts`, `browser.test.ts`) do not apply and are not loaded.
  For a `<script>` tag, use `<script type="module">` and a bundler or an ESM
  CDN.
- `bdfparser` (`fontpixel/compat/bdfparser`) initializes the WebAssembly
  module with a top-level `await`. CommonJS code cannot `require()` it; use
  `await import('bdfparser')`.
- Browsers download the WebAssembly module: serve `.wasm` files as
  `application/wasm` (bundlers handle this). Its size is listed in the
  fontpixel npm README.
- Node 18 or newer.

## Native fontpixel API notes (not bdfparser behaviour)

Two Rust functions of the native fontpixel API have no Python or JavaScript
binding (coverage status `unavailable`), because they return borrowed readers
of the third-party fontations crate: `fontpixel::sfnt::Sfnt::font_ref` (use
`Sfnt.table(tag)` and the `fontpixel.sfnt.*` table decoders) and
`fontpixel::sfnt::cff::Cff::fontations` (use `Cff.glyph`, `Cff.read_all` /
`readAll` and `Cff.data`). They do not affect the bdfparser API.

## Performance

Measured on one Linux desktop (median of five processes or rounds; ratios are
3.x time ÷ 4.x time, above 1 means 4.x is faster). Full tables:
[benches/PHASE7-REPORT.md](../benches/PHASE7-REPORT.md) (Python) and
[benches/PHASE8-REPORT.md, "兼容层性能修复之后"](../benches/PHASE8-REPORT.md#兼容层性能修复之后2026-10-12)
(JavaScript, after the 2026-10-12 small-call fixes).

Python (CPython 3.10, against pure-Python bdfparser 3.0.0):

- faster: parsing (4.8× on a 5.7 MB CJK BDF, 3.3× on a 1.2 KB font), writing
  PCF (5×) and TrueType (11×), drawing many characters (30–200×),
  `drawall()` (29–290×), `font.draw()` of one character (2.5–4×),
  `glyph.draw()` (3.5×), bitmap `crop`/`concat`/`shift`/`glow` (24–1,000×),
  `todata(2/3/4/5)` (5–24×);
- about equal: `tobdf()`, `glyphbycp()`, `iterglyphs()`;
- slower: `font.headers[...]` / `font.props[...]` reads (0.16–0.19×, about
  360 ns more per read), iterating glyphs while reading `cp()` and `meta`
  (0.42×), single `glyph.meta[...]` reads (0.7×), building a `Bitmap` from rows
  (5–26× slower) and `todata(0)`/`todata(1)` (10–50× slower).

JavaScript (Node 24 and headless Chromium, against bdfparser-js 3.0.0):

- faster: parsing (1.7× Node, 2.3× Chromium on the CJK font), writing PCF
  (3.2× / 4.8×), drawing many characters (73× / 87×), `drawall` (77× / 96×),
  iterating codepoints (5.8× / 12.6×), `bitmap.glow` (2.3×),
  `todata(3/4/5)` (1.7–2.1×);
- about equal: `glyph(...)` lookup, `font.length`, `getpixel`, writing
  TrueType, iterating glyphs (0.74× Node, 1.09× Chromium);
- slower: writing BDF (0.40× Node, 0.57× Chromium), `glyph.draw()` of a
  single glyph (0.27×, about 9 µs more per call), spreading
  `{...font.headers}` (about 2.4 µs more), `new Bitmap(rows)` (about 0.75 µs
  more), `bitmap.clone/crop/shift` (0.07–0.3×), glyph metadata reads (0.5×),
  `todata(1/2)` (0.5×).

For bulk work on large fonts 4.x is faster; code dominated by many small
per-glyph or per-bitmap calls can be slower, and the browser has to download
and compile the WebAssembly module once. The native fontpixel API (`fontpixel`
in Python, `fontpixel` on npm) is faster than the compatibility layer for
nearly every operation.
