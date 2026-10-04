# Raw cell bitmaps

The raw reader has no content detector. Supply a `RawLayout` or named `RawPreset`.
Width, height, count, first legacy code, bit and row order, row padding, prefix
length, and charset are explicit. Exact payload length is required. Padding is
normalized with a repair report; a prefix is retained as an opaque property.

Named profiles: VGA 8×8/14/16, C64 screen-code order, two-byte-prefix C64 `.64c`,
ZX Spectrum printable 32–127, generic `.ch8`, and MSX. `.ch8` alone does not prove
Spectrum encoding. MSX likewise leaves regional encoding unspecified.

Primary references: [Sinclair BASIC manual](https://worldofspectrum.org/ZXBasicManual/),
[Commodore Programmer's Reference Guide](https://www.commodore.ca/manuals/c64_programmers_reference/c64-programmers_reference_guide-03-programming_graphics.pdf),
and [MSX technical handbook](https://github.com/Konamiman/MSX2-Technical-Handbook).
The MIT monobit raw reader is used only as an external oracle.

| Preset spelling | Geometry/count | First legacy code | Charset |
| --- | --- | --- | --- |
| `vga8`, `vga14`, `vga16` | 8×8/14/16, 256 | 0 | unknown; hardware does not establish a codepage |
| `c64` | 8×8, 256 | 0 | C64 uppercase screen codes |
| `64c`, `c64-prg` | 8×8, exact payload-derived count; two-byte PRG load address | 0 | C64 uppercase screen codes |
| `spectrum`, `zx-spectrum` | 8×8, 96 | 32 | ZX Spectrum |
| `ch8` | 8×8, exact payload-derived count | 0 | unknown |
| `msx` | 8×8, 256 | 0 | unknown; regional ROMs differ |

`RawPreset::layout()` yields an editable `RawLayout`. The registry's default raw
reader returns an actionable error instead of inferring geometry from length or
filename. `RawCodec.read_options.layout` configures it. Direct `read` takes the
same `RawReadOptions` and resource limits. `RawWriteOptions.layout` optionally
sets output layout; without it, the first source glyph establishes the cell and
first legacy code. The writer never silently selects a font from a collection.

With the default layout, glyphs are consecutive, row-major, byte-padded bitmaps. `row_padding` is an
additional byte count, not an alignment exponent. Prefix bytes are retained in
`raw.prefix` and reused only when their length matches the requested offset;
otherwise `default_load_address: Option<u16>` supplies a little-endian address
when present (requires `offset == 2`), or the prefix is zero-filled. The changed
property is reported. The `.64c` profile defaults to `$3800` for new output;
imported prefixes always take precedence. Callers may edit the default address.
The charset label does not invoke recoding. Gray8 requires an explicit threshold.

The address convention follows the [VICE file-format manual](https://vice-emu.sourceforge.io/vice_17.html)
and [Peter Kofler's original charset gallery](https://home-2002.code-cop.org/c64/).
The gallery archive contains 615 `.64c` files: 374 start with `00 38`, 152 with
`00 20`, and others use varied addresses. Thus `$3800` is a useful default,
not a magic signature or mandatory address. Partial charsets are common; files
whose payload is not a multiple of eight are rejected rather than padded.
The archive stays outside the repository because artwork rights vary.

The writer checks output by reading it with its chosen layout and identical
limits, charging both source and resulting models. Normalized cells, remappings,
aliases, sequences, metrics, provenance, names, properties and stable-ID holes
produce individual diagnostics. Tests cover arbitrary packed widths, both bit
and row orders, prefixes, padding, overflow, exact counts, and limited budgets.

Memory accounting includes source metadata, temporary prefix/row/cell buffers,
the output vector capacity, and the complete read-back model before allocating
each stage. Diagnostic vector and string capacities consume the same write
budget. Oversized escaped property names are checked before formatting. A budget
failure returns `ResourceLimit` directly instead of allocating a second complete
failure report; other failures retain bounded per-glyph diagnostics.

## Additional explicit layouts (W5.X)

`RawPacking::ByteAlignedRows` is the unchanged default. `strike_count` is the
number of glyphs placed side by side in one horizontal strike; glyphs inside a
strike abut at their actual pixel width, including widths not divisible by eight.
Each whole strike row is byte-aligned and then has `row_padding` extra bytes.
Strikes follow one another vertically in file order. An incomplete final strike
has zero-filled unused cells; an explicit `count` distinguishes these from glyphs.
Without a count, all complete strike cells are read. Exact final boundaries are
required; incomplete strikes are not padded silently.

`RawByteOrder::RowMajor` is the unchanged default. `ColumnMajor` transposes the
byte matrix of each strike, storing all row bytes for the first byte column,
then the second byte column, and so on. Bits inside each byte still represent
horizontal pixels. `bit_order` and `row_order` retain their current meanings.
Padding columns in this transposed matrix remain outside the glyph pixels.

`RawPacking::BitAligned` packs pixels continuously, including across scanline
and glyph boundaries. It has no per-row or per-glyph byte alignment. Only the
final storage byte is padded. This mode requires `strike_count == 1`,
`row_padding == 0`, and `byte_order == RowMajor`, matching monobit's defined
bit-aligned raw branch. With no explicit count, the maximum number of complete
glyphs fitting the bit stream is inferred; set `count` when tail zeros could
otherwise appear to be additional small glyphs. Nonzero unused tail bits produce
a repair diagnostic. No length-only content detector is added.

These mappings are independently checked against the MIT
[monobit raw implementation](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/raw.py).
There is an explicit Harlekin documentation discrepancy: the pinned monobit
README describes 512 8×8 glyphs in one 4096×8 strike, whereas its implementation
comment cites [Atari documentation](https://temlib.org/AtariForumWiki/index.php/Fonts)
describing 256 glyphs at 8×8 or 8×16.
The layouts are all representable and are given distinct profiles:

| Preset | Glyph count and cell | Strike geometry | Provenance |
| --- | --- | --- | --- |
| `harlekin`, `harlekin-iii`, `harlekin512` | 512 × 8×8 | 4096×8 pixels, 4096 bytes | monobit 0.54.0 README |
| `harlekin8` | 256 × 8×8 | 2048×8 pixels, 2048 bytes | Atari Harlekin description |
| `harlekin16` | 256 × 8×16 | 2048×16 pixels, 4096 bytes | Atari Harlekin description |

All have unknown legacy charset and first code zero. Their names do not assert
that an arbitrary `.fnt` file has one of these layouts. The existing C64, VGA,
Spectrum, CH8 and MSX definitions and `.64c` address precedence remain unchanged.

The row-major, single-glyph, byte-aligned path keeps its current direct byte loop.
Additional layouts use checked offset arithmetic and per-glyph or per-strike
scratch, never repeatedly copying a full growing atlas. Reader and writer budget
reservations include scratch, output capacity, padding diagnostics and readback.

The independent column-order check uses monobit 0.54.0's public
`Raster.from_bytes(order="column-major")` reader followed by cell crops.
Its raw loader passes an obsolete `byte_order` keyword that Raster ignores;
the unmodified wrapper therefore reads column-order input incorrectly. The
crosscheck asserts this known discrepancy, then uses the functional public
Raster API. The independent writer direction transposes monobit's row-order
output using an original byte-matrix assembler. No oracle files are patched.

The direct `RawCodec` trait reader honors the `ReadContext` resource limits;
its trait writer honors `WriteOptions.strict`. All new layouts retain the same
full model loss audit as the original default layout. Original fixture tests
cover both bit orders, both row orders, incomplete final strikes, zero and
nonzero padding, checked geometry overflow and equal read/write budgets.

For bit-packed data, write scratch and readback budgets use the expanded
canonical row storage size, which may be up to eight times the wire size.
