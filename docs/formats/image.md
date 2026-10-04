# Image atlas + explicit mapping

## Evidence and field mapping (W3.3, 2026-10-04, recorded before code)

Behavioral source: supplied MIT `reference/fontpixel/pipeline/opf/ingest/spritesheet.py`
and `tests/test_spritesheet.py`. Container coding uses the MIT/Apache `image` crate
with only PNG, BMP, GIF, TGA and PNM codecs enabled. Netpbm P1–P6 are image inputs;
P4 PBM and P5 PGM are output options. No image-format implementation is copied.

A mapping is UTF-8 JSON or TOML. `pixel_size`, `baseline`, `foreground` (RGB list),
`cell_size`, `characters`, `cells`, `omit_blank` match the catalogue importer.
`rect` is [left, top, width, height]; `bearing_x` is horizontal BBX offset;
`baseline - height` is bottom bearing; `advance` defaults to width. Empty char
with a name means an unencoded glyph. One Unicode scalar per char, no surrogate.
The regular layout is row-major. Nontransparent pixels count as ink unless an
explicit foreground RGB palette is supplied. No image resizing or antialiasing.

Native output adds `fontpixel_image = 1`, exact font metadata and per-cell labels,
metrics, properties and depth. It retains aliases, sequences, unencoded glyphs,
charset tags, absent metrics, gray8 bytes and ordered duplicate properties.
The atlas contains actual glyph pixels, not embedded serialized bitmap data.
JSON is default; TOML is selectable. The native extension is versioned and validated.

## Ambiguities and choices

Image input needs an explicit mapping object/path, or a same-stem `.json` sidecar
(then `.toml` only if JSON is absent). No character guessing. Reader also accepts a
mapping as primary input when its `image` field identifies a safe relative image.
Assets resolve only through `ReadContext`; includes cannot escape caller authority.
Regular dimensions must divide BOTH image dimensions. Visible pixels outside any
mapped rectangle, including irregular sheets, are rejected. Overlaps are rejected;
blank unused pixels are allowed. This intentionally tightens the supplied importer.
Transparent colored background pixels never become ink. Native gray8 preserves
zero by opaque black; mono uses black/white. Native mode determines interpretation,
so exported opaque BMP/PNM backgrounds are not mistaken for foreground.
Only a single frame is accepted from animated GIF; multiple frames are rejected
rather than silently discarded. Input metadata is bounded before decode/model
allocation. Image dimensions, decoded bytes, table bytes, glyph pixels, models and
output bytes have independent and aggregate checks. There is no filesystem I/O in
the codec.

## Write losses

One font per atlas; multi-font sets fail instead of selecting a strike silently.
Native sidecars preserve metadata; model slot holes/default IDs must be remapped or
reported. Gray8 through monochrome PBM or binary GIF requires an explicit threshold
and reports every affected glyph. Source container provenance is not font content.
Unsupported metadata must never be silently discarded. Coded colors beyond the
monochrome/grayscale model are reduced according to the requested foreground rule.
