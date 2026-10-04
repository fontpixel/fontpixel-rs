# Scalable Screen Font 2

Specification recorded before implementation, 2026-10-04. Primary sources are
bzt's [binary SFN2 specification](https://gitlab.com/bztsrc/scalable-font2/-/blob/master/docs/sfn_format.md),
[ASCII source specification](https://gitlab.com/bztsrc/scalable-font2/-/blob/master/docs/asc_format.md),
and [MIT renderer](https://gitlab.com/bztsrc/scalable-font2/-/blob/master/ssfn.h).
The independent oracle is the author's `sfnconv` plus renderer; the original
project raster fixtures contain no upstream font artwork. Download hashes and
external-tool build commands will accompany the W5.E2-outline report.

## Binary wire representation

SFN2 uses a 32-byte little-endian header and terminal `2NFS`. The header gives
the complete byte size, family/style byte, revision zero, grid width/height,
top-relative baseline/underline, a 16-bit fragment offset and 32-bit character,
ligature, kerning and palette offsets. Six mandatory NUL-terminated UTF-8 strings
supply names, version, designer and license. Individual strings are bounded by
255 bytes. SFNC contains concatenated complete SFN2 fonts under its own length.
The shared bounded gzip transport supports gzip-wrapped single fonts/collections.

The character stream covers Unicode through U+10FFFF using one/two-byte skips,
the special 65536-position skip, or six-byte glyph headers. Glyph records contain
width/height, two advances, an overlap up to 63 pixels and fragment descriptors
with 24/32-bit offsets. Descriptor coordinates are relative to the font grid.
Surrogate mappings are invalid Unicode scalars. Ligature records associate the
U+F000–U+F8FF interval with UTF-8 strings in the string table.

Bitmap fragments have a two-byte header: byte pitch and row count are encoded
minus one. Pixels are LSB-first within each byte. Pixmap fragments contain an
8 KiB-bounded TGA packet stream and palette indices: 254 means foreground and
255 means transparency. Palette entries are BGRA. Native grayscale coverage is
represented with alpha; RGB color reduction is an explicit loss. All referenced
fragment spans, packets, dimensions, offsets and final boundaries are checked.

Fragments are composed in native order. Bitmap/pixmap fragments remain raster
content; contours are never rasterized as a substitute. Mixed contour/raster
files produce an explicit contour-loss report. Unsupported hinting/color/kerning
normalization must be either retained in native properties or diagnosed. Pure
outline input must not silently appear as a successfully imported bitmap font.
Native ligature sequences and glyph advances are preserved where representable.

## ASCII and output

The ASCII variant uses `# Scalable Screen Font #`, `$` properties, Unicode glyph
records, bitmap rows or ARGB pixel rows, optional color/kerning/hint commands,
and mandatory `# End #`. It shares the binary coordinate and layer semantics.
`SsfnEncoding::{Binary, Ascii}` selects output, with binary as default. Binary
output includes actual bitmap/pixmap fragments and character mappings; ASCII is
an additional native representation. SFNC is used for multiple binary fonts.

Output strings, grid coordinates and native byte-sized metrics are range checked.
Unsupported model attributes, changed cells, positional mappings, aliases, names,
sequences, metadata and stable slots are audited by decoding the produced output
under the same limits. Strict mode rejects any combined read/write loss.
Cumulative budgets cover input, decompression, fragment indexes, composition
scratch, glyph models, palettes, native metadata, diagnostics and writer readback.
No detector allocates a font model or trusts an unvalidated offset/length.

## Verified author-tool compatibility details

The author's current `sfnconv` emits bitmap ASCII rows at the actual glyph width,
although its document describes byte-rounded rows. Both forms are accepted;
nonzero padded columns are rejected, and the writer uses actual width. The
converter also emits a final skip ending at U+110001 and may omit the documented
zero ligature terminator. Those two exact terminal forms are accepted only at a
validated section end and produce Repair diagnostics; normal output uses the
specified U+110000 endpoint and explicit ligature terminator.

Native Gray8 output uses white palette RGB with exact alpha coverage, and the
binary writer splits pixmaps into bounded literal TGA packets. The author's
binary reader/ASCII exporter preserves every alpha sample from this output.
Its ASCII importer coalesces equal RGB with differing alpha: the pinned original
fixture's varying coverage becomes 139 at every ink position. This external-tool
normalization is asserted separately in crosschecks. Independent parsing of the
written ASCII values still verifies every original alpha value. The codec itself
preserves Gray8 in both representations. Colored input reduces RGB to alpha with
Loss; no RGBA approximation is claimed.

Kerning and hint fragment spans/packets are validated and their omission produces
explicit per-glyph Loss. Mixed contours likewise produce Loss; contour-only input
fails instead of importing a fabricated rendered strike. Additional model fields,
aliases and unsupported ASCII properties participate in the full writer audit.
