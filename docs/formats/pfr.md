# Portable Font Resource bitmap strikes

Specification recorded before implementation, 2026-10-04. The complete primary
wire reference is [DAVIC 1.4.1 Part 9, Annex A](https://docbox.etsi.org/reference/davic/davic%201.4.1%20specification%20part%209.pdf),
PDF pages 111–137, especially A.2–A.5.4 and A.8.1–A.10. The public
[FreeType PFR interface](https://freetype.org/freetype2/docs/reference/ft2-pfr_fonts.html)
provides the independent consumer. Production parsing/writing is authored from
wire semantics; no GPL/MPL/EPL implementation is incorporated. Original Python
wire fixtures exercise independently assembled records and FreeType consumes
native writer output.

## Wire sections and native scope

The 58-byte big-endian PFR0 header carries version 4, a CR/LF check word,
section spans, counts and bitmap polarity/order flags. A logical font directory
contains 16-bit record sizes and 24-bit absolute offsets. Logical records hold
an affine matrix, optional outline stroke/bold data, extension items and a
physical-font reference. Physical records describe coordinate resolutions,
bounds, variable-length character entries, auxiliary data and extension items.
The final trailer contains a 24-bit total length followed by `$PFR$`.

Physical extension type 1 supplies bitmap strikes and character-table spans;
its flags select one/two-byte ppem/counts and two/three-byte table fields.
Bitmap character tables are sorted by native character code and contain bounded
glyph-program spans. Embedded bitmap code values are legacy positions because
the base format does not establish Unicode. Each bitmap strike is exposed as a
font with its own X/Y ppem. Multiple logical/physical fonts and multiple strikes
are handled without conflating their geometry.

Bitmap glyph-program headers select position, size, advance and image encodings.
Positions and advances may be integer or signed 1/256-pixel values; fractional
values retained in native properties require an explicit loss when projected to
integer model metrics. Raster formats are continuous packed bits, white/black
RLE4 runs, and white/black RLE8 runs. Rows are not byte padded on disk. All three
are reader/writer variants. Polarity and vertical order follow the header flags.
Size/position/advance selector widths and terminal packet boundaries are checked.

Only embedded bitmap strikes are imported. Outline glyph programs are not
rendered in place of absent bitmaps; mixed outline content is explicitly reported
as loss. Outline-only fonts are explicit unsupported bitmap input. Logical
outline effects and auxiliary/native metadata must be retained where meaningful
or reported when omitted. Missing mappings, duplicate codes, zero resolutions,
truncated spans and invalid record sorting are errors.

## Output and resource contract

The writer emits a complete native PFR container with logical records, physical
records, bitmap extension tables and bitmap glyph programs, including the trailer.
`PfrCompression::{Raw, Rle4, Rle8}` selects the native raster stream. Explicit
Unicode-number-to-legacy conversion and grayscale thresholding are opt-ins.
Native character codes, geometry, offsets and fields are checked before encoding.
Unknown metadata, outline-related information, normalization, aliases, sequences,
font/glyph metrics and stable slots are covered by a full emitted-model audit.
Strict mode refuses any combined read/write loss.

Cumulative limits cover section/table scans, character lookup indexes, decoded
pixels, retained models, native properties, report allocation and output/readback.
Offset arithmetic is checked before slicing and repeated shared program references
cannot evade glyph/pixel/aggregate limits. The detector validates the PFR header,
section boundaries and matching trailer without constructing raster models.

## Published specification versus deployed FreeType dialect

Original records were independently consumed by FreeType 2.13.2. Its actual
conventions differ from DAVIC in four observable places: BCT offsets start at the
*end* of the physical record, 16/24-bit positions are whole pixels, Y positions
always identify the bottom row, and raw ink is always bit 1 regardless of the
header polarity flag. Three compression modes and both row orders have identical
45-pixel and signed-bearing results in 12 original FreeType fixtures.
`PfrDialect::{Davic, FreeType}` makes this difference explicit. Reader options may
override detection; absent an override, a BCT relative offset smaller than its
physical record selects FreeType, otherwise DAVIC. Ambiguous nonzero offsets can
be selected explicitly. Writer `dialect: None` preserves `pfr.dialect`; new fonts
default to FreeType. In that dialect `black_pixel` changes the header declaration,
while raster bits follow the actual consumer's invariant bit-1 ink convention.

Native physical metadata retains the resolution/bounds, non-bitmap extensions,
auxiliary and blue/stem records; logical prefixes and per-glyph outline advance /
obsolete ASCII fields are also retained. Output derives live character and BCT
records rather than replaying stale offsets. Logical/physical sharing may be
normalized to one physical record per model strike; the audit reports any changed
native reference or ordering. Physical records above 64 KiB use the documented
high size byte; the original 7,400-glyph regression exercises it.

Fractional DAVIC positions/advances are retained in `pfr.fractional-metrics` and
projected to integer metrics with floor, with explicit reader loss. Writing the
integer model reports omission of those fractional values. Outlines and unsupported
model metadata are never silently inferred from bitmap geometry.
