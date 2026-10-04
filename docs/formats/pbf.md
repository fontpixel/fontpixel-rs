# Pebble PBF (W3.4)

## Evidence

The supplied MIT catalogue `opf/ingest/pebble.py` and `tests/test_pebble.py`
provide v3 fixture/bitmap/baseline expectations. Google's Apache-2.0
[fontgen.py](https://github.com/google/pebble/blob/main/tools/font/fontgen.py)
provides the header and table wire description for v2/v3, verified against its
actual serialization (some old comments misleadingly say offsets are words;
they are byte offsets and glyph headers are five bytes, not eight). No source
implementation is copied. monobit 0.54.0 has no Pebble reader/writer.

## Layout and mapping

Little-endian common header: version u8, max_height u8, mapping count u16,
wildcard u16. V2 adds hash bucket count u8 and codepoint width u8 (2 or 4).
V3 adds header length u8 (at least 10) and features u8: bit 0 selects u16
instead of u32 glyph offsets, bit 1 selects RLE4. Other bits are rejected.

Each hash bucket is hash u8, count u8 (unsigned, including >127), byte offset
u16 into the mapping table. Each mapping is Unicode codepoint plus a glyph-data
byte offset. Shared glyph offsets represent aliases. Tables must be in bounds,
cover every mapping exactly once, have correct hashes, and have unique scalar
codepoints. Individual glyph records may be shared but cannot overlap.

Glyph header: width u8, height u8, left i8, top i8, advance i8. Raw images are
row-major LSB-first continuous bits, with no row padding. RLE4 changes height to
nibble count; low nibble first, bit 3 ink, low 3 bits run length minus one.
The runtime and catalogue display only complete decoded rows. TUMBLED v1.11's
`TUMBLED_28_BOLD.pbf` contains 7,829 glyph records with nonempty incomplete final
rows. The reader follows that observed rendering convention, discarding residual
pixels with a per-glyph `pbf.read.rle_tail` **loss** diagnostic; strict conversion
therefore refuses to write these repaired fonts. A zero-width glyph with any
RLE pixels is rejected. Writers produce complete standard raw or RLE4
records and deterministic hash tables; RLE4 records must fit 255 units.

No baseline is encoded. As in the catalogue, baseline=max_height; native BBX y
is baseline-top-height, ascent=max_height, descent is measured below baseline.
Untrimmed bitmap rectangles are retained by default for lossless native roundtrips;
optional trimming matches the catalogue importer. No font family is inferred
from a filename. Point size=pixel size at 72 dpi is an explicit conversion
convention, not a source physical measurement.

## Bounds and losses

All tables/offsets, mapping counts, expanded pixels, input/model/output totals
are checked before allocation. Unknown v3 header bytes are retained as native
properties; unknown feature bits fail. Missing wildcard glyph is legal and
reported as a warning, rather than silently manufacturing pixels.

PBF has no names/license, glyph names, sequences, vertical/scalable metrics or
arbitrary properties. Writers report those losses per field/glyph. Legacy codes
are never silently recast as Unicode. Out-of-range wire dimensions/bearings,
overfull hash buckets/tables and impossible RLE4 sizes error rather than truncate.
Version changes preserve pixels/metrics within their representable range. Explicit
grayscale threshold is required for Gray8 input and is reported as loss. Strict
registry output combines read and write diagnostics before returning any bytes.

## V1 runtime evidence (2026-10-04)

Google's Apache-2.0 [text_resources.c](https://github.com/google/pebble/blob/main/src/fw/applib/graphics/text_resources.c)
(`prv_load_font_res`, `prv_load_offset_table`, `prv_get_glyph_data_offset`) and
[text_resources.h](https://github.com/google/pebble/blob/main/src/fw/applib/graphics/text_resources.h)
(`GlyphHeaderDataV1`) establish a materially different layout: after the six-byte
header there is **no hash table**. Each sorted entry is a u16 codepoint followed
by a u16 offset measured in **four-byte words**, relative to the glyph-data area.
The v1 glyph header is width, height, left, top, three padding bytes, advance
(eight bytes; advance is at byte seven). Raw bitmap bits follow. The three
padding bytes are preserved as `pbf.v1_padding` glyph properties when nonzero.
V2/v3 use byte offsets and the five-byte glyph header described above.

Writers reserve four zero bytes at glyph-data offset zero, as Pebble uses a zero
mapping offset to mean missing. Reader compatibility permits a record at offset
zero, used by the supplied catalogue's synthetic fixtures, and reports a runtime
compatibility warning. Writers word-align v1 records and preserve independent
glyph identity even when pixels match; only aliases on the same glyph share an
offset. Unicode aliases are ordered numerically. Unsorted source mapping tables
are accepted with a repair warning, because Pebble's runtime binary-searches them.

V3 extension bytes beyond ten are retained in `pbf.header_extra`. The official
runtime requires exactly ten, so extensions produce a compatibility warning.
Output defaults to v3 raw, automatically chooses 16/32-bit offsets, and retains
these bytes only in v3. Writer options explicitly select v1/v2/v3, raw/RLE4,
offset size, codepoint size, hash count, and max_height. Default max_height is
ascent, then pixel_size; default wildcard is the model's BMP Unicode default
character, a BMP alias of its default glyph, or U+25AF. The selected fallback is
always represented by `default_character` on read even if absent; `default_glyph`
is populated only if it resolves. Missing fallback is a warning, not a fabricated
glyph. RLE cannot preserve a nonzero height with zero width and rejects it.

All runtime source was inspected as format evidence; no source implementation
was copied. Bounds include input, sorting tables, model slots and cmap overhead,
temporary rasters, diagnostics and output capacity before allocation.

Successful writes are preflighted for reads under the **same** `ResourceLimits`.
The output byte cap is the minimum of `max_output_bytes`, `max_input_bytes` and
the available aggregate output budget. A shared allocation-free reader budget
also accounts for alias-heavy tables, glyph slots, raster copies and preserved
fields; a small compressed input is not permission to return unreadable raw
expansion. No second parsed model is allocated merely to validate the output.
