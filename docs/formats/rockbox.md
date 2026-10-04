# Rockbox bitmap fonts

Canonical extension is `.fnt`; `.rb11` and `.rb12` are unambiguous alternatives.
Native RB11 and RB12 are selected with RockboxFlavor. RB11 begins with `RB11`,
64 bytes of family name and 256 bytes of copyright. RB12 starts with `RB12` and
has no name fields. Both then contain little-endian u16 maximum width, height,
ascent, depth, followed by u32 first character, default character, character
count, raster count, offset count, and width count. Zero optional table counts
select fixed metrics; otherwise the count equals the character count.

RB11 raster count and offsets are in 16-bit words. Each row is MSB-first within
little-endian words, with row width rounded to 16 bits; offsets are u32 and the
raster section is aligned to four bytes. RB12 depth zero packs vertical groups
of eight rows, least-significant bit at the top. Its raster count is bytes;
offsets are u16 below `0xFFDB` raster bytes and u32 otherwise, with corresponding
2/4-byte table alignment. Depth one stores continuous low-nibble-first inverse
4-bit coverage: `(15 - nibble) * 17`. Native width/height must fit the runtime
loader's positive signed 16-bit range. Width entries are bytes. Missing range members use
the default glyph at runtime; the model exposes this aliasing honestly.

Primary field references: [Rockbox font header](https://github.com/Rockbox/rockbox/blob/master/firmware/export/font.h),
[runtime loader](https://github.com/Rockbox/rockbox/blob/master/firmware/font.c),
[official converter](https://github.com/Rockbox/rockbox/blob/master/tools/convbdf.c).
Bits’N’Picas provides an additional independent RB11/RB12 consumer. Its RB12
reader only handles 16-bit offsets. The unmodified official runtime loader
validates the large-offset and Gray4 cases in the external crosscheck. For
offsetless fixed cells, the official loader rounds each cell to two bytes;
Bits’N’Picas emits packed odd cells. The reader accepts both, using the raster
length to distinguish the packed variant. The writer emits explicit offsets,
which both consumers accept. Gray4 rounds coverage to multiples of 17 and
reports any changed coverage as Loss.

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
