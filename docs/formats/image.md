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

### Native TOML envelope (2026-10-04, before implementation)

Native TOML output uses `fontpixel_image = 1` and `payload_json = "..."`.
The payload is the same complete JSON mapping as native JSON output. This avoids
TOML's signed-integer ceiling and restrictions on heterogeneous arrays of enum
properties; in particular `PropertyValue::Unsigned(u64::MAX)` stays exact.
External catalogue TOML continues to use ordinary grid/cell fields. The envelope
accepts exactly these two keys, requires version 1, and cannot contain another
envelope. Both serializations are streamed into bounded sinks; no intermediate
unbounded TOML tree is constructed. The JSON-string escaping emitted here is also
valid TOML basic-string escaping.

Native TGA output uses uncompressed 8-bit grayscale. `image 0.25.10`'s default
RLE writer emits packets across scanlines; a real 128×304 atlas roundtripped in
Rust but failed Pillow 12.3 with "buffer overrun". The uncompressed variant also
avoids a four-channel scratch copy. Independent Pillow checks cover this choice.

### Aggregate memory accounting

Native reads move glyph metadata directly into stable font slots, avoiding a
second glyph vector. JSON preflight charges vector element sizes by schema key;
raw byte properties are charged as bytes, not as full property enums per sample.
Encoder scratch depends on the selected format, and metadata clones are released
before encoding. Encoder and reader peak budgets are checked separately. This
allows the full 38,653-glyph Tong SC font to roundtrip at the default 256 MiB
aggregate limit. A 4,000-glyph regression also checks a 32 MiB limit with retained
raw bitmap properties. Output sinks cap vector growth before allocating.

All nested native metadata objects reject unknown fields, including shared metrics,
name, charset and property structures. Unknown future fields cannot disappear in a
strict conversion. The same model schema is used by YAFF's native metadata extension.
