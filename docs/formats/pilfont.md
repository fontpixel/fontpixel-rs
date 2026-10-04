# Pillow PILfont

## Native fields and evidence (recorded before implementation)

Primary specifications are the maintained Pillow
[FontFile writer](https://github.com/python-pillow/Pillow/blob/main/src/PIL/FontFile.py)
and [ImageFont loader](https://github.com/python-pillow/Pillow/blob/main/src/PIL/ImageFont.py).
The MIT [monobit codec](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/image/pilfont.py)
is an independent producer and consumer, with known omissions checked separately.

`.pil` contains `PILfont\n`, a semicolon-delimited metadata line (field six is
line height), optional informational lines, `DATA\n`, and 256 fixed records.
Each record has ten signed big-endian i16 values: advance x/y, destination
left/top/right/bottom, source left/top/right/bottom. All-zero records are absent;
nonzero advance with empty source geometry is a valid blank. Destination and
source rectangle sizes must agree for an unscaled bitmap. Destination top-down
coordinates become model x=left, y=-bottom; vertical advance changes sign.
Native byte slots are Legacy mappings. Numerical Unicode coercion requires an
explicit option. Header fields and information lines are retained as native
properties without inventing font name/charset metadata.

The paired `.pbm` is usually **PNG**, as Pillow's own writer specifies. The
reader follows explicit atlas selection or bounded same-stem PNG/GIF/PBM lookup;
actual container magic determines decoding. Gray8 is the image's intensity mask,
with zero clear and 255 ink. Only monochrome and grayscale images are native
Pillow mask types; RGB input is rejected. The writer uses deterministic grayscale
PNG named `.pbm`, accepted by Pillow ImageFont.load. It does not require Pillow.

## Limits and loss

Signed coordinate/advance limits and the 256-slot directory are validated before
allocation. Source model, resource bytes, atlas decoding/encoding scratch,
output bundles, diagnostics and native readback share a cumulative budget.
Output must read under the same limits. Shelf packing is deterministic and
linear after ordered mapping selection; no pairwise collision scan is used.

Unsupported aliases, mappings, Unicode sequences, names, global metrics and
properties produce explicit Loss via native readback. Grayscale masks are kept;
no threshold is silently chosen. Geometry that would clip or overflow fails.
Strict writes fail if any Loss occurs. Independent tests use original rasters
through both Pillow and monobit, not only a Rust roundtrip.

Native lookup reserves same-stem `.png`, `.gif` and `.pbm` asset names. Writers
reject these extensions for the metrics primary, since it would shadow its own
mask lookup and break default native readback. Dots in parent-directory names
do not replace the actual filename stem. Input/output path copies and caller
filename profiles are charged before allocation.
