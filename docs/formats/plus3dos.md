# PLUS3DOS Spectrum font (`plus3dos`)

## Sources and scope

The independent implementation is monobit 0.54.0, MIT,
[`raw/plus3dos.py`](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/plus3dos.py).
The precise PyPI source hashes and upstream/source distinctions are recorded in
[the W5.1 inventory](../../reports/w5.1-survey.json). Code here is independently
written from these wire facts; monobit supplies the external reader/writer oracle.

## Wire and model mapping

128-byte +3DOS header: PLUS3DOS 1A, issue/version, total size at 11, CODE type at 15, data length 768 at 16, load address at 18, and checksum at 127 (sum of preceding bytes modulo 256). Payload contains 96 8×8 glyphs at codes 32–127.

Implicit code positions map to `Codepoint::Legacy`; explicit PSF Unicode maps to
Unicode aliases/sequences. Bitmap coordinates are left-to-right, top-to-bottom
on wire and use the model's bottom-left placement for metrics.

## Writing and information loss

Writer emits valid native CODE headers, preserving unknown header fields and load address when read from this format; checksum and lengths are recomputed. Header checksums, data length and terminal boundaries are validated.

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
