# bdfparser reference tests on `fontpixel.compat.bdfparser`: known differences

`python run_reference.py` runs the unmodified bdfparser 3.0 test suite
(`reference/bdfparser/tests`) with the `shim/` package as `bdfparser`.
Result (`reference_results.json`): **1755 passed, 8 failed, 10 skipped**
(1773 collected; the reference implementation itself: 1763 passed, 10 skipped).
The 10 skips are the reference's own slow tests (`BDFPARSER_SLOW=1`); run with
`python run_reference.py --slow --json reference_results_slow.json -- -m slow`
they all pass (`reference_results_slow.json`: 10 passed, including the round-dot
TrueType fonts of HanWangYanKai-26 and the full unifont). Every remaining
failure is listed below; none is a behaviour of the Rust compatibility layer.

Categories:

- **intentional**: a documented design boundary of fontpixel.
- **internal**: the test inspects or monkeypatches a private bdfparser
  implementation detail that has no counterpart in the Rust-backed design.

## Intentional (3)

- `test_bitmap.py::TestBitmapAlter::test_replace`,
  `test_bitmap.py::TestBitmapPad::test_bytepad`,
  `test_bitmap.py::TestBitmapPad::test_bytepad4` — the tests put pixel value `'3'`
  in a bitmap (`replace('2', '3')`, `bitmap_qr3_bindata`). SPEC-v3 §1.3 defines
  `bindata` as strings of `'0'`, `'1'`, `'2'`; fontpixel stores bitmaps as native
  Gray8 label rasters and refuses other labels at construction
  (`compat::bdfparser::Bitmap::new`, docs/bitmap-model.md, docs/decisions.md
  "K12": invalid construction/replacement is rejected transactionally). The
  binding raises `ValueError` instead of storing an out-of-spec label.
  Evidence: `tests/test_bitmap.py:83` and `:595` fail with `ValueError` from
  `Bitmap` construction; every other assertion of these classes passes.

## Internal (5): private bdfparser implementation details

- `test_edit.py::TestAttached::test_negative_codepoint_is_normalised` — its last
  but one assertion (`tests/test_edit.py:299`) is
  `font.unencoded[0]['meta'] is g._store`: object identity with the private list
  a glyph shares with bdfparser's dict storage (`AttributeError: _store`).
  Glyph metadata lives in the Rust model; `unencoded` is a projection. All
  assertions before it pass (the codepoint normalisation and moves themselves).
- `test_encoding.py::test_tottf_resolves_charset_once`,
  `test_hardening.py::TestFontCodecCache::test_cached`,
  `test_hardening.py::TestFontCodecCache::test_invalidated_by_props_and_override`
  — they monkeypatch the private method `Font._is_gl` to count GL detections
  (`tests/test_encoding.py:549`, `tests/test_hardening.py:249`: `AttributeError:
  _is_gl`). Charset resolution is done once per font state in Rust
  (`Font::codec` caches the `CharsetCodec`; `setprop`, `delprop`, `encoding`,
  `addglyph`, `removeglyph` and moves clear it), so there is no Python method to
  patch. The cached behaviour itself is exercised by the passing encoding tests.
- `test_pcf.py::test_reader_swaps_partial_scan_units` — imports the private
  helper `bdfparser.pcf._swap_units` (`tests/test_pcf.py:1297`: `ImportError`).
  PCF decoding is the native codec (`fontpixel::formats::pcf`), which has no such
  Python-level helper.

## Fixed in the Rust compatibility layer (phase 7 unit P6)

The 17 former "core" failures (and the 5 slow TrueType failures) pass now:

- editors: `Font::set_headers`/`set_header`/`remove_header` and the
  unencoded-slot editors (`unencoded_handle(s)`, `set_unencoded_meta`,
  `set_unencoded_altcode`, `remove_unencoded`, public `put_with_alt`) edit the
  model in place, so header writes and `unencoded` edits keep glyph handles
  attached and keep the warning report; item assignment into
  `font.unencoded[i]['meta']` is written back (`test_writer_fallbacks_and_odd_hex`
  then matches `bdftopcf`);
- a handle follows its glyph when another handle moves it
  (`test_two_views_of_same_glyph`);
