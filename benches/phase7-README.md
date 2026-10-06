# Phase 7 Python benchmark: fontpixel bindings versus pure-Python bdfparser

`phase7_python.py` compares three engines on the same workloads, each in a fresh
interpreter process:

| Engine | What runs |
| --- | --- |
| `ref` | pure-Python bdfparser, imported from a writable copy of `reference/bdfparser/src` (the runner refuses `reference/` itself) |
| `compat` | `fontpixel.compat.bdfparser`, the drop-in bdfparser API on the Rust core |
| `native` | the native API: `fontpixel.read`/`write`, `Font`, `Glyph`, `Bitmap` |

The reference never shares a process with `fontpixel`: every worker records the
module file it imported and any foreign `fontpixel`/`bdfparser` modules loaded,
and the summary prints them if there are any.

Results and analysis are in [PHASE7-REPORT.md](PHASE7-REPORT.md); raw data in
`results/phase7-python*.json`.

## Running

```sh
# CPython 3.10 venv with fontpixel installed (maturin develop --release, or the wheel)
cp -r ../reference/bdfparser/src "$WORK/bdfparser-ref-src"      # writable copy
python benches/phase7_python.py run --out "$WORK/phase7-python.json" \
  --ref-src "$WORK/bdfparser-ref-src" --cpus 0-11 --note 'machine state'
python benches/phase7_python.py summarize "$WORK/phase7-python.json"
python benches/phase7_python.py cases          # list the cases
```

`run` starts every measured process as
`systemd-run --user --scope -q -p CPUWeight=10000 -- taskset -c CPUS python -I phase7_python.py worker ENGINE CASE`
(`--no-scope` and `--cpus ''` drop the wrappers; the raw JSON records the prefix).
For each case it runs `--reps` (default 5) repetitions, and within each repetition
the three engines one after another, so drift in machine load hits all engines
alike. `/proc/loadavg` is recorded before and after every process, together with
its wall time. The raw JSON is rewritten after each case, so an interrupted run
keeps its finished cases (`--resume` continues it).

## Measurement contracts

- **whole** (full workloads): one untimed warmup call, `gc.collect()` before and
  after it (outside the timer), then one timed call. Automatic GC inside the call
  is included. The value is that call's wall time; process CPU time is recorded too.
- **small** (single calls): the loop count doubles until one batch takes ≥ 20 ms,
  then is scaled to ~50 ms batches; one warmup batch, then seven timed batches,
  each after a `gc.collect()` outside the timer. The per-process value is the
  median batch time divided by the batch size. The `for` loop and the closure call
  are inside the timer for every engine; the `noop` case measures that baseline
  (roughly 0.1 µs), and it is not subtracted.
- The reported value per engine is the median over the processes; the summary
  also lists min/max when they differ by more than 25 %.
- Every case digests its result outside the timer (SHA-256 of output bytes, of the
  bitmap rows as `0`/`1`/`2` labels, or the value itself). The summary compares the
  compat and native digests with the reference digest; a mismatch is shown as
  "no", never dropped. The native BDF/PCF/TTF writers are fontpixel's own
  encoders, so their bytes are expected to differ from bdfparser's; their rows
  measure a different (complete) encoder, not the same bytes faster.
- All engines run with the same `warnings.simplefilter('ignore')`.
- Mutating bdfparser calls (`crop`, `concat`, `shift`, `glow`) are timed as
  `b.clone().op(...)` so the input stays unchanged, as in phase 6; the native
  calls return new bitmaps. `bitmap_clone` measures the clone alone.

## Cases

Whole workloads use the complete Tong Pixel Sans 14 Large SC BDF (5,746,021 bytes,
38,653 glyphs) unless named otherwise:

| Case | bdfparser call | native call |
| --- | --- | --- |
| `parse_tong_sc`, `parse_latin` | `Font().load_bytes(data)` | `fontpixel.read(data, "bdf")` |
| `write_bdf` / `write_pcf` / `write_ttf` | `tobdf()` / `topcf()` / `tottf()` | `fontpixel.write(fonts, "bdf"/"pcf"/"ttf")` |
| `draw_2000_repeat` | `draw('永'*2000, mode=0, linelimit=80*fbbx)` (phase-6 workload) | `font.draw(text, {"mode": "cell", "line_limit": 2560})` |
| `draw_2000_mixed` | `draw(text, mode=1, linelimit=512)`, 2,000 different characters spread over the font | `font.draw(text, {"mode": "advance", "line_limit": 512})` |
| `drawall_mode1` | `drawall(mode=1)` | `font.draw_all(options={"mode": "advance", "line_limit": 512})` |
| `drawall_mode0_default` | `drawall()` (mode 0, 512×43,488 px) | same with `limits.bitmap.max_pixels = 2**26` |
| `iterglyphs` | `sum(1 for _ in font.iterglyphs())` | `sum(1 for _ in font.glyphs())` |
| `iterglyphs_meta` | `[(g.cp(), g.meta['dwx0']) for g in font.iterglyphs()]` | `(g.codepoints[0].value, g.metrics['advance']['x'])` per glyph |

Small single calls (bitmaps are 64×64 with a fixed pattern unless named otherwise):
`parse_tiny` (1.2 KB BDF), `glyph_lookup`, `glyphbycp`, `attr_font_header`,
`attr_font_prop`, `attr_glyph_meta`, `attr_glyph_cp`, `glyph_draw`,
`draw_one_char`, `bitmap_new_16`, `bitmap_new_64`, `bitmap_clone`, `bitmap_crop`,
`bitmap_concat`, `bitmap_shift`, `bitmap_glow`, `todata_0`…`todata_5`, `getpixel`.
`python benches/phase7_python.py cases` prints the exact call pairs.
