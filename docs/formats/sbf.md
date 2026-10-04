# Sabriel Bitmap Font (SBF)

The native signature is `10 B1 7E 5C`. Four header bytes contain fixed advance
(zero means proportional), line height, ascent, and descent. A fixed directory
contains 255 eight-byte records for native codes 1 through 255. Each record is
u16 little-endian absolute raster offset, u8 row stride, u8 height, signed i8 left
bearing, signed i8 top above the baseline, u8 advance, and u8 overhang. Width is
stride*8-overhang; row bits are MSB-first. Offset zero means absent. Payload
starts after the 2048-byte header/directory. Native positions are Legacy codes;
no Unicode interpretation of the application’s SuperLatin encoding is invented.

An explicit unicode_as_legacy option permits numerical Unicode-to-slot mapping.
Code zero has no directory entry and cannot be represented. Signed bearings,
row padding, 16-bit offset limits, fixed/proportional advances, zero-sized blanks,
and sparse slots are validated independently. Only raster start addresses are
u16; the final bitmap may extend beyond byte 65535. The writer puts the largest
bitmap last when necessary to fit all start addresses. Canonical extension is
`.sbf`. Names and charset conversions are
Loss. Native line metrics and byte mappings are preserved.

Primary author/tool reference: [Bits’N’Picas](https://github.com/kreativekorp/bitsnpicas),
including its documented SBF support and external importer/exporter. Field facts
were checked against the author’s byte layout; implementation remains original.

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