- a lone global header component (`dwx0`, `dwy0`, `swx0`) is used by
  `drawcps` and `topcf` (`test_global_dwidth_used_in_mode1`,
  `test_writer_fallbacks_and_odd_hex`, `test_writer_global_swidth_and_bad_resolution`);
- `tobdf` quotes non-integer property values (`test_full_format`);
- `topcf` refuses a negative BBX size with bdfparser's `PCFError` text
  (`test_glyph_messages`, `test_writer_errors`);
- bdfparser's texts for: `tobdf` missing headers, gzip, `linelimit`, hex rows
  (name and `None` codepoint), `read_ttf` (table exceeds the file, missing
  table, glyph bounding box) and `write_ttf` (`value out of range for a TrueType
  field: (values) (struct.error text)`, the values of the failing pack call and
  CPython 3.10's `struct.error` wording);
- the round-dot TrueType writer admits outline points by memory (see
  docs/decisions.md, "bdfparser compatibility: editors, messages and the
  TrueType memory admission").

## Remaining narrow differences (no failing test)

- A header pair (`dwx0`/`dwy0`, …) is one native BDF keyword, so the projection
  cannot tell an absent component from an explicit None: after
  `headers['dwx0'] = 3` alone, `headers` also shows `'dwy0': None`, and
  `headers = {'dwx0': None, 'dwy0': 4}` draws with spacing 4 where bdfparser
  uses no global spacing.
- `font.glyphs[cp] = [...]` with fewer than 17 values is stored padded to 17
  slots (the native model always holds 17); the Python layer remembers the code
  and `write_pcf` raises bdfparser's `glyph meta must be a 17-slot list`
  (`test_writer_errors`). Other writers see the padded list (bdfparser would
  fail with an `IndexError`).
- The `linelimit` message prints `None` for a negative requested code (the
  binding passes `None` codes to Rust as -1).
- `write_ttf`'s pack message groups values as bdfparser's `_pack` calls do;
  only the message of a value bdfparser cannot reach could differ.
- A lone surrogate from a `cp2char` converter cannot be quoted in Rust's
  `glyphbycp` miss warning; it prints as `Glyph (codepoint N)`.

## Notes on passing tests that use shim helpers

`test_ttf.py` and `test_hardening.py` import the private `bdfparser.ttf` helpers
`_bindata`, `_jsnum`, `_parse_tables` and `_parse_cmap`. The shim's `ttf.py`
supplies them: `_bindata` uses the compatibility `Glyph.draw(mode=1)` (Rust);
`_jsnum`, `_parse_tables`, `_parse_cmap` are test-inspection helpers ported from
bdfparser. `test_ttf.py::test_jsnum` therefore checks the shim's port, not
fontpixel; the other uses only inspect fontpixel-written TrueType bytes.

## Binding-supplied messages and checks

All warnings and the texts of errors raised inside Rust operations come from the
Rust layer. The Python layer adds only argument validation at the Python
boundary, with bdfparser's texts; all are exercised by the reference suite or
`tests/test_compat.py::test_python_boundary_semantics`:

- warnings: `Bitmap.overlay` size mismatch, `Bitmap.tobytes` unknown mode;
- argument validation: `Bitmap.fromstring` (`Invalid character 'x' in bitmap row
  N`, `Bitmap rows have different lengths`), `getpixel`/`setpixel` range
  (`IndexError`) and value checks, `addglyph` unknown key (`KeyError`) / missing
  codepoint (`The glyph has no codepoint`), `bitmap_contours` `ppu`/`dotsize`
  types, `read_ttf` `pixelsize` type, `write_ttf` `ppu`/`timestamp`/`dotsize`
  types, `write_pcf` option values and the 17-slot check above,
  `char2cp`/`glyph` single-character `TypeError`;
- a malformed PCF read through `Font(...)`/`load_bytes`/`load_file_path` inside a
  gzip wrapper raises `PCFError` (the binding peeks at the decompressed magic).

## Binding behaviour notes (not covered by a failing test)

- `Glyph.draw()`, `origin()`, `clone()` and `drawcps(missing=glyph)` read pending
  direct `glyph.meta` edits without writing them back; the next editing method
  writes them, as in bdfparser.
- `write_ttf`/`write_pcf` accept bdfparser's duck-typed font objects (any object
  with `headers`, `props`, `glyphs`, optionally `unencoded`, `_propnames`) by
  copying them into a compatibility `Font` first.
