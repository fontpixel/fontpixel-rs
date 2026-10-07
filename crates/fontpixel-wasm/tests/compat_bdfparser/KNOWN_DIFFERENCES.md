# bdfparser-js reference tests on `fontpixel/compat/bdfparser`: known differences

`node run_reference.mjs --work DIR [--install]` runs the unmodified bdfparser-js
3.0.0 test suite (`reference/bdfparser-js/test`, vitest 5.0.3, Node 24) with every
`../src/<module>` import resolved to `shim/<module>.mjs`, which re-exports the
compatibility API of the built `full` package. The test tree is copied to the
work directory and the fonts that the bdfparser-js package deduplicated
(`test/fonts/DEDUPED.txt`) are restored from `reference/bdfparser/tests/fonts`
(HanWangYanKai-26 and the full unifont included).

Results (`reference_results.json`, `reference_results_slow.json`):

| run | passed | failed | skipped | suites not loaded |
|---|---|---|---|---|
| default | 1178 | 11 | 3 | 2 (`build.test.ts`, `browser.test.ts`) |
| `--slow` (`BDFPARSER_SLOW_TESTS=1`) | 1181 | 11 | 0 | 2 |
| bdfparser-js itself, same environment | 1189 (1192 with `--slow`) | 0 | 3 (0) | 2 |

The 3 skips are the reference's own slow TrueType tests (HanWangYanKai-26 square
and round dots, the reduced unifont with both shapes); with `--slow` they pass.
Every failure is listed below with its category, evidence and reason; none is an
unexplained behaviour of the compatibility layer.

Categories:

- **environment**: the test checks bdfparser-js's own build artefacts.
- **intentional**: a documented design boundary of fontpixel (the same
  boundaries as the Python compatibility layer, see
  `crates/fontpixel-py/tests/compat_bdfparser/KNOWN_DIFFERENCES.md`).
- **internal**: the test depends on an implementation detail of bdfparser-js's
  own code that has no counterpart in the Rust-backed design.

## Environment (2 suites)

- `test/build.test.ts`, `test/browser.test.ts` — they rebuild bdfparser-js's
  `dist/` bundles with `tsup` and its `tsup.config.ts` (`build.test.ts:16`,
  `browser.test.ts:12`: `import { build } from 'tsup'`) and check those bundles
  (class names, tree shaking, the IIFE in a bare `vm` context). They test the
  reference package's build output, not an API; they cannot load here (no
  `tsup`, no bdfparser-js sources), and they fail the same way for the reference
  implementation in this environment.

## Intentional (7)

- `bitmap.test.ts > Bitmap alter replace` (`bitmap.test.ts:87`:
  `replace('2', '3')`), `bitmap.test.ts > Bitmap pad bytepad` and `bytepad4`
  (`bitmap.test.ts:545`, `info.ts:68`: `bitmap_qr3_bindata` holds `'3'`),
  `svg.test.ts > Bitmap.draw2canvas v2 / v3-rc signature ... still works`
  (`svg.test.ts:243`: `new Bitmap(['13'])`) — pixel value `'3'`. SPEC-v3 §1.3
  defines `bindata` as strings of `'0'`, `'1'`, `'2'`; fontpixel keeps bitmaps as
  native Gray8 label rasters and refuses other labels at construction
  (`compat::bdfparser::Bitmap::new`, decision "K12"), exactly as the Python layer
  (whose suite fails the same three bitmap tests). Every other assertion of
  these tests before the `'3'` passes.
- `hardening.test.ts > §11.4 parsing and drawing overlay keeps its size, aligns
  at the top-left, clips (K04)` (`hardening.test.ts:441`:
  `new Bitmap(['1111', '1', '1'])`) and `parse.test.ts > glyph section fewer hex
  rows than BBX height: padded when drawing, warned once` (`parse.test.ts:538`:
  `draw(2)` of rows `F` and `FF` expected as the ragged `['1111', '11111111']`) —
  ragged bitmaps. A raster is rectangular; rows of different lengths are outside
  the SPEC input domain and are rejected ("Bitmap rows have different lengths" /
  "Bitmap rows must be rectangular labels 0, 1, 2", K12). The size-mismatch
  warnings and the padding warning (once per glyph) before those lines pass.
- `encoding.test.ts > v3 charsets multi-byte charsets without bdfparser/cjk:
  restricted TextDecoder, one warning` (`encoding.test.ts:600`) and `... iconv-lite
  instead of TextDecoder` (`encoding.test.ts:627`) — they check bdfparser-js's
  degraded mode when its optional CJK tables are not imported (`exact` false, a
  "tables are not loaded" warning, `TextDecoder`/iconv-lite results such as
  `cp2char(0x8862, 'big5hkscs')` from the runtime). fontpixel's `full` module
  always contains the exact tables generated from CPython's codecs, so
  `getCodec(...).exact` is `true`, there is no fallback warning and `iconv` is
  not used; `fontpixel/compat/bdfparser/cjk` only registers a marker. Exact
  conversions are what SPEC-v3 §11.1 requires; the exhaustive table tests
  (`charsets.test.ts`, every key and the GB18030 4-byte space) all pass.
- `pcf.test.ts > more edge cases readPcfAsync without DecompressionStream`
  (`pcf.test.ts:1396`) — expects a rejection when `globalThis.DecompressionStream`
  is missing. fontpixel decodes gzip in Rust (bounded, the same decoder as the
  native reader), so `readPcfAsync`, `load_bytes_async` and `$Font` need no
  `DecompressionStream` and succeed in such runtimes.
