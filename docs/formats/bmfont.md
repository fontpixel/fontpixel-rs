# AngelCode BMFont

Specification before implementation, 2026-10-04:
[AngelCode file-format documentation](https://www.angelcode.com/products/bmfont/doc/file_format.html).
Text, XML, binary version 3, and Jam3 JSON descriptions are supported with PNG pages.
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

Crosschecks cover monobit 0.54 in both directions for all four encodings.
Its BMFont reader ignores `common.base` and derives baseline from raster height;
the crosscheck asserts this exact known y-offset difference independently while
checking the full raster, x bearing and advance. Rust uses the documented base.

## JSON descriptors (W5.X)

`BmfontEncoding::Json` implements the
[Jam3 BMFont JSON schema](https://github.com/Jam3/load-bmfont/blob/master/json-spec.md),
also emitted and consumed by monobit 0.54.0. `info` and `common` are objects,
`pages` is an ordered array of image-name strings (the index is the page ID),
`chars` is an ordered array of glyph objects, and optional `kernings` is an array
of kerning objects. `info.padding` and `info.spacing` are numeric arrays of four
and two entries respectively. Integer and boolean scalar flags are accepted;
Unicode strings use JSON escaping. This adds a descriptor encoding, not another
font family or a generic JSON-byte wrapper.

The parser checks JSON structure, depth, duplicate keys, record counts and
cumulative allocations before retaining values. Field values are converted to
the same canonical native records used by text/XML/binary, so PNG page handling,
coordinate transforms, channel roles, glyph ordering and kerning semantics
remain shared. Unknown record strings, integers, booleans and flat integer arrays are retained
as native properties. Nulls, fractional numbers, and nested extension objects
are rejected explicitly. All defined schema dimensions and metrics are integer
values; booleans are accepted only for the flag fields. JSON does not permit duplicate object keys
in this implementation. The descriptor may start with a UTF-8 BOM.

The JSON probe checks the required BMFont structure and returns `Possible`;
a leading `{` or matching property name alone never identifies a font. It does
not resolve page resources or construct glyph bitmaps. The writer emits standard
JSON values and ordered page/glyph arrays, uses the same deterministic atlas
packing and complete model audit, and rereads its bundle with identical limits.
All prior text/XML/binary-v3 and PNG behavior remains covered by regressions.

A native record property normally stores each field as `[key, text]`. Unknown
non-string JSON field types use a third text marker (`json:number`, `json:bool`,
`json:numbers`), so a vendor string `"17"` remains distinct from the integer `17`.
Known schema field types have no extra marker; boolean flags normalize to their
numeric semantic value. Unknown info fields survive JSON rewriting with their
types. Atlas repacking can change common/char fields and page placement; the
complete audit reports every omitted or normalized native property. JSON root
extensions must be flat record objects and are retained until an explicitly
reported writer projection. The direct codec trait writer also enforces strict.

The bounded deserializer charges input escape/key buffers, container capacity,
scalar strings, canonical records and sorting scratch cumulatively. Glyph/page
array count limits are checked before deserializing an excess entry. Resource
failures retain the typed `ResourceLimit` error. Output is counted before its
exact vector reservation, and final bundles are reread under the caller's limits.
The allocation-free probe validates the complete JSON terminal boundary and
required root container types, returning only `Possible`.
