# bdfparser reference tests on `fontpixel.compat.bdfparser`: known differences

`python run_reference.py` runs the unmodified bdfparser 3.0 test suite
(`reference/bdfparser/tests`) with the `shim/` package as `bdfparser`.
Result (`reference_results.json`): **1738 passed, 25 failed, 10 skipped**
(1773 collected; the reference implementation itself: 1763 passed, 10 skipped).
The 10 skips are the reference's own slow tests (`BDFPARSER_SLOW=1`, run with
`--slow`; `reference_results_slow.json`: 5 passed, 5 failed). Every failure is
listed below.

Categories:

- **core**: a behaviour or message of the Rust compatibility layer
  (`crates/fontpixel/src/compat/bdfparser`) differs from bdfparser. The binding
  passes it through; the fix belongs in the core (reported to the coordinator).
- **intentional**: a documented design boundary of fontpixel.
- **internal**: the test inspects or monkeypatches a private bdfparser
  implementation detail that has no counterpart in the Rust-backed design.

## Core: messages differ from bdfparser

| Test | bdfparser | fontpixel (Rust) |
|---|---|---|
| `test_bdf_write.py::test_unloaded_font_raises` | `Cannot write BDF: missing header(s) fbbyoff (font not loaded?)` | `Cannot write BDF: missing headers (font not loaded?)` (names no header; `Font::tobdf`) |
| `test_cli.py::test_truncated_gzip_input` | `Malformed gzip data: …` | `invalid gzip input: incomplete deflate stream` (native gzip transport) |
| `test_font.py::TestFontV3Querying::test_draw_linelimit_too_small` | ``` `linelimit` (3) is too small the line can't even contain one glyph: … ``` | `line_limit (3) cannot contain one glyph: code Some(Unicode(97)), width 16` (native renderer) |
| `test_hardening.py::TestNonHexRows::test_message_quotes` | `Glyph "my "A"" (codepoint 65): …` | `Glyph "my \"A\"" (codepoint 65): …` (`Glyph::draw` formats the name with `{:?}`) |
| `test_hardening.py::test_glyph_detached_hexrow_error_uses_meta` | `Glyph "x" (codepoint None): …` | `Glyph "x" (codepoint 0): …` (`Glyph::cp()` maps a null codepoint to 0) |
| `test_hardening.py::TestReadTtfGuard::test_huge_glyph_rejected` | `Malformed TrueType font: glyph 1 bounding box too large (4800x4800 pixels)` | `Malformed TrueType font: glyph bounding box too large` (`ttf::read_ttf`) |
| `test_hardening.py::TestReadTtfGuard::test_huge_bdfp_box_rejected` | `… glyph 1 bounding box too large (5000x4000 pixels)` | as above |
| `test_ttf.py::test_truncated_rejected` | `Malformed TrueType font: table 'glyf' exceeds the file` | `Malformed TrueType font: table is truncated` |
| `test_ttf.py::test_missing_table_rejected` | `Malformed TrueType font: missing required table 'glyf'` | `Malformed TrueType font: required table missing` |
| `test_ttf.py::test_write_upem_too_small_and_bad_advance` | `value out of range for a TrueType field: …` | `TrueType field exceeds uint16` (`ttf::write_ttf`) |

In each case the exception class matches (`ValueError` / `BDFParseError`); only
the text differs. Rule followed by the binding: argument validation at the
Python boundary and `warnings` emission are done by the binding with
bdfparser's texts (see "Binding-supplied messages and checks"); exception texts
raised inside a Rust operation (parsing, serializing, drawing, the TrueType
reader/writer) pass through unchanged. `tobdf()` naming the missing header is
part of the Rust writer's validation, so it stays a core item.

## Core: behaviour differs from bdfparser

