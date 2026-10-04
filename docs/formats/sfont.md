# SDL SFont and Kreative RFont

## Native fields and evidence (recorded before implementation)

The original [SFont project](https://github.com/karlb/sfont) describes a horizontal
image strip with a magenta marker row. Native character order begins at ASCII
33; spaces are a runtime convention. Alternating marker/advance runs delimit
proportional cells. Half of an internal marker belongs to each adjacent glyph;
odd runs follow the native loader's integer midpoint. The outer half of the first/final marker
is outside the SDL blit cell. Glyph pixels may extend into these
bearing areas. A caller may explicitly supply a different codepoint sequence
and baseline; absent such a declaration the native ASCII order and full-height
ascent apply. Writer default is the fixed ASCII 33..126 repertoire, with absent
slots represented as blank native cells and any added mappings diagnosed.

[Kreative Bits’N’Picas](https://github.com/kreativekorp/bitsnpicas) defines RFont:
multiple strips with a vertical metric control column, magenta codepoint columns,
red left-overhang spans, transparent advance spans and blue right-overhang spans.
Control colors are red/orange/yellow/transparent for successive ascent regions,
green/cyan/blue for descent regions and violet for line gap. The first RGB sample
under a codepoint header stores the 24-bit value. Following samples repeat its
six hexadecimal digits at increasing redundancy; 0xFFFFFF terminates a strip.
Rows are grouped by Unicode page in canonical output. Native PNG text keywords
FontFamily, FontStyle and Copyright retain names. The auxiliary rfVM/rfHM chunks
contain duplicate big-endian metric tables; they are native metadata, never a
fontpixel-specific envelope. Exact field variants and crosschecks are archived
in reports/w5.e2-image. Reference applications execute externally; their
GPL/LGPL implementation is not incorporated in the codec.

## Model, ambiguity and loss

RGBA alpha is coverage for native transparent fonts. Opaque background-key
images use the native background color with an explicit grayscale projection;
color-to-coverage approximation produces Loss. Native RFont positions preserve
Unicode scalars; SFont's marker image cannot encode arbitrary codepoints by
itself. Images have no reliable font-specific magic, so callers should select
`sfont` explicitly when a generic image decoder is also enabled.

The codec never crops overflowing ink silently. Padding, missing ASCII slots,
unsupported names or metadata, mapping/domain changes, baseline changes and
color projections are audited against native readback. Strict mode rejects
Loss. Ordered selection, bounded strips and bounded PNG metadata avoid quadratic
work. Input/model/PNG scratch/output/readback/diagnostics share ResourceLimits.

## Exact native tables and reader profile

`rfVM` starts with big-endian u16 version 1 or 2, followed by ten fields of u8
(v1) or u16 (v2): maximum ascent, line ascent, em ascent, x-height, em descent,
line descent, maximum descent, line gap, baseline offset (maximum ascent minus
one, wrapping at zero), and bitmap height. `rfHM` starts with u16 version and
u32 range count. Each range has u32 first scalar and u32 count. Entries are
left overflow, advance, right overflow, bitmap width, bitmap x and bitmap y;
v1 uses four u8 plus two u16, v2 uses four u16 plus two u32. Both tables must
agree exactly with control pixels. Duplicate tables, overlapping ranges,
invalid scalars and contradictory redundant codepoint samples fail.

SFont reads use the official SDL integer-midpoint crop and bearing convention.
Odd marker widths follow that loader; the writer emits even marker runs so
half-width rounding is unambiguous. Outer half-markers are unused by SDL;
visible source pixels outside its cells produce explicit Loss. Canonical
output reserves transparent outer halves and never clips source ink. Default
writing supplies all 94 ASCII slots; missing slots gain blank advance-one
cells and this repertoire change is audited. Custom sequences are an explicit
out-of-file declaration, with at most 255 glyphs. RFont stores its own sequence
and baseline, so these SFont-only overrides are rejected for RFont.

RFont keeps complete native cells, including transparent line-gap rows.
Its maximum-descent calculation excludes those retained gap rows, so repeated
writes do not grow the font. Native em/max/x-height fields and table versions
are retained as typed `rfont.*` properties. FontFamily, FontStyle and Copyright
are native Latin-1 PNG tEXt fields in either flavor. Other tEXt fields retain
exact bytes as `sfont.png_text` / `rfont.png_text`; conflicting duplicate names
are rejected. Names outside Latin-1 or containing NUL cannot use this native
text representation and are diagnosed on write.

The Bits’N’Picas public glyph API contracts transparent borders. Crosschecks
compare all nonzero coverage samples in exact baseline coordinates and all
advances, and independently validate complete wire cells. The official SDL
consumer is compiled unchanged, using its caller-owned surface blend mode
`SDL_BLENDMODE_NONE` to inspect original alpha coverage without blend rounding.
Bits’N’Picas can reject its own SFont output when first-glyph overhang paints the
vertical column it uses for format detection; this external-tool limitation is
recorded separately and the same data is checked with the native SDL loader.
