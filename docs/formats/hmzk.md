# HMZK (Mi Band 2)

Canonical extension is `.bin`; `.hmzk` is an unambiguous alternative.
The native header is 16 bytes: ASCII `HMZK`, ten device-header bytes, and a
little-endian u16 character-directory byte length. The directory is an ordered
array of u16 character values; each entry owns exactly 32 following raster
bytes. Rasters are 16 rows of 16 pixels, MSB-first within each byte. The model
uses Unicode BMP scalars, ascent 16, descent 0, advance 16 and bounds (0,0,16,16).
Surrogate values and duplicate mappings are malformed. The ten opaque header
bytes are retained as `hmzk.header`; new files use the documented canonical
header. Odd directory lengths and truncated rasters are rejected.

Writing places representable ink into the fixed cell, with no implicit clipping.
Moving or padding a glyph, changing metrics, dropping a mapping, or reducing
grayscale produces Loss. Directory count is limited by its u16 byte length,
not by a hardcoded size of a particular vendor font dump.

Primary format account: [Gadgetbridge Mi Band 2 font format](https://freeyourgadget.codeberg.page/gadgetbridge_org/internals/specifics/mi-band-2/#font-format).
[Bits’N’Picas](https://github.com/kreativekorp/bitsnpicas) reads and writes native
HMZK and is used only as an external oracle. No vendor font data is redistributed.

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