- `--slow` only (`reference_results_slow.json`: 5 passed, 5 failed):
  `test_ttf.py::test_fonttools_validation[HanWangYanKai-26.bdf-circle]`,
  `test_ttf.py::test_freetype_native[HanWangYanKai-26.bdf-circle-1.0-97-hinted]`,
  `test_ttf.py::test_freetype_native[HanWangYanKai-26.bdf-circle-1.0-97-unhinted]`,
  `test_ttf.py::test_round_trip[HanWangYanKai-26.bdf-circle]`,
  `test_ttf.py::test_full_unifont_slow[circle]` — `ttf::write_ttf` with round
  dots on the two large fonts stops with `ValueError: TrueType font point budget
  exceeded` (a fixed point budget inside the Rust writer); bdfparser writes these
  fonts (the HanWang circle TTF is about 68 MB). The square-dot variants of the
  same fonts pass.

- `test_bdf_write.py::test_full_format` — `setprop('FLOAT', 1.5)` then `tobdf()`:
  bdfparser quotes non-integer values (`FLOAT "1.5"`), `Font::tobdf` writes every
  JSON number bare (`FLOAT 1.5`, `v.is_number()` in font.rs). Everything else in
  this full-format golden matches (headers, comments, unencoded alt code via the
  binding's `unencoded.append`).
- `test_edit.py::TestAttached::test_two_views_of_same_glyph` — after `g1` moves
  the glyph (`setmeta(codepoint=90)`), bdfparser's second view `g2` still edits
  the moved glyph (both share the stored list). In Rust `Glyph::sync` re-adds the
  moved glyph under a new `GlyphId`, so `g2`'s slot no longer exists and `g2`
  detaches; its `shift(2)` is not written (`font.glyphs[90][4] == 0`, expected 2).
- `test_font.py::TestFontV3Misc::test_global_dwidth_used_in_mode1`,
  `test_pcf.py::test_writer_fallbacks_and_odd_hex`,
  `test_pcf.py::test_writer_global_swidth_and_bad_resolution` — the tests set a
  *single* global header component (`headers['dwx0'] = 3`, `headers['swx0'] = 333`)
  without its pair. The native model stores BDF keyword pairs (`DWIDTH`, `SWIDTH`),
  so the projection holds `[3, None]`, which `Font::drawcps` / `Font::topcf` ignore;
  bdfparser uses the lone value. Verified: with both components set
  (`dwx0=7, dwy0=0`; `swx0=333, swy0=0`) the PCF assertions hold
  (`back.glyphs[65][8] == 7`, rows `['FFF0','0010','1000','ABC0']`, `SWIDTH 333`,
  `RESOLUTION 103`). `test_writer_fallbacks_and_odd_hex` additionally ends with
  `font.unencoded[0]['meta'][0] = 'named'` before comparing with `bdftopcf`:
  item assignment into an *unencoded* glyph's meta list is not written back
  (the core has no editor for an unencoded slot; encoded glyphs' lists in
  `font.glyphs[cp]` do write back), so that final comparison would also differ.
- `test_hardening.py::TestPcfGuards::test_glyph_messages`,
  `test_pcf.py::test_writer_errors` — a glyph with a negative BBX width
  (`[-1, 2, 0, 0]`) is written by `Font::topcf` without error; bdfparser raises
  `PCFError('Glyph "A" has a negative sized bitmap, -1x1')`. (`test_writer_errors`
  also assigns a 2-slot list `['A', 65]` and expects `17-slot` at write time; the
  binding fills the missing slots on assignment because the Rust model always
  holds 17 slots, so that last assertion would also differ.)

## Intentional

- `test_bitmap.py::TestBitmapAlter::test_replace`,
  `test_bitmap.py::TestBitmapPad::test_bytepad`,
  `test_bitmap.py::TestBitmapPad::test_bytepad4` — the tests put pixel value `'3'`
  in a bitmap (`replace('2', '3')`, `bitmap_qr3_bindata`). SPEC-v3 §1.3 defines
  `bindata` as strings of `'0'`, `'1'`, `'2'`; fontpixel stores bitmaps as native
  Gray8 label rasters and refuses other labels at construction
  (`compat::bdfparser::Bitmap::new`, docs/bitmap-model.md, docs/decisions.md
  "K12": invalid construction/replacement is rejected transactionally). The
  binding raises `ValueError` instead of storing an out-of-spec label.

## Internal (private bdfparser implementation details)

- `test_edit.py::TestAttached::test_negative_codepoint_is_normalised` — the last
  assertion is `font.unencoded[0]['meta'] is g._store` (object identity with the
  private list a glyph shares with bdfparser's dict storage). Glyph metadata lives
  in the Rust model; `unencoded` is a projection. All other assertions of the test
  pass up to that line.
- `test_encoding.py::test_tottf_resolves_charset_once`,
  `test_hardening.py::TestFontCodecCache::test_cached`,
  `test_hardening.py::TestFontCodecCache::test_invalidated_by_props_and_override`
  — they monkeypatch the private method `Font._is_gl` to count GL detections.
  Charset resolution is done once per font state in Rust (`Font::codec` caches
  the `CharsetCodec` in the shared state; `setprop`, `delprop`, `encoding`,
  `addglyph`, `removeglyph` and moves clear it), so there is no Python method to
  patch. The cached behaviour itself is exercised by the passing encoding tests.
- `test_pcf.py::test_reader_swaps_partial_scan_units` — imports the private
  helper `bdfparser.pcf._swap_units`. PCF decoding is the native codec
  (`fontpixel::formats::pcf`), which has no such Python-level helper.

## Notes on passing tests that use shim helpers

`test_ttf.py` and `test_hardening.py` import the private `bdfparser.ttf` helpers
`_bindata`, `_jsnum`, `_parse_tables` and `_parse_cmap`. The shim's `ttf.py`
supplies them: `_bindata` uses the compatibility `Glyph.draw(mode=1)` (Rust);
`_jsnum`, `_parse_tables`, `_parse_cmap` are test-inspection helpers ported from
bdfparser. `test_ttf.py::test_jsnum` therefore checks the shim's port, not
fontpixel; the other uses only inspect fontpixel-written TrueType bytes.

## Binding-supplied messages and checks

These bdfparser behaviours are produced by the Python layer (the Rust layer has
no equivalent or a different text); all are exercised by the reference suite or
`tests/test_compat.py::test_python_boundary_semantics`:

- warnings: `glyphbycp` miss (`Glyph "A" (codepoint 65) does not exist in the
  font. Will return `None``; the Rust warning text is replaced), `glyph()` with an
  unrepresentable character (`… cannot be encoded in the charset …`),
  `charsetconvert` to an unknown charset (`Unknown charset '…'`), `Bitmap.overlay`
  size mismatch, `Bitmap.tobytes` unknown mode;
- argument validation: `Bitmap.fromstring` (`Invalid character 'x' in bitmap row
  N`, `Bitmap rows have different lengths`), `getpixel`/`setpixel` range
  (`IndexError`) and value checks, `addglyph` unknown key (`KeyError`) / missing
  codepoint (`The glyph has no codepoint`), `bitmap_contours` `ppu`/`dotsize`
  types, `read_ttf` `pixelsize` type, `write_ttf` `ppu`/`timestamp`/`dotsize`
  types, `write_pcf` option values, `char2cp`/`glyph` single-character `TypeError`;
- `fmtnum(-0.0)` normalizes to `'0'` (the Rust `svg::fmtnum` writes `'-0'`);
- a malformed PCF read through `Font(...)`/`load_bytes`/`load_file_path` inside a
  gzip wrapper raises `PCFError` (the binding peeks at the decompressed magic).

## Binding behaviour notes (not covered by a failing test)

- Header writes (`font.headers[...] = …`, `del`, assignment), `unencoded.append`
  with an alt code and `unencoded.clear()`/`del` rebuild the native model
  (`Font::core` → edit → `Font::from_core`) because the Rust layer has no header
  or unencoded-slot editor. Glyph handles taken before such a write detach.
- Item assignment into `font.glyphs[cp]` lists is written back; into
  `font.unencoded[i]['meta']` lists it is not.
- `Glyph.draw()`, `origin()`, `clone()` and `drawcps(missing=glyph)` read pending
  direct `glyph.meta` edits without writing them back; the next editing method
  writes them, as in bdfparser.
- `write_ttf`/`write_pcf` accept bdfparser's duck-typed font objects (any object
  with `headers`, `props`, `glyphs`, optionally `unencoded`, `_propnames`) by
  copying them into a compatibility `Font` first.