- `pcf.test.ts > more edge cases writer: unusual model values`
  (`pcf.test.ts:1327`, `:1344`) — `font.props.flag = true`. A boolean has no
  SPEC-v3 property type (int, str, null); the shared model stores it as the
  Python reference does (a bool is an int: `FLAG 1` in BDF and PCF), where
  bdfparser-js writes `FLAG "true"` in BDF and drops it from PCF. All other
  assertions of the test pass (checked separately: lower-case key spelling,
  FONT_ASCENT fallback, SWIDTH from FONTBOUNDINGBOX, point size 0, empty name and
  rows, 75 dpi fallbacks, empty FONT).

## Internal (1)

- `pcf.test.ts > readPcf malformed input gives clear errors` (`pcf.test.ts:1223`)
  — it matches the error texts of bdfparser-js's own PCF reader. PCF decoding is
  the native codec (`fontpixel::formats::pcf`), whose diagnostics match all but
  three of the expected patterns: a directory entry whose offset overflows
  (`pcf.test.ts:1263`) reports `PCF table range overflow` (expected
  `/truncated/`), a bad METRICS format word (`:1269`) reports `unsupported PCF
  table format 0x20e` (expected `/Unsupported PCF metrics format/`), and an
  overlong property count (`:1293`) reports `PCF count overflow` (expected
  `/truncated/`). The truncation sweep (every prefix of the file) passes: each
  error mentions PCF and none is a `RangeError`, and successful partial reads
  give the full font.

## Fixed for the JS layer (were failures of the first run)

Behaviour differences of the compatibility layer found by the suite and fixed:

- Rust core (`fontpixel::compat::bdfparser`, shared with Python; see
  docs/decisions.md "bdfparser-js compatible API"): the TrueType writer reports a
  non-hex bitmap row with SPEC-v3 §11.6's text (`Glyph "g" (codepoint 65):
  bitmap row 1 "G0" contains a non-hex character`); the BDFP reader uses
  bdfparser's `Malformed BDFP table: bad ppu / bad structure / missing key /
  bad glyph entry` texts and treats a non-object `propnames` as empty; `tobdf`
  iterates a `comment` value as bdfparser does (a falsy value is no comment, a
  string gives one line per character, any other truthy value is an error,
  `TypeError` in JS).
- JS layer: parse diagnostics quoted as bdfparser-js does (`JSON.stringify` for
  `malformed number in ...`, `... needs N numbers, got ...`, `malformed version
  number in STARTFONT ...`, `malformed property count ...`; plain single quotes
  for `unknown header keyword`, `unexpected line`, `unknown keyword ... in glyph
  ...`, `property ... treated as a COMMENT line`), where the Rust core uses
  Python's `repr`; the missing-rows warning once per glyph (until the glyph is
  edited); one leading BOM skipped (by the Rust loader only); PCF directory
  entries of unknown table types ignored and the native PCF decoder's notes kept
  out of the warning handler (bdfparser's readers are silent); non-integral
  numeric properties written unquoted by `tobdf` (`X 1.5`, bdfparser-js's
  `fmtnum`; the Python reference quotes them); `readTtf` checks `pixelsize` only
  for fonts without BDFP metadata (`RangeError`); `readPcfAsync` words gzip
  errors as bdfparser-js (`Cannot decompress gzip-compressed PCF: ...`).

## Shim-supplied helpers

`shim/Font.mjs` exports `gunzip` and `shim/outline.mjs` exports `flattenContour`:
they are bdfparser-js internals (`@internal`) without a fontpixel counterpart
(the Rust loader decodes gzip itself, the Rust rasterizer flattens curves
internally), ported from bdfparser-js as test-inspection helpers so the test
modules resolve. `parse.test.ts > load_bytes / load_bytes_async / $Font gunzip
errors` and the four `outline.test.ts > flattenContour` tests therefore check
the ports, not fontpixel; the other `flattenContour` use
(`outline.test.ts:521`) only inspects the orientation of contours produced by
fontpixel's `bitmapContours`.

## Binding behaviour notes (no failing test)

- The views (`headers`, `props`, `glyphs`, `unencoded`, `_propnames`) return
  copies of arrays: `font.headers.comment.push('x')` or
  `font.unencoded[0].meta[16].push('00')` do not change the font (assign the
  slot instead). bdfparser-js returns its stored arrays.
- A header pair (`dwx0`/`dwy0`, ...) is one native BDF keyword: after
  `headers.dwx0 = 3` alone, `headers.dwy0` reads `null` (bdfparser-js:
  `undefined`); the Python layer has the same narrow difference.
- `font.glyphs.set(cp, list)` with fewer than 17 values is stored padded to 17
  slots; `writePcf` then refuses it with `glyph meta must be a 17-slot list`,
  as the Python layer does.
- An integer property set beyond 2^53 − 1 (a `bigint`) is kept exact and reads
  back as a `bigint` (the plain-data number policy); parsed unquoted values
  outside −2^31 … 2^32 − 1 stay strings, as in bdfparser-js.
- Native `RawFont` warnings that bdfparser-js has no counterpart for (for
  example the Rust `glyphbycp`'s missing-glyph warning) are only produced by
  the raw handles; the bdfparser-js methods (`glyphbycp`, `glyph`) do not warn,
  as in bdfparser-js.
