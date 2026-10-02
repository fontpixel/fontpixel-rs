# Text rendering and images (W1.4)

## API and semantics

`Font::draw(text, &RenderOptions)`, `draw_codepoints(iter, options)` and
`draw_all(order, ranges, options)` return `Rendered { bitmap, report }`. The bitmap
is packed Mono with top-left storage. Rendering borrows existing glyphs, records
placements/lines, and uses `Bitmap::composite_region` to copy clipped packed rows
into one destination. No per-glyph bitmap or intermediate line image is allocated.

Defaults match the supplied bdfparser `Font.draw`/`drawcps` implementation and
SPEC-v3 §2.2. In particular:

- `Advance` uses horizontal DWIDTH or vertical DWIDTH1. A glyph's nonzero x is
  preferred to y; a missing glyph-level vector inherits the global vector's x.
  Zero/negative advances are retained. Default missing glyphs use an empty font
  cell with zero advance, independently of the font's default-character metadata
- `Cell` is SPEC mode 0: concatenate complete font bounding cells, not tight
  glyph ink. `Ink` is an explicit native extension for tight nonzero rectangles
- `WritingDirection` permits all 16 independent glyph/line direction pairs.
  `lr/rl/tb/bt` shortcuts expand as in Python. Strict `FromStr` rejects bad CLI
  values; `from_spec` reproduces Python's default/suffix-tolerant behavior
- Line limit accounting uses **cell width + offset**, even for vertical text.
  This intentionally retains Python's historically width-based vertical wrapping
- Current-glyph spacing versus previous-glyph spacing is selectable. Signed
  `letter_spacing` and `line_spacing` extend SPEC's fixed gaps without changing
  zero-gap defaults. Overlapping cells compose ink; later blank paper does not
  erase previous ink
- Input order is not shaped or bidi-reordered. Newlines are ordinary characters.
  A literal newline with no font mapping follows the missing-glyph policy
- Empty output is 0×0; degenerate font cells retain the reference's normalization.
  A line too narrow for its first glyph fails rather than looping indefinitely
- `draw_all` includes effective encoded aliases, omits unencoded glyphs, supports
  ascending/descending/effective-file/reverse-file order and inclusive ranges.
  Duplicate codes select their last effective source occurrence. Its Rust options
  are explicit: pass `mode: Cell` for SPEC drawall defaults; the CLI does this
- Text is mapped through the font's existing exact charset tables; direct Unicode
  aliases in mixed-domain fonts stay usable. Disabled CJK tables return an error.
  Unknown charsets give an identity-fallback warning per draw; compatibility-layer
  warning-once state belongs to W1.5

Missing policies are `Blank`, `Error`, or a borrowed replacement `Glyph`.
Missing/replaced text, clipped actual ink and explicit Gray8 thresholding are
reported as loss. Missing diagnostics are deduplicated by code/glyph/category;
unknown charset and short source-row repairs are not loss. Invalid unchanged BDF
source hex fails before placeholder pixels can be rendered. Edited pixels take
precedence over old source rows. Rendering Gray8 requires an explicit threshold.

Native errors and structured reports are not a completed Python exception/warning
facade. W1.5 remains responsible for that adapter and the full default parity suite.

## Bounds and failure behavior

`RenderLimits` bounds input count, combined placement/line buffers, diagnostic
storage and result dimensions/pixels/bytes independently. Defaults are one million
inputs, 128 MiB layout, 16 MiB diagnostic storage, and `BitmapLimits::default()`
(16 Mi pixels, 64 MiB, maximum dimension 1,048,576). Diagnostics use fallible hash
lookup/growth rather than a quadratic scan; their conservative byte accounting
includes collection growth and owned text. Glyph names in diagnostics are limited
to 200 characters. Budgets describe operation buffers, not process-wide allocator
usage or memory owned by the caller's font. Increasing one budget does not disable
the others. Arithmetic is widened/checked before constructing result dimensions.

`strict` rejects reported loss. CLI combines read and render reports and validates
image output in memory before using its existing atomic-replacement persistence.
Failures leave existing output files intact. Exit 3 is strict loss; 1 is a runtime
failure; clap syntax errors are 2. Preview sends only art to stdout, diagnostics to
stderr, and applies the byte budget to UTF-8 output, including non-ASCII palettes.

## PNG and SVG image export

`to_png(bitmap, &ImageOptions)` is behind the independent `png` feature. It emits
1-bit indexed PNG: background palette index 0, foreground index 1, sRGB, optional
per-index tRNS alpha, filter 0, zlib-compressed scanlines and CRC-protected chunks.
Scale is a positive integer and expands a single packed row at a time. Empty
PNG dimensions are an explicit error. Encoding follows the
[PNG third-edition chunk specification](https://www.w3.org/TR/png-3/#11Chunks).

`to_svg` needs no optional dependency and emits disjoint horizontal ink/paper run
paths with an unscaled viewBox, scaled dimensions and crisp-edge rendering.
Separating paper from ink preserves alpha rather than compositing paper beneath
translucent ink. It uses standard
[SVG path commands](https://www.w3.org/TR/SVG2/paths.html). Typed RGBA colors allow
`#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA` and `transparent`; arbitrary XML/CSS is not
accepted. No external fonts, references, scripts or resources occur in the SVG.
Empty bitmaps return an empty SVG element; PNG instead rejects them.

This native image SVG is visually exact, not the later SPEC contour/dot/SVG-font
serializer. It does not enable the catalogue's `svg` font codec (W2.8) or `image`
atlas reader/writer with sidecar mappings (W3.3). Both remain honestly unimplemented.
Only `png` adds flate2 and crc32fast, already used by the gzip feature; minimal
BDF/PCF production dependencies do not gain an image-processing framework.

## CLI examples

```sh
fontpixel preview font.bdf 'Hello, 世界!' --direction lrtb --chars '#.'
fontpixel preview font.pcf.gz --all --range U+0020-007E --order file --line-limit 640
fontpixel preview font.bdf AB --mode ink --letter-spacing 2 --line-spacing 3
fontpixel render font.bdf 'Hello' -o out.png --scale 4 --color '#174a7e' --background transparent
fontpixel render font.bdf 'Hello' -o out.svg --strict --report json
fontpixel render font.bdf --all -o atlas.png --max-pixels 67108864
```

See `preview --help` / `render --help` for strike selection, ranges, replacement
codes, threshold, explicit output format and all resource budgets. PNG is enabled
by default by the CLI and can be omitted with `--no-default-features` (enable the
font input features required for your use). `--font-index` is zero-based and
mandatory for a multi-strike input; there is no implicit first-strike selection.

## Validation

[W1.4 report](../reports/w1.4.md), [benchmarks](../benches/RENDER-REPORT.md),
[external checks](../tests/crosscheck/README.md) and the checked-in synthetic oracle
record actual executed coverage and the work deferred to later stages.
