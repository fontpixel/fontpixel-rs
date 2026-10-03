# SVG images and SVG 1.1 fonts

## Authority and scope (W2.8)

- W3C SVG 1.1 Second Edition, [fonts](https://www.w3.org/TR/SVG11/fonts.html)
  and [path data](https://www.w3.org/TR/SVG11/paths.html).
- Supplied `reference/bdfparser/docs/SPEC-v3.md` §7.3 and its Python serializer.
  The existing compatibility serializer stays byte-exact. Native output uses the
  same coordinate convention and merged-square contour engine, with typed options.
- SVG-font import consumes `font` / `font-face` / path-based `glyph` and
  `missing-glyph` elements. It does not execute scripts, resolve URLs or load fonts.
  SVG images are output-only; they are not misidentified as fonts.

## Mapping

| SVG field | Model |
|---|---|
| font-face units-per-em | native pixel size × units per pixel |
| ascent / descent | baseline metrics, descent normalized to positive distance |
| font-family / font-style | family / SVG style descriptor; native fp:style retains free-form style names |
| horiz-adv-x | effective horizontal device advance × scale |
| vert-adv-y | negative of upward-facing vertical advance y × scale |
| vert-origin-x/y | vertical origin relative to horizontal origin |
| unicode | one scalar, or a Unicode sequence for ligatures |
| glyph-name | glyph name |
| d | upward-facing outline; nonzero-winding exact pixel-center fill |
| font horiz-origin-x/y | subtracted from glyph design coordinates |

The native namespace `https://fontpixel.org/ns/svg/1` carries pixel size, original
bitmap rectangles (including blank padding), optional metrics/names, and a local
glyph identity joining repeated Unicode alias/sequence records. It never stores a
second bitmap: the visible path is the raster source. Generated aliases and the
missing-glyph record share geometry and metrics; inconsistent identity groups are
rejected. Other SVG consumers can ignore the extension and still draw the font.

Native square output is deterministic and preserves unencoded/blank glyphs,
Unicode aliases/sequences, negative bearings and effective horizontal/vertical
metrics. Square-separate and circular images/fonts are explicit options. The
native reader uses the stored pixel size; generic fonts require all geometry on
the coarsest integral em divisor with ppem in 4..64, or an explicit ppem override.
Grid detection examines every endpoint/control. Curves with an explicit ppem are
filled without hinting or antialiasing; elliptical arcs are converted to cubics by
svgtypes. Tight generic bounds derive from the control hull and are then trimmed.

## Parsing, budgets, unsupported content

XML uses roxmltree with DTD disabled and a node budget. Input must be UTF-8, with
bounded bytes, nodes, attributes, text, fonts, glyphs, paths, aggregate edges and
aggregate raster pixels. XML entities are limited to the five predefined entities
and numeric character references; no external entities or resource loader exist.
Path commands use svgtypes and the shared bounded outline rasterizer. Nonfinite
or excessive coordinates are rejected. A typed output-byte budget prevents large
in-memory SVG serialization. Input and writer limits can be lowered by callers.

Unsupported graphical glyph children, transforms, CSS, contextual glyph selectors,
paint effects and nonzero-origin ambiguity are rejected or reported explicitly,
never silently rendered as different ink. Kerning and descriptive/unhandled
metadata are reported as conversion loss. General SVG document rendering and
external font-face references are not implemented. Namespace matching is exact;
unqualified legacy SVG is accepted deliberately, but foreign-namespace lookalikes
are not fonts. XML 1.0 forbidden characters cannot be emitted as numeric entities;
unsupported mappings are reported, and invalid metadata strings are errors.

## Loss and compatibility

Original source-format properties, layout tables, raw headers, charset encodings,
scalable/ABC/image-relative metrics and metadata without an SVG/native mapping
produce structured loss diagnostics. Grayscale requires an explicit threshold.
A requested ppem rescale can round metrics and reports that projection. Strict
registry/CLI output rejects combined read/write loss before replacing a file.
The SVG 1.1 font feature is historical and unsupported in many modern browsers;
standalone SVG image exports contain paths and need no installed font.

Independent oracle, parser fuzz and visual evidence are recorded in the W2.8
report. No copied GPL implementation is used.

## Oracle and capacity findings

The installed monobit 0.54 SVG adapter is for single-stroke fonts. Its loader
requires several optional font-face numbers and searches a glyph's descendants
for `d`, ignoring a standard `d` attribute on the glyph itself; its writer emits
`fill="none" stroke="currentColor"`. It therefore cannot be a filled-pixel SVG
oracle. The recorded attempt fails on an absent optional underline field. Native
filled paths are instead parsed independently with FontTools and rasterized with
CairoSVG, including a supplied-Python-produced SVG-font import check.

Large CJK separate-square/dot documents can exceed the standard 64 MiB input
budget. CLI `info`, `convert`, `preview` and `render` accept an explicit
`--max-input-bytes 268435456`; the independent 256 MiB aggregate cap remains.
A library caller may explicitly raise `ReadOptions.limits.resources.max_input_bytes`
(and the registry's common resource limit when using registry dispatch). The
crosscheck driver does this explicitly for full Tong CJK dot output. Geometry is
retained for one glyph at a time, with a separate bounded exhaustive pass only
when automatic grid detection is required; input size is not permission to retain
all expanded curves simultaneously. Both passes consume the total edge-work budget.

XML limits are checked in an allocation-free tokenizer pass before DOM parsing,
including namespace declaration count. Arc operand validation bounds radius
correction and the normalizer's subdivision buffer before it is allocated. The
conservative radius bound can reject extreme anisotropic ellipses even if a
particular rotated segment would fit; these are errors, never approximated silently.

## Independent-review corrections

Separate child paths are painted independently and unioned, while subpaths
within one path retain nonzero winding. The cumulative raster-work budget counts
each painted layer. Native alias mappings are collected before indexing, avoiding
quadratic cloning/reindexing; report locations refer to the merged output glyph.
UTF-32 mapping/model allocation is charged before decoding. Fixed native metric
fields do not allocate an unbounded token vector. SMIL animation elements are
rejected as active content, and unknown native glyph attributes report loss.

Free-form style names (for example `Regular` or `Bold Italic`) use `fp:style`.
The standard `font-style` attribute is emitted only for valid SVG descriptors
(`all`, or a comma-separated list of `normal`, `italic`, `oblique`). This preserves
native style names without emitting invalid standard font metadata.

SVG requires nonnegative horizontal advances, including for RTL fonts. Native
writing rejects a negative effective/default horizontal advance rather than
emitting invalid SVG; negative glyph bearings and vertical/horizontal advance-y
remain supported. Pixel centers exactly on a right boundary follow the shared
half-open raster convention. Cairo antialias-alpha ties are not used as the
expected value for that boundary-only case; it has a direct regression instead.
