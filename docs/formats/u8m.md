# U8/M — UTF-8 for Microcomputers

Canonical extension is `.u8m`. The first 256 bytes contain `U8/M`, a length-prefixed UTF-8 family name (118-byte
capacity), u16 family ID, style and point-size bytes, page addresses and counts
for glyph/map tables, Unicode/native map roots, and byte-sized ascent, descent,
line gap and line height. Table addresses are little-endian u16 pages (256-byte
units). Table entries contain a 24-bit byte address and one count/advance byte.
Map zero is empty and glyph zero is the default glyph.

Each map range is four bytes: first six-bit index, last index, and initial u16
glyph/submap index. Values advance with indices. Ranges are ascending and do not
overlap. Native roots cover four 64-code blocks; low-BMP roots cover 32 blocks;
high-BMP roots add a radix-64 level; six astral roots add two. Low-BMP/high-BMP
roots take priority over broader roots. Traversal expands actual entries at at
most three levels. Glyph records contain signed top-down y/x, height, width and
a contiguous MSB-first pixel bitstream (rows are **not** byte aligned). Records
and map data cannot cross a 256-byte page. Empty bitmap address zero retains
advance. Unicode scalars and native byte positions are distinct model mappings.
A two-byte load-address prefix is supported without changing font-relative offsets.

Primary specification: [author’s U8MFontFormat.pdf](https://github.com/kreativekorp/u8m/blob/master/U8MFontFormat.pdf).
The author’s U8/M project and external Bits’N’Picas are consumers. Aliases and
unencoded glyphs remain model glyphs; Unicode sequences have no native field.

## Model, loss, and resource contract

Each file is one strike. Bitmap coordinates use the model’s baseline and upward
positive y; native top-down coordinates are converted explicitly. Unsupported
names, metadata, Unicode sequences, bearings, vertical metrics, and mapping
changes are diagnosed by comparing the source with a native readback. Strict
writes reject every Loss. Grayscale reduction requires an explicit threshold
unless the selected wire layout stores grayscale. No private metadata envelope
is emitted.

Input, tables, mapping expansion, output, conversion scratch, diagnostics, and
writer readback share a cumulative ResourceLimits budget. Counts and products
are checked before allocation; writer output must be readable with the same
limits. Selection uses ordered maps, and mapping trees are traversed at bounded
depth rather than scanning the Unicode address space. Outputs are deterministic.
Original fixtures and external tool observations are recorded in reports/w5.e1.
Reference implementations are external consumers and sources of field facts;
no GPL/MPL implementation is incorporated into the Rust codec.
