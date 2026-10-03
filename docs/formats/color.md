# Color bitmap reading (`color`)

Primary sources (2026-10-03): Microsoft OpenType
[CBDT](https://learn.microsoft.com/en-us/typography/opentype/spec/cbdt),
[CBLC](https://learn.microsoft.com/en-us/typography/opentype/spec/cblc), and
[sbix](https://learn.microsoft.com/en-us/typography/opentype/spec/sbix).
PNG decoding uses the MIT/Apache `png` crate, with explicit allocation limits.
Only reader/detector hooks are registered in W2.4. Color writing is W5 work.

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
outline fallback rasterization or color writer is advertised. Pixel, decompression,
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
