# Hercules Write On! WOF (`writeon`)

## Sources and scope

The independent implementation is monobit 0.54.0, MIT,
[`raw/writeon.py`](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/writeon.py).
The precise PyPI source hashes and upstream/source distinctions are recorded in
[the W5.1 inventory](../../reports/w5.1-survey.json). Code here is independently
written from these wire facts; monobit supplies the external reader/writer oracle.

## Wire and model mapping

22-byte little-endian header: ES 01 00, header size, width, height, tile height 14, first/last character, three auxiliary words. Payload is contiguous glyph-major, byte-aligned MSB-left rows. Width is a multiple of eight and height a multiple of fourteen.

Implicit code positions map to `Codepoint::Legacy`; explicit PSF Unicode maps to
Unicode aliases/sequences. Bitmap coordinates are left-to-right, top-to-bottom
on wire and use the model's bottom-left placement for metrics.

## Writing and information loss

Writer uses the native contiguous 0–127 character interval and 8×14 tile geometry. Header auxiliary words are retained. Glyphs are normalized to a common cell, with changes audited.

Writers require one font, accept monochrome bitmaps, and require an explicit
threshold to reduce Gray8. Unicode numbers become legacy positions only when
`unicode_as_legacy` is selected. After encoding, the output is decoded with the
same limits and audited against every public source model field, including
font/glyph properties, mappings, names, metrics and raster data. Strict mode
rejects any change or omission. Sparse range fillers and normalization are
reported through the same audit. There are no filesystem side effects.

## Bounds and validation

Input/output, tables, glyph counts, per-glyph pixels and cumulative retained
model/scratch/output bytes are bounded before allocation. Arithmetic and spans
are checked; noncanonical padding is diagnosed. No allocation-heavy model is
constructed during detection. Tests include independent oracle directions,
malformed boundaries, low budgets and strict metadata-loss behavior.
