# Apple Lisa font libraries

Specification recorded before implementation, 2026-10-04. Basis:
[KreativeKorp's Apple Lisa Font Format](https://www.kreativekorp.com/swdownload/lisa/AppleLisaFontFormat.pdf),
including visually inspected directory and compressed-record diagrams. This is
an explicitly empirical description; unknown fields must remain opaque. External
monobit decoding and the document's original consumer are independent oracles.

All words are big endian. font.lib begins with directory length in words after
byte 4 and a u16 entry count. Each entry is a Pascal name padded to an even address
and four u32 word offsets relative to the end of the directory: resource start,
resource end, metrics start, metrics end. Bitmap resource types are 9000/B000 or
9080/B080 compressed. Other printer resources are preserved as opaque entries
with an explicit projection report, never guessed as bitmaps. Per-entry metrics
records are separate truncated NFNT-like records and remain native metadata.

A compressed bitmap begins with type u16, compressed length u32, expanded length
u32 and payload. Expanded length excludes the two-byte type word; its body
buffer length rounds up to eight. Decoding proceeds
backward: each control byte contributes low-bit-first flags, where one emits zero
and zero consumes a preceding literal byte. After expansion, bitmap rows after
the first XOR their predecessor; only the raster is delta-coded. The result is
the native NFNT-like bitmap record. Encoding reverses those operations, supports
both compressed/uncompressed native resources and does not substitute an NFNT
file beneath a fabricated Lisa header. The native consumer decodes in place:
every output write must stay outside the unread compressed prefix. Both total
length and each reverse cursor transition are checked. An explicit `Compressed`
request fails if a record cannot satisfy that constraint; default `Preserve`
uses a native uncompressed record when recompression cannot fit safely.

Native directory order, names, metrics, resource bytes and unknown records are
retained. Writers regenerate checked word offsets after edits and audit all
geometry, mapping, default identity and metadata changes. Expansion size is
validated before allocation, including rounded padding, source and output.
Collections share cumulative resource/glyph/storage budgets. The unrelated
font.heur lookup file is ancillary metadata, not another bitmap font layout.

The public document's original `f.c` appendix was extracted and compiled outside
this repository, unchanged apart from removing PDF form feeds. It independently
expanded original and Rust-generated compressed resources. Valid Lisa records
may omit Macintosh's extra FFFF width-table terminator; monobit 0.54 rejects that
plain fixture. The regression follows the Lisa specification, and a separate
compatibility copy adds only that sentinel when monobit inspects the original
C consumer's output. No change is made to that native output.
