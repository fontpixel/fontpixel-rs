# Color bitmap reading and writing (`color`)

Primary sources (2026-10-03): Microsoft OpenType
[CBDT](https://learn.microsoft.com/en-us/typography/opentype/spec/cbdt),
[CBLC](https://learn.microsoft.com/en-us/typography/opentype/spec/cblc), and
[sbix](https://learn.microsoft.com/en-us/typography/opentype/spec/sbix).
PNG framing follows the [W3C PNG specification](https://www.w3.org/TR/png-3/).
Stored DEFLATE and zlib framing follow [RFC 1951](https://www.rfc-editor.org/rfc/rfc1951)
and [RFC 1950](https://www.rfc-editor.org/rfc/rfc1950). The writer is original code
from these specifications; PNG decoding uses the MIT/Apache `png` crate, with
explicit allocation limits.
Reader/detector hooks were registered in W2.4; W5.6 adds native writers.

CBDT/CBLC version 3 uses the embedded locator layer, including formats 17, 18 and
19 (PNG with small, big or shared metrics), and ordinary EBDT image forms.
Depth-32 raw images contain premultiplied BGRA. PNG dimensions must equal the
metrics. The codec validates PNG framing, allowed CBDT chunks, and bounded output.

sbix version 1 supplies strike ppem/PPI, glyph offsets, signed graphic origin,
format tag and data. PNG and bounded `dupe` references are supported; duplicate
cycles and unavailable targets error. JPEG/TIFF/PDF/mask are explicit unsupported
errors. Offsets cannot overlap headers or cross their enclosing strike.
Absent glyph images remain absent rather than fabricated transparent bitmaps.
The placement API keeps bitmap origin offsets. Native projection accounts for
nonempty glyf bounding-box origins as required by OpenType; unsupported outline
origin sources error rather than silently displace glyphs. Advances come from
hmtx, scaled and rounded to pixels with any inexact rounding reported. sbix's
optional draw-outlines flag is reported as loss because this codec extracts
bitmaps rather than compositing outlines.

The native model has no RGBA type. Default read projects alpha to Gray8 coverage,
so opaque white/colored artwork still has ink. An explicit alternate rule uses
alpha-weighted darkness over white with integer luminance weights 77/150/29.
An optional explicit threshold converts coverage to Mono. Every actual color
projection and threshold is a loss diagnostic; no RGB channel is silently
presented as a preserved color image. Low-level APIs expose decoded bitmap
coverage and placement; original sfnt bytes remain available through `Sfnt`.

No color profile transformation, JPEG/TIFF fallback, SVG/COLR rendering, animation,
outline fallback rasterization is advertised. Pixel, decompression,
table, strike, glyph, graph-visit and whole-font memory/work budgets apply to all
paths, including aliases and empty images.

## Verified duplicate/composite convention

The supplied sbix `dupe` image inherits the referenced image record's origin
(offsets in the duplicate header are ignored), as verified with FreeType 2.13.3.
The current glyph's nonempty glyf bounds are then applied independently. PNG
support in freetype-py's bundled shared library is absent in the task environment;
the independent script uses the installed system library instead.
BGRA composite darkness projection errors explicitly: alpha-only OR composition
is supported, but correct RGB composition needs a future RGBA native model.

## W5.6 canonical writing (specifications checked 2026-10-04)

`ColorWriteOptions.flavor` chooses `Cbdt` (the default) or `Sbix`. Both accept
Mono and Gray8 coverage, and encode black pixels with the original coverage in
the PNG alpha channel. Thus both supported read projection rules return identical
coverage. Gray8 is exact; Mono expands to Gray8 and produces a representation-loss
diagnostic. This does not reconstruct RGB information lost on an earlier read.
The shared registry accumulates these diagnostics and enforces strict output.

CBDT/CBLC uses version 3, depth 32, index 1 and PNG image 17/18, preserving small
and big horizontal/vertical metrics. Zero-area glyphs use the permitted raw image
1/6 representation, because PNG prohibits zero width/height. Sparse gaps remain
absent. Bitmap dimensions/advance use uint8, bearings use int8, and ppem is uint8;
unrepresentable values error. Global sfnt metadata follows the OTB convention.

sbix uses version 1, flags 1 (no outline overlay), sorted ppem/PPI strikes and
PNG records with signed int16 left/bottom offsets. Glyphs use the common cmap
and glyph order, including aliases and unencoded glyphs. No outlines are created.
Non-printing zero-area images have zero-length records as required by the sbix
specification; loss is reported for any source glyph rectangle/membership which
cannot survive that absence. Nonempty transparent images remain explicit PNGs.
PPI comes from a square positive source resolution or defaults to 72; non-square
resolution cannot be represented exactly and is diagnosed. Horizontal advances
are shared design units: the first available strike establishes each glyph's
advance and later strikes are checked against the resulting rounded value, with
any change reported. Vertical metrics unavailable in native sbix extraction are
diagnosed. Duplicate ppem/PPI strikes error, rather than silently overwrite.

The writer bounds glyph/index work, PNG scratch, decompressed images, encoded
payloads, common tables, output and native readback. Every successful native
color write is readable with the identical limits. Encoding is deterministic;
no timestamp, system font, subprocess or external encoder participates. PNGs are
encoded with stored DEFLATE blocks: predictable linear work and exact bounded
scratch, appropriate for small bitmap glyphs. The independent validation script
uses fontTools and system FreeType as oracles, not as production dependencies.


W5.6 independent verification used fontTools 4.66.1, Pillow 12.3.0 and system
FreeType 2.13.2. `tests/crosscheck/w56.py` checks every bitmap sample, Unicode
mapping, origin and horizontal advance across both families, vertical CBDT big
metrics, missing glyphs, empty records, transparency and a 300 × 256 sbix PNG at
512 ppem spanning multiple stored DEFLATE blocks. It also recompiles the output
with fontTools and compares the complete decoded native model. The bundled
freetype-py library lacks PNG; the script selects the system library in-process
without changing the Python environment. Generated fixtures are project-authored
MIT test data; hashes and tool versions are in `reports/w5.6-color/crosscheck.json`.
