# FontForge SFD bitmap strikes

Specification before implementation, 2026-10-04:
[FontForge SFD documentation](https://fontforge.org/docs/techref/sfdformat.html),
sections Font Header, Outline Character Data, Bitmap Fonts. Only public format
documentation is used; no GPL FontForge implementation is copied.

`SplineFontDB` identifies the document. `BitmapFont` records describe pixel
size, possible glyph slots, ascent, descent and sample depth (1/2/4/8).
`BDFChar` identifies a source glyph slot and local encoding, advance, inclusive
xmin/xmax/ymin/ymax, and optional vertical advance. Raster rows are top-down,
MSB first, byte aligned and encoded as Adobe ASCII85, including `z` zero groups.
The original outline `Encoding` records supply Unicode identity and glyph names.
Multiple bitmap strikes become multiple Fonts in source order. Outlines and
unsupported instructions are explicitly reported as loss, never rasterized or
executed. Bitmap composite references are fatal unsupported input: no partial
font is returned. BDF properties retain their native type tags. Outline Width
and VWidth are retained separately from bitmap advances.

The deterministic writer emits bitmap-only SFD with identity outline stubs,
ASCII85 data and no timestamps. All model differences on reread are reported,
including native defaults that replace absent values. Input records, expanded
rasters, strike/glyph counts and output size are bounded.
Native depths 2 and 4 survive when all Gray8 samples match their quantization;
otherwise the writer uses depth 8 and reports the metadata change. UnicodeFull
output reserves all 1,114,112 encoding positions, as required by FontForge.
Writer names must currently be single-line ASCII; unsupported names fail instead
of being silently altered. Unknown header records remain ordered properties on
read and their omission on write is reported. The writer does not regenerate
outline geometry, hints, lookups, localized name tables or bitmap composites.

The prose documentation lists the BDFChar bounds in a different order. Black-box
Bits’N’Picas 2.2.2 and the official FontForge 20230101 binary independently
establish the wire order as xmin, xmax, ymin, ymax; an original negative-bearing
fixture distinguishes both interpretations. Crosschecks include both directions,
independent Python depth-2/4/8 raster packing, and a 3,645-glyph BDF sample.
FontForge's BDF importer uses UCS-2, so its reverse BDF fixture is BMP-only;
the forward SFD fixture independently includes U+20000.
