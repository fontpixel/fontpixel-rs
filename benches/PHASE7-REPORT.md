# Phase 7 Python benchmark and manylinux wheels

Status (2026-10-06): measured with the release manylinux wheel built by
`scripts/build_wheels.sh`. Method and case list: [phase7-README.md](phase7-README.md).
Raw data: [results/phase7-python-run1.json](results/phase7-python-run1.json) and
[results/phase7-python-run2.json](results/phase7-python-run2.json).

## Short answer for bdfparser

Measured against pure-Python bdfparser 3.0.0 on CPython 3.10. Ratios are
bdfparser time ÷ fontpixel time, so above 1 means fontpixel is faster. They hold
across both runs (see "Speedup across both runs").

| | through `fontpixel.compat.bdfparser` (drop-in) | through the native API |
| --- | --- | --- |
| **Faster** | parse (4.7× on Tong SC, 3.4× on a 1.2 KB font), PCF 5–7×, TTF 11–13×, `drawall(mode=1)` 120–190×, drawing 2,000 different characters 9×, `glyph.draw()` 2×, bitmap `crop`/`concat`/`shift`/`glow` 26–880×, `clone` 2.3×, `getpixel` 5×, `todata(2/3/4)` 2.5–3× | everything in the left column, and more: drawing text 70–400×, glyph lookup 10–16×, iterating glyphs 14×, the `todata(5)` equivalent 10× |
| **About equal** | `tobdf()` (0.92–0.95×), drawing '永'×2000 (0.85–0.91×, close to the 0.83× "slower" cut-off), cached `glyph.meta[...]` and `glyph.cp()` | — |
| **Slower** | **`font.draw()` of a short text on a large font: about 1,700× slower** (a fixed 70–95 ms per call on Tong SC), `font.glyph()`/`glyphbycp()` 2.3–2.8×, `iterglyphs()` 3×, `font.headers[...]`/`font.props[...]` 40–75×, `Bitmap(rows)` 5–25×, `todata(0)`/`todata(1)` 10–50×, `todata(5)` 4.7× | attribute reads that build a fresh dict or list (`font.metrics` 20×, `glyph.metrics` 10×, `glyph.codepoints` 3×), `Bitmap.from_text` 5–50×, `to_text` 20–100× |
| **Fails** | **`drawall()` with bdfparser's defaults on Tong SC raises `ResourceLimitValueError`**: 22.3 Mpx is over the 16,777,216-pixel default budget, while bdfparser finishes in 1.6–1.9 s | the same render succeeds once `limits.bitmap.max_pixels` is raised (18–23 ms) |

An optional "use fontpixel when installed" switch in bdfparser would be a large
win for whole-font work: loading, writing PCF/TTF, `drawall` and bitmap effects.
It would be no better for writing BDF. It would be a loss for code that draws
short strings, looks glyphs up one at a time, iterates glyph objects, reads
`headers`/`props` in loops, or builds bitmaps from strings. The slow `draw` and
the `drawall` failure are defects in the compatibility layer, not limits of the
approach: the native API on the same core does both in microseconds (see
"Why the slow cases are slow"). They should be fixed before bdfparser offers
fontpixel as an accelerator.

## Machine and load

- CPU: 12th Gen Intel Core i7-12700H (6 performance cores with SMT and 8
  efficiency cores, 20 logical CPUs); 62.5 GiB RAM; Linux 7.0.0-38-generic.
- CPython 3.10.20 (pyenv) for all three engines. bdfparser 3.0.0 runs from a
  copy of `reference/bdfparser/src` (tree SHA-256 `e44e944f…`). fontpixel 0.1.0
  comes from the x86_64 manylinux wheel below (`_native.abi3.so` SHA-256
  `974ba9fe…`), built from Rust and Python sources identical to main `f4eb45b`
  (this branch changes no binding code). Both runs used the harness as committed
  in `5356121`; later commits changed only the `summarize`/`compare` output code.

The machine is a shared desktop. During both runs other work was active:
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

Load inflates absolute times. Process CPU time stayed close to wall time, so
the cause is shared cores, memory bandwidth and clock speed, not descheduling.
For example, bdfparser's 2,000-different-character draw took 0.62 s in a single
check at a calmer moment (1-minute load 2.6), 2.2–3.2 s in run 1 and
0.71–0.75 s in run 2. Read the absolute numbers as indications only. The speedup ratios are stable: each ratio agrees between the
two runs within the spread of the paired per-repetition ratios, and no case
changes verdict between runs except the `noop` baseline, which is noise.

## Run 1 (median of 5 fresh processes per engine)

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

