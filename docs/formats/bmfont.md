# AngelCode BMFont

Specification before implementation, 2026-10-04:
[AngelCode file-format documentation](https://www.angelcode.com/products/bmfont/doc/file_format.html).
Text and XML descriptions plus binary version 3 are supported with PNG pages.
XML uses a bounded streaming parser. DTDs and custom entities are rejected;
the five predefined entities and numeric character references are decoded.
Binary versions 1 and 2 are rejected explicitly. Pages are resolved only through
`ReadContext`, and their relative names and cumulative decoded size are checked.

Image offsets are preserved as top-left coordinates; y-up origin is
`base - yoffset - height`. Line height and baseline remain independent.
Ordered `bmfont.*` properties retain info/common fields, kerning pairs and
page/rectangle/channel provenance. The writer repacks rectangles deterministically
using shelf packing; original atlas placement changes are reported. Kernings
retain their original code pairs and signed amounts.

Channel masks use blue=1, green=2, red=4, alpha=8. A single selected channel is
sampled exactly. For all-channel glyphs, alpha is used when present and its
common-field role contains glyph data; otherwise glyph-bearing RGB channels are
combined by their maximum value. Channel roles 1/2 include outlines: their
reduction is reported. Color differences and discarded channels are reported;
Gray8 cannot preserve source color. RGB without alpha uses maximum RGB coverage.
Grayscale PNG aliases the alpha channel to its gray samples, matching the
AngelCode/monobit grayscale page convention. Sixteen-bit samples are reduced with
an explicit loss. Ancillary PNG metadata is diagnosed and omitted; compressed
text and ICC profiles are skipped without decompression. Animated PNG is rejected.
The explicit `gray_threshold` option converts coverage to monochrome and always
reports the depth change. Writer pages use grayscale PNG, read back as
Gray8; Mono promotion is reported as a depth change. Unsupported or missing model
fields are diagnosed by comparing the emitted document's model to the input.

Input and output budgets include retained pages, descriptor records, source and
reread models, PNG scratch and report entries. The writer rereads its output with
the same per-resource limits and the remaining aggregate memory allowance.
An independent fuzz assertion also rereads with the caller's original limits and
requires full model equality whenever no loss was reported.

Crosschecks cover monobit 0.54 in both directions for all three encodings.
Its BMFont reader ignores `common.base` and derives baseline from raster height;
the crosscheck asserts this exact known y-offset difference independently while
checking the full raster, x bearing and advance. Rust uses the documented base.
