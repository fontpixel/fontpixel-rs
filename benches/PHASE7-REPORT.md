# Phase 7 Python benchmark and manylinux wheels

Status (2026-10-06): final run with fontpixel built from main `0020c26` (after the
bdfparser compatibility fixes in `a97b916`) by `maturin develop --release`.
Method and case list: [phase7-README.md](phase7-README.md). Raw data:
[results/phase7-python-final.json](results/phase7-python-final.json). The two
earlier runs on `f4eb45b`, before those fixes, are kept below as history
([run 1](results/phase7-python-run1.json), [run 2](results/phase7-python-run2.json)).

## Short answer for bdfparser

Measured against pure-Python bdfparser 3.0.0 on CPython 3.10. Ratios are
bdfparser time ÷ fontpixel time, so above 1 means fontpixel is faster. Ranges
are the paired per-repetition ratios of the final run.

| | through `fontpixel.compat.bdfparser` (drop-in) | through the native API |
| --- | --- | --- |
| **Faster** | parse (4.5–4.9× on Tong SC, 3.2× on a 1.2 KB font), PCF 4–5×, TTF 11–14×, drawing '永'×2000 26–31×, drawing 2,000 different characters 176–207×, `drawall(mode=1)` 280–290×, `drawall()` with defaults 25–31×, `font.draw('永')` 2.5–4×, `glyph.draw()` 3.5×, `font.glyph()` 1.3–1.4×, bitmap `crop`/`concat`/`shift`/`glow` 24–1,000×, `clone` 2.1–2.5×, `getpixel` 5×, `todata(2/3/4/5)` 5–24× | everything in the left column, and more: drawing 34–690×, glyph lookup 11–18×, iterating glyphs 13–14×, reading codepoint + advance of every glyph 2.2–2.4× |
| **About equal** | `tobdf()` (0.88–0.91×), `glyphbycp()` (0.89–0.99×), `iterglyphs()` (1.02–1.12×) | — |
| **Slower** | iterating glyphs while reading `cp()` and `meta['dwx0']` (0.41–0.45×), single `glyph.meta[...]`/`glyph.cp()` reads (0.69–0.77×, about 25–30 ns more per call), `font.headers[...]`/`font.props[...]` (0.16–0.19×, 430 ns against 70 ns), `Bitmap(rows)` 5–26×, `todata(0)`/`todata(1)` 10–50× | attribute reads that build a fresh dict or list (`font.metrics` 21–23×, `glyph.metrics` 9–11×, `glyph.codepoints` 3×), `Bitmap.from_text` 5–40×, `to_text` 20–100× |
| **Fails** | — (every case completes and gives bdfparser's result) | — |

An optional "use fontpixel when installed" switch in bdfparser is now a clear
win for whole-font work and for drawing: loading, writing PCF/TTF, `draw`,
`drawall` and bitmap effects. It is no better for writing BDF. It still loses
for code that reads `headers`/`props` or per-glyph metadata in tight loops, or
builds bitmaps from strings and reads them straight back as strings. These
remaining costs are per-call Python-object overheads (see "Why the remaining
slow cases are slow"); none of them is a fixed per-font cost any more.

The fixes that changed the verdicts (commit `a97b916`, recorded in
[docs/decisions.md](../docs/decisions.md)):

- `draw`/`drawcps` prepare the drawing model once per edit revision instead of
  cloning and re-projecting the whole font per call: `font.draw('永')` went from
  73.5 ms to 11.7 µs on Tong SC.
- Drawing admits result images by memory (packed render plus its 8-bit copy
  within `ResourceLimits::max_total_bytes`) instead of the native
  16,777,216-pixel cap, as bdfparser has no pixel cap. bdfparser's default
  `drawall()` on Tong SC (22.3 Mpx) now succeeds in 55 ms. The native API keeps
  its pixel cap; the difference is listed in
  `crates/fontpixel-py/tests/compat_bdfparser/KNOWN_DIFFERENCES.md`.
- Glyph handles share one metadata snapshot and build `glyph.meta` on first
  access; `headers`/`props` read a projection cached per edit revision;
  `todata` builds Python values directly instead of through `serde_json`.

## Machine and load

- CPU: 12th Gen Intel Core i7-12700H (6 performance cores with SMT and 8
  efficiency cores, 20 logical CPUs); 62.5 GiB RAM; Linux 7.0.0-38-generic.
- CPython 3.10.20 (pyenv) for all three engines. bdfparser 3.0.0 runs from a
  copy of `reference/bdfparser/src` (tree SHA-256 `e44e944f…`).
- Final run: fontpixel 0.1.0 from `maturin develop --release --locked` at main
  `0020c26`, clean tree (`_native.abi3.so` SHA-256 `e9ae0d8b…`).
- Runs 1 and 2: fontpixel from the earlier x86_64 manylinux wheel
  (`_native.abi3.so` SHA-256 `974ba9fe…`), built from sources identical to main
  `f4eb45b`, before the compatibility fixes. They used the harness as committed
  in `5356121`; later commits changed only the `summarize`/`compare` output code.

The machine is a shared desktop. During all three runs other work was active:
browsers, other agents' Python jobs and Rust builds. Each measured process ran
as `systemd-run --user --scope -p CPUWeight=10000 -- taskset -c 0-11`, which
pins it to the six performance cores and their SMT siblings, the same for every
engine. Within each repetition the three engines ran back to back, so changes in
load hit them alike. The 1-minute load average at the start and end of the
measured processes was:

| Run | Time (CEST) | min | median | max |
| --- | --- | ---: | ---: | ---: |
| 1 | 14:13–14:28 | 2.91 | 3.53 | 19.13 |
| 2 | 14:29–14:44 | 2.56 | 7.55 | 12.59 |
| final | 15:56–16:07 | 2.45 | 3.51 | 5.35 |

Load inflates absolute times. Process CPU time stayed close to wall time, so
the cause is shared cores, memory bandwidth and clock speed, not descheduling.
For example, bdfparser's 2,000-different-character draw took 0.62 s in a single
check at a calmer moment (1-minute load 2.6), 2.2–3.2 s in run 1 and
0.71–0.75 s in run 2. Read the absolute numbers as indications only. The
speedup ratios are stable: in runs 1 and 2 (same build) each ratio agrees
between the two runs within the spread of the paired per-repetition ratios, and
no case changes verdict between them except the `noop` baseline, which is
noise. The final run had the calmest load of the three (median 3.5, max 5.4).

## Final run (median of 5 fresh processes per engine)

`same result as ref` compares SHA-256 digests of the output (bytes, bitmap rows
or values). The native TTF comes from fontpixel's own TrueType writer, not
bdfparser's format, so its bytes differ by design. Its BDF and PCF output match
bdfparser's bytes for this font. The compat TTF is byte-identical to
bdfparser's.

| Case | pure-Python bdfparser | compat | native | compat vs ref | native vs ref | same result as ref (compat / native) |
| --- | ---: | ---: | ---: | --- | --- | --- |
| `parse_tong_sc` (whole): Parse Tong SC BDF (5.7 MB, 38,653 glyphs) | 553.6 ms | 116.2 ms | 58.2 ms | 4.76× faster | 9.52× faster | yes / yes |
| `parse_latin` (whole): Parse Tong Latin BDF (419 KB) | 45.8 ms | 9.1 ms | 4.2 ms | 5.05× faster | 10.90× faster | yes / yes |
| `write_bdf` (whole): Write BDF (Tong SC) | 70.8 ms | 79.1 ms | 38.3 ms | 0.90× about equal | 1.85× faster | yes / yes |
| `write_pcf` (whole): Write PCF (Tong SC) | 708.2 ms | 142.3 ms | 32.7 ms | 4.98× faster | 21.65× faster | yes / yes |
| `write_ttf` (whole): Write TTF (Tong SC) | 12.30 s | 1.12 s | 484.7 ms | 10.97× faster | 25.38× faster | yes / no |
| `draw_2000_repeat` (whole): Draw '永'×2000 (mode 0, linelimit 80×fbbx; phase-6 workload) | 65.3 ms | 2.2 ms | 829.22 µs | 30.25× faster | 78.80× faster | yes / yes |
| `draw_2000_mixed` (whole): Draw 2,000 different characters (mode 1, linelimit 512) | 601.5 ms | 3.0 ms | 1.6 ms | 199.43× faster | 384.46× faster | yes / yes |
| `drawall_mode1` (whole): drawall(mode=1), all 38,653 glyphs | 12.19 s | 41.7 ms | 18.2 ms | 292.00× faster | 670.11× faster | yes / yes |
| `drawall_mode0_default` (whole): drawall() defaults (mode 0, 22.3 Mpx); native with max_pixels raised to 2^26 | 1.59 s | 54.9 ms | 18.6 ms | 28.94× faster | 85.41× faster | yes / yes |
| `iterglyphs` (whole): Iterate all glyph objects (Tong SC) | 85.5 ms | 81.5 ms | 6.4 ms | 1.05× about equal | 13.34× faster | yes / yes |
| `iterglyphs_meta` (whole): Iterate all glyphs reading codepoint + advance | 90.0 ms | 215.4 ms | 40.2 ms | 0.42× slower | 2.24× faster | yes / yes |
| `noop` (small): Baseline: empty Python closure call | 51 ns | 51 ns | 51 ns | 1.00× about equal | 1.00× about equal | yes / yes |
| `parse_tiny` (small): Parse a 1.2 KB BDF (unifont 2atest, 3 glyphs) | 105.40 µs | 32.27 µs | 30.45 µs | 3.27× faster | 3.46× faster | yes / yes |
| `glyph_lookup` (small): font.glyph('永') / native glyph_for_codepoint | 3.30 µs | 2.48 µs | 198 ns | 1.33× faster | 16.70× faster | yes / yes |
| `glyphbycp` (small): font.glyphbycp(0x6C38) / native glyph_for_codepoint | 2.17 µs | 2.35 µs | 196 ns | 0.92× about equal | 11.03× faster | yes / yes |
| `attr_font_header` (small): font.headers['fbbx'] / native font.metrics['bounds']['width'] | 67 ns | 428 ns | 1.47 µs | 0.16× slower | 0.046× slower | yes / yes |
| `attr_font_prop` (small): font.props['font_ascent'] / native font.properties.get('FONT_ASCENT') | 72 ns | 429 ns | 288 ns | 0.17× slower | 0.25× slower | yes / yes |
| `attr_glyph_meta` (small): glyph.meta['dwx0'] / native glyph.metrics['advance']['x'] | 67 ns | 93 ns | 665 ns | 0.72× slower | 0.10× slower | yes / yes |
| `attr_glyph_cp` (small): glyph.cp() / native glyph.codepoints[0].value | 71 ns | 100 ns | 211 ns | 0.71× slower | 0.34× slower | yes / yes |
| `glyph_draw` (small): glyph.draw() in the font box / native font.glyph_in_cell(id) | 20.16 µs | 5.81 µs | 322 ns | 3.47× faster | 62.63× faster | yes / yes |
| `draw_one_char` (small): font.draw('永') (one-glyph render) | 41.29 µs | 11.72 µs | 1.21 µs | 3.52× faster | 34.08× faster | yes / yes |
| `bitmap_new_16` (small): Bitmap(16 row strings) / native Bitmap.from_text | 292 ns | 1.57 µs | 1.37 µs | 0.19× slower | 0.21× slower | yes / yes |
| `bitmap_new_64` (small): Bitmap(64 row strings of 64) / native Bitmap.from_text | 292 ns | 7.42 µs | 11.01 µs | 0.039× slower | 0.027× slower | yes / yes |
| `bitmap_clone` (small): b.clone() / native b.copy() (64×64) | 611 ns | 264 ns | 227 ns | 2.31× faster | 2.69× faster | yes / yes |
| `bitmap_crop` (small): b.clone().crop(31, 31, 3, 5) / native b.crop(...) (64×64) | 12.25 µs | 495 ns | 628 ns | 24.73× faster | 19.52× faster | yes / yes |
| `bitmap_concat` (small): b.clone().concat(o) right/bottom / native b.concat(o, "right", "end") | 80.05 µs | 1.22 µs | 1.08 µs | 65.53× faster | 74.22× faster | yes / yes |
| `bitmap_shift` (small): b.clone().shift(3, -2) / native b.shift(3, -2) (64×64) | 30.53 µs | 843 ns | 1.05 µs | 36.21× faster | 28.94× faster | yes / yes |
| `bitmap_glow` (small): b.clone().glow() / native gray8 b.glow(False, 1, 2) (64×64) | 1.8 ms | 1.98 µs | 20.78 µs | 906.36× faster | 86.25× faster | yes / yes |
| `todata_0` (small): b.todata(0) / native to_text({0:"0",1:"1"}) (64×64) | 630 ns | 6.71 µs | 13.12 µs | 0.094× slower | 0.048× slower | yes / yes |
| `todata_1` (small): b.todata(1) / native to_text(...).split("\n") (64×64) | 184 ns | 7.11 µs | 18.19 µs | 0.026× slower | 0.01× slower | yes / yes |
| `todata_2` (small): b.todata(2) / native to_rows() (64×64) | 811.70 µs | 41.48 µs | 57.50 µs | 19.57× faster | 14.12× faster | yes / yes |
| `todata_3` (small): b.todata(3) / native to_list() (64×64) | 859.13 µs | 57.59 µs | 54.38 µs | 14.92× faster | 15.80× faster | yes / yes |
| `todata_4` (small): b.todata(4) / native to_numeric_hex_rows() (64×64) | 66.12 µs | 11.95 µs | 11.78 µs | 5.53× faster | 5.61× faster | yes / yes |
| `todata_5` (small): b.todata(5) / native to_row_values() (64×64) | 14.45 µs | 2.43 µs | 1.19 µs | 5.96× faster | 12.14× faster | yes / yes |
| `getpixel` (small): b.getpixel(10, 20) / native b.get_pixel(10, 20) | 567 ns | 117 ns | 113 ns | 4.84× faster | 5.02× faster | yes / yes |

- `write_pcf` compat: spread min 135.8 ms / max 182.7 ms
- `write_pcf` native: spread min 31.9 ms / max 43.3 ms
- `write_ttf` ref: spread min 11.70 s / max 16.18 s
- `draw_one_char` compat: spread min 9.98 µs / max 16.06 µs
- `todata_1` ref: spread min 179 ns / max 444 ns
- `todata_1` compat: spread min 6.72 µs / max 17.75 µs
- `todata_1` native: spread min 15.76 µs / max 35.08 µs
- `todata_2` ref: spread min 782.55 µs / max 1.0 ms
- `todata_2` compat: spread min 37.39 µs / max 48.74 µs
- `todata_2` native: spread min 54.60 µs / max 68.30 µs
- `todata_3` compat: spread min 47.60 µs / max 73.14 µs
- `todata_3` native: spread min 46.33 µs / max 70.95 µs
- `todata_4` ref: spread min 62.16 µs / max 82.11 µs
- `todata_4` compat: spread min 10.98 µs / max 14.47 µs
- `todata_4` native: spread min 11.34 µs / max 14.80 µs

load average (1 min) at child start/end: min 2.45, median 3.51, max 5.35

## Before the compatibility fixes: run 1 (median of 5 fresh processes per engine)

These numbers are from `f4eb45b` and are superseded by the final run. They are
kept because they show the defects the fixes removed.

`same result as ref` compares SHA-256 digests of the output (bytes, bitmap rows
or values). The native TTF comes from fontpixel's own TrueType writer, not
bdfparser's format, so its bytes differ by design. Its BDF and PCF output match
bdfparser's bytes for this font. The compat TTF is byte-identical to
bdfparser's.

| Case | pure-Python bdfparser | compat | native | compat vs ref | native vs ref | same result as ref (compat / native) |
| --- | ---: | ---: | ---: | --- | --- | --- |
| `parse_tong_sc` (whole): Parse Tong SC BDF (5.7 MB, 38,653 glyphs) | 563.0 ms | 120.1 ms | 61.7 ms | 4.69× faster | 9.13× faster | yes / yes |
| `parse_latin` (whole): Parse Tong Latin BDF (419 KB) | 48.4 ms | 9.4 ms | 4.4 ms | 5.13× faster | 11.10× faster | yes / yes |
| `write_bdf` (whole): Write BDF (Tong SC) | 73.5 ms | 77.6 ms | 40.5 ms | 0.95× about equal | 1.82× faster | yes / yes |
| `write_pcf` (whole): Write PCF (Tong SC) | 765.0 ms | 107.9 ms | 35.6 ms | 7.09× faster | 21.48× faster | yes / yes |
| `write_ttf` (whole): Write TTF (Tong SC) | 16.62 s | 1.46 s | 623.0 ms | 11.39× faster | 26.68× faster | yes / no |
| `draw_2000_repeat` (whole): Draw '永'×2000 (mode 0, linelimit 80×fbbx; phase-6 workload) | 94.7 ms | 111.5 ms | 1.1 ms | 0.85× about equal | 87.06× faster | yes / yes |
| `draw_2000_mixed` (whole): Draw 2,000 different characters (mode 1, linelimit 512) | 2.83 s | 294.0 ms | 7.2 ms | 9.62× faster | 392.60× faster | yes / yes |
| `drawall_mode1` (whole): drawall(mode=1), all 38,653 glyphs | 14.34 s | 118.9 ms | 20.4 ms | 120.57× faster | 703.31× faster | yes / yes |
| `drawall_mode0_default` (whole): drawall() defaults (mode 0, 22.3 Mpx); native with max_pixels raised to 2^26 | 1.61 s | **error** | 17.9 ms | — | 90.13× faster | — / yes |
| `iterglyphs` (whole): Iterate all glyph objects (Tong SC) | 86.0 ms | 262.0 ms | 6.3 ms | 0.33× slower | 13.57× faster | yes / yes |
| `iterglyphs_meta` (whole): Iterate all glyphs reading codepoint + advance | 89.7 ms | 256.6 ms | 41.2 ms | 0.35× slower | 2.18× faster | yes / yes |
| `noop` (small): Baseline: empty Python closure call | 51 ns | 52 ns | 51 ns | 0.99× about equal | 1.00× about equal | yes / yes |
| `parse_tiny` (small): Parse a 1.2 KB BDF (unifont 2atest, 3 glyphs) | 105.50 µs | 31.57 µs | 31.43 µs | 3.34× faster | 3.36× faster | yes / yes |
| `glyph_lookup` (small): font.glyph('永') / native glyph_for_codepoint | 3.17 µs | 7.28 µs | 193 ns | 0.44× slower | 16.46× faster | yes / yes |
| `glyphbycp` (small): font.glyphbycp(0x6C38) / native glyph_for_codepoint | 2.22 µs | 6.19 µs | 190 ns | 0.36× slower | 11.70× faster | yes / yes |
| `attr_font_header` (small): font.headers['fbbx'] / native font.metrics['bounds']['width'] | 77 ns | 3.33 µs | 1.71 µs | 0.023× slower | 0.045× slower | yes / yes |
| `attr_font_prop` (small): font.props['font_ascent'] / native font.properties.get('FONT_ASCENT') | 83 ns | 5.73 µs | 348 ns | 0.014× slower | 0.24× slower | yes / yes |
| `attr_glyph_meta` (small): glyph.meta['dwx0'] / native glyph.metrics['advance']['x'] | 74 ns | 79 ns | 742 ns | 0.93× about equal | 0.099× slower | yes / yes |
| `attr_glyph_cp` (small): glyph.cp() / native glyph.codepoints[0].value | 76 ns | 84 ns | 222 ns | 0.91× about equal | 0.34× slower | yes / yes |
| `glyph_draw` (small): glyph.draw() in the font box / native font.glyph_in_cell(id) | 21.73 µs | 10.36 µs | 362 ns | 2.10× faster | 60.00× faster | yes / yes |
| `draw_one_char` (small): font.draw('永') (one-glyph render) | 41.89 µs | 73.5 ms | 1.28 µs | 0.00057× slower | 32.82× faster | yes / yes |
| `bitmap_new_16` (small): Bitmap(16 row strings) / native Bitmap.from_text | 308 ns | 1.63 µs | 1.43 µs | 0.19× slower | 0.22× slower | yes / yes |
| `bitmap_new_64` (small): Bitmap(64 row strings of 64) / native Bitmap.from_text | 304 ns | 7.66 µs | 11.51 µs | 0.04× slower | 0.026× slower | yes / yes |
| `bitmap_clone` (small): b.clone() / native b.copy() (64×64) | 619 ns | 282 ns | 226 ns | 2.20× faster | 2.74× faster | yes / yes |
| `bitmap_crop` (small): b.clone().crop(31, 31, 3, 5) / native b.crop(...) (64×64) | 20.28 µs | 770 ns | 1.06 µs | 26.32× faster | 19.13× faster | yes / yes |
| `bitmap_concat` (small): b.clone().concat(o) right/bottom / native b.concat(o, "right", "end") | 155.16 µs | 2.27 µs | 2.09 µs | 68.40× faster | 74.20× faster | yes / yes |
| `bitmap_shift` (small): b.clone().shift(3, -2) / native b.shift(3, -2) (64×64) | 53.71 µs | 1.58 µs | 1.79 µs | 34.05× faster | 29.97× faster | yes / yes |
| `bitmap_glow` (small): b.clone().glow() / native gray8 b.glow(False, 1, 2) (64×64) | 2.9 ms | 3.24 µs | 34.18 µs | 882.88× faster | 83.81× faster | yes / yes |
| `todata_0` (small): b.todata(0) / native to_text({0:"0",1:"1"}) (64×64) | 1.11 µs | 11.58 µs | 20.56 µs | 0.095× slower | 0.054× slower | yes / yes |
| `todata_1` (small): b.todata(1) / native to_text(...).split("\n") (64×64) | 312 ns | 17.41 µs | 26.42 µs | 0.018× slower | 0.012× slower | yes / yes |
| `todata_2` (small): b.todata(2) / native to_rows() (64×64) | 400.48 µs | 158.00 µs | 27.94 µs | 2.53× faster | 14.33× faster | yes / yes |
| `todata_3` (small): b.todata(3) / native to_list() (64×64) | 447.02 µs | 144.19 µs | 28.39 µs | 3.10× faster | 15.74× faster | yes / yes |
| `todata_4` (small): b.todata(4) / native to_numeric_hex_rows() (64×64) | 34.88 µs | 14.19 µs | 6.39 µs | 2.46× faster | 5.46× faster | yes / yes |
| `todata_5` (small): b.todata(5) / native to_row_values() (64×64) | 12.80 µs | 59.57 µs | 1.20 µs | 0.21× slower | 10.68× faster | yes / yes |
| `getpixel` (small): b.getpixel(10, 20) / native b.get_pixel(10, 20) | 545 ns | 106 ns | 106 ns | 5.12× faster | 5.14× faster | yes / yes |

- `parse_latin` native: spread min 4.2 ms / max 7.2 ms
- `write_bdf` native: spread min 36.5 ms / max 49.2 ms
- `write_ttf` ref: spread min 14.55 s / max 43.88 s
- `write_ttf` compat: spread min 1.28 s / max 1.64 s
- `write_ttf` native: spread min 554.1 ms / max 957.1 ms
- `draw_2000_repeat` ref: spread min 81.4 ms / max 149.9 ms
- `draw_2000_repeat` compat: spread min 92.4 ms / max 126.8 ms
- `draw_2000_repeat` native: spread min 1.1 ms / max 2.4 ms
- `draw_2000_mixed` ref: spread min 2.20 s / max 3.22 s
- `draw_2000_mixed` compat: spread min 244.0 ms / max 420.9 ms
- `draw_2000_mixed` native: spread min 2.6 ms / max 7.6 ms
- `drawall_mode1` ref: spread min 12.94 s / max 22.26 s
- `drawall_mode1` compat: spread min 113.1 ms / max 219.1 ms
- `drawall_mode1` native: spread min 19.1 ms / max 30.4 ms
- `drawall_mode0_default` ref: spread min 1.49 s / max 3.00 s
- `drawall_mode0_default` compat: error: fontpixel.compat.bdfparser.errors.ResourceLimitValueError: bitmap pixels exceeds configured limit 16777216
- `drawall_mode0_default` native: spread min 17.2 ms / max 32.9 ms
- `bitmap_clone` compat: spread min 265 ns / max 416 ns
- `bitmap_crop` ref: spread min 12.35 µs / max 23.40 µs
- `bitmap_crop` compat: spread min 473 ns / max 1.04 µs
- `bitmap_crop` native: spread min 628 ns / max 1.41 µs
- `bitmap_concat` native: spread min 1.97 µs / max 2.68 µs
- `bitmap_shift` ref: spread min 46.06 µs / max 57.88 µs
- `bitmap_shift` native: spread min 1.65 µs / max 2.21 µs
- `bitmap_glow` ref: spread min 2.2 ms / max 3.2 ms
- `bitmap_glow` compat: spread min 2.56 µs / max 3.66 µs
- `bitmap_glow` native: spread min 27.90 µs / max 37.67 µs
- `todata_0` compat: spread min 11.12 µs / max 14.60 µs
- `todata_0` native: spread min 20.33 µs / max 28.79 µs
- `todata_1` ref: spread min 171 ns / max 391 ns
- `todata_1` compat: spread min 9.26 µs / max 24.14 µs
- `todata_1` native: spread min 15.19 µs / max 32.74 µs

load average (1 min) at child start/end: min 2.91, median 3.53, max 19.13

## Before the compatibility fixes: speedup across runs 1 and 2

"run 1 / run 2" are ratios of medians. "Paired range" is the min–max of
bdfparser time ÷ fontpixel time over the 10 repetitions; in each repetition the
two engines ran back to back.

| Case | compat vs ref (run 1 / run 2) | paired range | native vs ref (run 1 / run 2) | paired range | verdict compat / native |
| --- | --- | --- | --- | --- | --- |
| `parse_tong_sc` | 4.69× / 4.68× | 4.18–4.83× | 9.13× / 9.32× | 8.64–10.76× | faster / faster |
| `parse_latin` | 5.13× / 4.73× | 2.97–5.82× | 11.10× / 10.45× | 6.26–16.38× | faster / faster |
| `write_bdf` | 0.95× / 0.92× | 0.89–1.00× | 1.82× / 1.77× | 1.74–2.01× | about equal / faster |
| `write_pcf` | 7.09× / 5.36× | 3.60–7.84× | 21.48× / 19.48× | 11.38–22.93× | faster / faster |
| `write_ttf` | 11.39× / 12.54× | 4.08–30.07× | 26.68× / 30.24× | 10.05–47.56× | faster / faster |
| `draw_2000_repeat` | 0.85× / 0.91× | 0.65–1.34× | 87.06× / 68.91× | 61.36–129.60× | about equal / faster |
| `draw_2000_mixed` | 9.62× / 8.68× | 7.06–10.96× | 392.60× / 398.96× | 350.79–842.60× | faster / faster |
| `drawall_mode1` | 120.57× / 186.40× | 50.00–242.69× | 703.31× / 1099.73× | 306.37–1487.25× | faster / faster |
| `drawall_mode0_default` | — | — | 90.13× / 82.66× | 78.50–127.66× | n/a / faster |
| `iterglyphs` | 0.33× / 0.33× | 0.29–0.36× | 13.57× / 13.82× | 7.82–14.63× | slower / faster |
| `iterglyphs_meta` | 0.35× / 0.35× | 0.31–0.37× | 2.18× / 2.24× | 1.86–2.64× | slower / faster |
| `noop` | 0.99× / 1.33× | 0.89–2.20× | 1.00× / 1.17× | 0.84–2.82× | mixed: about equal, faster / about equal |
| `parse_tiny` | 3.34× / 3.40× | 3.15–3.72× | 3.36× / 3.50× | 3.29–3.88× | faster / faster |
| `glyph_lookup` | 0.44× / 0.43× | 0.40–0.46× | 16.46× / 10.15× | 10.03–22.73× | slower / faster |
| `glyphbycp` | 0.36× / 0.38× | 0.33–0.39× | 11.70× / 11.74× | 10.33–12.17× | slower / faster |
| `attr_font_header` | 0.023× / 0.023× | 0.019–0.026× | 0.045× / 0.049× | 0.041–0.053× | slower / slower |
| `attr_font_prop` | 0.014× / 0.013× | 0.011–0.015× | 0.24× / 0.24× | 0.17–0.26× | slower / slower |
| `attr_glyph_meta` | 0.93× / 0.96× | 0.86–1.16× | 0.099× / 0.089× | 0.086–0.11× | about equal / slower |
| `attr_glyph_cp` | 0.91× / 0.93× | 0.84–1.19× | 0.34× / 0.35× | 0.32–0.42× | about equal / slower |
| `glyph_draw` | 2.10× / 2.00× | 1.86–2.46× | 60.00× / 63.12× | 52.22–71.27× | faster / faster |
| `draw_one_char` | 0.00057× / 0.00059× | 0.00056–0.00067× | 32.82× / 16.27× | 16.01–42.89× | slower / faster |
| `bitmap_new_16` | 0.19× / 0.18× | 0.13–0.21× | 0.22× / 0.21× | 0.16–0.24× | slower / slower |
| `bitmap_new_64` | 0.04× / 0.039× | 0.033–0.046× | 0.026× / 0.023× | 0.023–0.031× | slower / slower |
| `bitmap_clone` | 2.20× / 2.38× | 1.47–4.89× | 2.74× / 2.95× | 2.60–5.54× | faster / faster |
| `bitmap_crop` | 26.32× / 27.32× | 15.17–30.91× | 19.13× / 20.10× | 5.74–21.74× | faster / faster |
| `bitmap_concat` | 68.40× / 68.20× | 55.79–76.00× | 74.20× / 74.04× | 59.61–87.70× | faster / faster |
| `bitmap_shift` | 34.05× / 36.08× | 29.36–53.61× | 29.97× / 28.62× | 25.80–83.88× | faster / faster |
| `bitmap_glow` | 882.88× / 832.38× | 325.67–989.09× | 83.81× / 82.89× | 66.23–97.99× | faster / faster |
| `todata_0` | 0.095× / 0.091× | 0.044–0.10× | 0.054× / 0.049× | 0.041–0.054× | slower / slower |
| `todata_1` | 0.018× / 0.017× | 0.015–0.019× | 0.012× / 0.011× | 0.01–0.013× | slower / slower |
| `todata_2` | 2.53× / 2.50× | 2.44–2.60× | 14.33× / 14.21× | 12.30–14.97× | faster / faster |
| `todata_3` | 3.10× / 3.12× | 3.00–3.46× | 15.74× / 14.70× | 14.37–15.78× | faster / faster |
| `todata_4` | 2.46× / 2.52× | 2.27–2.70× | 5.46× / 5.46× | 4.93–6.13× | faster / faster |
| `todata_5` | 0.21× / 0.21× | 0.20–0.22× | 10.68× / 10.50× | 10.15–11.05× | slower / faster |
| `getpixel` | 5.12× / 5.03× | 4.53–5.63× | 5.14× / 5.13× | 4.38–5.48× | faster / faster |

## Why the remaining slow cases are slow

The binding overhead itself is small. A trivial native method (`get_pixel`)
costs about 110 ns per call. That includes the ~51 ns loop-and-closure baseline
(`noop`), so the crossing into Rust is about 50–60 ns. What remains slower is
Python-object work done per call:

1. **`font.headers[...]` and `font.props[...]` (compat 0.16–0.19×, about 430 ns
   against 70 ns).** The projection itself is cached per edit revision since
   `a97b916`, so a read no longer rebuilds the dict from Rust (it did, at
   3–6 µs). What is left is pure Python: the `headers` property, the
   `MutableMapping` view's `__getitem__`, the revision check in `_state()` and
   a defensive copy of list values. bdfparser reads a plain `dict`. Callers that
   read in loops can take `h = dict(font.headers)` once.
2. **Per-glyph metadata (`iterglyphs` while reading `cp()` and `meta['dwx0']`
   0.41–0.45×; single `glyph.meta[...]`/`glyph.cp()` reads 0.69–0.77×).**
   bdfparser glyphs hold their meta list already. A compat glyph builds its
   `meta` dict from the shared native snapshot on first access
   (`RawGlyph.meta_dict`), and `cp()` goes through that dict, so iterating
   all 38,653 glyphs and touching each one's metadata pays one dict build per
   glyph. Iterating without reading metadata is as fast as bdfparser
   (1.02–1.12×), and the native API is 2.2–2.4× faster for the same metadata
   walk. Single cached reads cost 25–30 ns more than bdfparser (an extra
   attribute lookup in the compat wrapper).
3. **On the native side, `font.metrics`, `glyph.metrics` and
   `glyph.codepoints` build a new dict or list on every access** (0.2–1.5 µs).
   They return plain data by value, as the binding contract specifies;
   callers keep the result instead of re-reading it.
4. **Building bitmaps from strings, and `todata(0)`/`todata(1)`.** bdfparser
   keeps the row strings it is given and returns them unchanged; `todata(1)`
   returns its internal list in 0.2 µs. Both fontpixel APIs parse the rows into
   a raster when a bitmap is built (1.5–11 µs for 64×64) and rebuild the
   strings on output (7–18 µs). Code that builds many bitmaps from strings and
   reads them straight back as strings loses. Code that transforms them in
   between wins: one `concat` or `glow` saves more time than many constructions
   cost.
5. **`tobdf()` (0.88–0.91×).** bdfparser's BDF writer mostly joins strings it
   already holds. The compat layer regenerates 5.7 MB of text in Rust and copies
   it into a Python `str`. The native writer returns bytes and is 1.8–1.9×
   faster.
6. **Native gray8 `glow` takes 21 µs, against 2 µs for compat `glow`**, on the
   same 64×64 input. It is still 71–90× faster than bdfparser; it is listed
   only because the two fontpixel paths differ by 10×. The native call works
   on a gray8 raster and the compat call on bdfparser's label rows; the gap
   has not been profiled.

## manylinux wheels

`scripts/build_wheels.sh` builds release abi3 wheels with
`maturin build --release --locked --zig --compatibility manylinux2014`, using
maturin 1.15.0, ziglang 0.16.0 and rustc 1.99.0, without Docker. It then:

- runs `auditwheel show` (6.8.2);
- checks the wheel members: exactly one extension module and no bytecode;
- installs the host wheel into fresh CPython 3.10.20 and 3.12.13 venvs;
- runs the pytest suite from the repository against the installed wheel. A
  pytest plugin aborts the run if `fontpixel` or `fontpixel._native` is imported
  from anywhere other than the venv's site-packages.

The wheels are in `local-work/wheels/`. Nothing was published.

| Wheel | Bytes | SHA-256 |
| --- | ---: | --- |
| `fontpixel-0.1.0-cp310-abi3-manylinux_2_17_x86_64.manylinux2014_x86_64.whl` | 10,010,425 | `f155596b87fa7b12f8efae462a494aff9a32c6429749d3160568a13103ec192b` |
| `fontpixel-0.1.0-cp310-abi3-manylinux_2_17_aarch64.manylinux2014_aarch64.whl` | 9,421,335 | `055ee1984da0eb2a91a89a33a4daa3b76d1f3383fb8a38846c6db52f1bba93e6` |

`auditwheel show` reported:

- x86_64: "consistent with the following platform tag: manylinux_2_17_x86_64".
  External versioned symbols appear only in the system libraries libc.so.6
  (GLIBC_2.2.5 to 2.17), libm.so.6, libpthread.so.0 and libdl.so.2. No other
  external shared libraries are needed.
- aarch64: "consistent with the following platform tag: manylinux_2_17_aarch64";
  libc, libm, libpthread and libdl at GLIBC_2.17; "The wheel requires no
  external shared libraries!"

Tests of the installed x86_64 wheel:

| Interpreter | Result |
| --- | --- |
| CPython 3.10.20 | 1227 passed, 1 skipped |
| CPython 3.12.13 | 1227 passed, 1 skipped |

The one skipped test is `test_formats_generators.py:65` ("no rustdoc inventory;
run tools/bindings/gate.sh"). It skips whenever the rustdoc JSON inventory has
not been generated by `tools/bindings/gate.sh`; a source-tree run without the
inventory skips it in the same way. It is not a limitation of the wheel.

Cross-compiling for aarch64 with zig needed only the Rust standard library for
that target (`rustup target add aarch64-unknown-linux-gnu`); no C cross
toolchain, sysroot or Docker. The wheel's extension is an "ELF 64-bit LSB shared
object, ARM aarch64". It was **not run**: this machine has neither aarch64
hardware nor qemu-user. Its tests still need an aarch64 host.

The first build packaged `__pycache__/*.pyc` files that an earlier
`maturin develop` run had left under `crates/fontpixel-py/python/`, because
maturin packages everything in the Python source directory. The script now
refuses to build while such bytecode exists and checks every built wheel for it.
Adding `exclude = ["**/__pycache__/**", "**/*.pyc"]` to `[tool.maturin]` would
also fix it at the source. That is a packaging change I have not made.