## Speedup across both runs

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

## Why the slow cases are slow

The binding overhead itself is small. A trivial native method (`get_pixel`)
costs about 106 ns per call. That includes the ~51 ns loop-and-closure baseline
(`noop`), so the crossing into Rust is about 50–60 ns. The slow cases come from
avoidable work done on every call:

1. **`compat` `Font.draw`/`drawcps` takes 70–95 ms per call on Tong SC, for any
   text length.** `fontpixel::compat::bdfparser::Font::drawcps`
   (`crates/fontpixel/src/compat/bdfparser/font.rs`, around line 925) clones the
   whole core font (`self.core()`), re-projects the X11 metadata and rewrites
   the DWIDTH metrics of all 38,653 glyphs before every render. A profile puts
   99.7 % of the time inside the one native `RawFont.draw` call. The cost grows
   with font size, not text length, so `draw('永')` takes 73–94 ms against
   42–55 µs in bdfparser. For 2,000 characters the fixed cost is shared out:
   about equal for one repeated character, and 9× faster for 2,000 different
   characters, where bdfparser's per-glyph work dominates. Phase 6 measured the
   same Rust-level cost (91 ms, `benches/REPORT.md`). The native renderer draws
   one character from the parsed font directly in 1.3–3.4 µs. Fix: cache the prepared core font in
   the compat `Font` and invalidate the cache on edits.
2. **`drawall()` with default arguments fails.** The compat layer uses the
   native 16,777,216-pixel bitmap budget. bdfparser's default `drawall()` on Tong
   SC (mode 0, line limit 512) produces 512×43,488 = 22.3 Mpx. bdfparser has no
   such limit, so the drop-in call raises `ResourceLimitValueError` where
   bdfparser succeeds. This difference is not listed in
   `crates/fontpixel-py/tests/compat_bdfparser/KNOWN_DIFFERENCES.md`.
3. **Glyph lookup and iteration (compat 2.3–3× slower).** The compat layer wraps
   every returned `Glyph` eagerly: `Glyph._wrap` → `_pull` copies all 17 meta
   slots, including the hex bitmap rows (`RawGlyph.meta()`), and `font.glyph()`
   makes four native calls (`char2cp`, `glyph` and two `warnings_since`). In a
   profile of `iterglyphs()`, `_pull` plus `meta()` take about 60 % of the time.
   bdfparser returns a view of a list it already holds. The native API returns a
   handle without copying: 0.2–0.4 µs per lookup, 10–16× faster than bdfparser.
4. **`font.headers[...]` and `font.props[...]` (compat 40–75× slower).** Every
   item access rebuilds the whole headers or properties dict from Rust
   (`RawFont.headers()`, 3–6 µs). bdfparser reads a plain Python dict (80 ns).
   `glyph.meta[...]` does not have this problem, because the compat layer caches
   the meta list, and it is as fast as bdfparser. Callers can work around it
   with `h = dict(font.headers)`. A fix would cache the projection per edit
   revision, as `_state()` already does for glyphs. On the native side,
   `font.metrics`, `glyph.metrics` and `glyph.codepoints` also build a new dict
   or list on every access (0.2–1.7 µs).
5. **Building bitmaps from strings, and `todata(0)`/`todata(1)`.** bdfparser
   keeps the row strings it is given and returns them unchanged; `todata(1)`
   returns its internal list in 0.2–0.3 µs. Both fontpixel APIs parse the rows
   into a raster when a bitmap is built (8–15 µs for 64×64) and rebuild the
   strings on output (7–26 µs). Code that builds many bitmaps from strings and
   reads them straight back as strings loses. Code that transforms them in
   between wins: one `concat` or `glow` saves more time than many constructions
   cost.
6. **`todata(5)` (compat 4.7× slower).** The compat core converts each row
   through an arbitrary-precision decimal routine and passes the result through
   `serde_json` (`crates/fontpixel/src/compat/bdfparser/bitmap.rs`, `todata`).
   Every compat `todata` result also goes through a `serde_json::Value` before
   it becomes a Python object. The native `to_row_values()` is 10× faster than
   bdfparser.
7. **`tobdf()` (about equal).** bdfparser's BDF writer mostly joins strings it
   already holds. The compat layer regenerates 5.7 MB of text in Rust and copies
   it into a Python `str`. The native writer returns bytes and is 1.8× faster.
8. **Native gray8 `glow` takes 34 µs, against 3 µs for compat `glow`**, on the
   same 64×64 input. It is still 84× faster than bdfparser; it is listed only
   because the two fontpixel paths differ by 10×.

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
