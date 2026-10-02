# YAFF (W1.6)

## Sources and scope

- Rob Hagemans, [YAFF specification 1.0.4](https://github.com/robhagemans/monobit/blob/master/YAFF.md), consulted 2026-10-02.
- monobit 0.54.0, `storage/fontformats/text/yaff.py` (MIT), independent executable crosscheck. No upstream implementation is copied.
- PLAN §4, §9.1/9.3/9.6 and W1.6: deterministic, readable test-golden interchange.

UTF-8 (optional BOM), LF/CRLF/CR, inline and indented multiline properties,
comments, repeated labels, Unicode/grapheme/legacy/tag labels, per-glyph and
global metrics, `---` multi-strike sections, and 2/4/16/256 gray levels are read.
The writer emits LF, a `yaff: 1.0` signature, four-space indentation, stable
field and glyph order, ordinary YAFF pixels, and no dates or host paths.

## Field mapping

| YAFF | Native model |
|---|---|
| `u+0041`, `'A'` | Unicode codepoint |
| `u+0066, u+0069`, `'fi'` | Unicode sequence |
| `0x41`, decimal/octal, byte lists | Legacy codepoint, big-endian concatenation |
| `"name"` | Glyph name; additional tags retained as properties |
| left-bearing / shift-up | BBX x / y |
| right-bearing | advance.x − BBX.x − width |
| global plus local bearings | Additive, per specification |
| family/subfamily/name/revision/foundry | Corresponding font names |
| copyright/notice | Copyright/license |
| pixel-size/point-size/dpi/ascent/descent/leading/line-height | Corresponding strike metrics |
| underline-descent / underline-thickness | Negated underline position / thickness |
| encoding / default-char | Charset name / default code or tagged glyph |
| levels 4/16/256 | Gray8 samples scaled exactly to 0–255 |
| unknown fields and comments | Ordered text properties (no silent discard) |

Vertical, kerning and scalable properties without an exact native semantic mapping are
retained as ordered properties. They are not applied by the native renderer.
`multi-cell` retains its original property text and maps to native CharacterCell.
Path-only glyphs are rejected: rasterizing strokes belongs to a later phase.

## Lossless golden extension

YAFF permits namespaced custom properties. The default writer includes
`fontpixel.font`, `fontpixel.glyph`, and (first section only)
`fontpixel.container` JSON-valued properties, with schema version 1. They retain
native metadata not expressible in portable YAFF: optional versus explicit zero
metrics, exact rational representation, all source headers/properties with typed
values and raw bytes, charset details, gray-vs-mono identity, Unicode aliases,
source-format identity, stable glyph IDs and removed slots. Pixel arrays are never
hidden inside JSON: the visible raster is authoritative. The extension enables
full native model round trips while independent readers still consume normal
YAFF labels, rasters, and horizontal metrics.

Portable field edits override corresponding extension metadata, compared against
the extension's derived portable projection. Unchanged projections restore the
original optionality and richer metadata. The extension is versioned, validated,
resource-bounded and never treated as executable instructions. JSON nesting is
bounded; hostile counts/dimensions are checked before allocations. Removing all
extensions yields ordinary interoperable YAFF with its smaller information model.

## Ambiguity and limits

- Strong detection requires the `yaff:` signature. Signatureless label/raster
  structure is weaker evidence; `.yaff`/`.yaffs` remain filename fallbacks.
- Empty explicit input is one empty strike; section boundaries preserve empty
  strikes. A trailing boundary does not invent an additional font.
- Old unquoted tags/characters and offset/tracking aliases are accepted. New
  output always uses quoted tags and explicit Unicode/legacy labels.
- Duplicate labels and properties preserve input order. Multiple same-type
  labels are a widely supported deprecated YAFF feature, necessary for aliases.
- Invalid UTF-8/control/noncharacter input, malformed numerics/ragged rasters,
  bad gray pixels, arithmetic overflow, and malformed extensions are errors.
- Comments are preserved as metadata, but exact original textual positioning,
  whitespace, line endings and quote style are not a byte-for-byte contract.
- Zero-area bitmap dimensions use monobit's sized-empty spelling. Stock monobit
  rewrites zero-area rasters as 0×0; fontpixel retains the dimensions.
- Portable consumers may ignore extension metadata. This is not a loss in a
  fontpixel YAFF round trip; converting it through a third-party writer can lose
  those details. Unknown input properties are retained rather than interpreted.
