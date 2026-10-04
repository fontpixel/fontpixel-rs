# PSF Tools textual PSF2 (`psf2txt`)

## Sources and scope

The independent implementation is monobit 0.54.0, MIT,
[`text/psf2txt.py`](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/text/psf2txt.py).
The precise PyPI source hashes and upstream/source distinctions are recorded in
[the W5.1 inventory](../../reports/w5.1-survey.json). Code here is independently
written from these wire facts; monobit supplies the external reader/writer oracle.

## Wire and model mapping

`%PSF2` followed by Version, Flags, Length, Width, Height. `%` begins each glyph; Bitmap consists of dash/hash rows, with backslash continuation. Unicode uses semicolon-delimited hexadecimal brackets; plus joins a Unicode sequence. The declared count and raster dimensions are validated.

Implicit code positions map to `Codepoint::Legacy`; explicit PSF Unicode maps to
Unicode aliases/sequences. Bitmap coordinates are left-to-right, top-to-bottom
on wire and use the model's bottom-left placement for metrics.

Text metadata uses validated UTF-8 (an optional UTF-8 BOM is accepted); raster
syntax and numeric fields retain their native ASCII spelling.

## Writing and information loss

Writer emits version 0 text with fixed cells, aliases and sequences. Glyphs without Unicode mappings retain their positional legacy label. Non-Unicode legacy values other than position have no wire field and are audited.

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
