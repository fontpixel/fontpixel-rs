# Processing PFont VLW

## Native fields and evidence (recorded before implementation)

Processing's [loadFont documentation](https://processing.org/reference/loadFont_)
and original [PFont field reader/writer](https://github.com/processing/processing4/blob/main/core/src/processing/core/PFont.java)
define the binary format. The implementation here is original; Processing runs
only as an independent external consumer/producer. No Processing code is copied.

Six big-endian signed i32 header fields contain glyph count, version, point/pixel
size, old mbox field, ascent and descent. Each glyph directory record has seven
i32 fields: Unicode value, height, width, advance, top extent, left extent and
an obsolete pointer field. Records precede all raster payloads. Each payload is
width*height unsigned coverage bytes, row-major, 0 clear and 255 ink. The model
uses Gray8 with x=leftExtent and y=topExtent-height. Obsolete fields are retained
as native properties; negative sizes, invalid scalars, duplicate mappings and
noncanonical unsorted directories are rejected rather than passed to an unsafe
external loader.

Versions 8 and 9 end after bitmaps. Versions 10 and 11 append font name and
PostScript name using Java DataInput/DataOutput modified UTF-8: a u16 byte count,
NUL encoded C0 80 and supplementary characters encoded as two three-byte UTF-16
surrogates. Version 11 then appends one boolean smooth byte. Malformed/overlong
sequences and unpaired surrogates are rejected. The native loader derives missing
ascent/descent from d/p glyphs before rejecting a wholly missing baseline; this
behavior is reflected explicitly. The writer defaults to version 11.

## Limits and loss

Counts, dimensions, sums, signed metric conversion, string lengths and payload
ends are checked before allocation. Model source, directory, raster, modified
UTF scratch, output, diagnostics and writer readback share cumulative budgets.
Version downgrades diagnose lost names/smooth state; unsupported metadata,
legacy mappings, sequences and glyph associations are reported by native
readback. Mono-to-Gray8 projection is explicit in the readback model. Strict mode
rejects every Loss. All variants emit real VLW; no text wrapper is used.

The original fixtures cover binary and multilevel coverage, bearings, blank
advances, modified UTF including NUL/supplementary characters and legacy versions.
Processing PFont load/save validates every stored glyph and metric directly.
The Processing text API uses UTF-16 characters; stored supplementary values are
verified through indexed glyph access, without claiming text rendering support.
