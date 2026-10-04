# EDWIN decimal-row font (`edwin`)

## Sources and scope

The independent implementation is monobit 0.54.0, MIT,
[`text/edwin.py`](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/text/edwin.py).
The precise PyPI source hashes and upstream/source distinctions are recorded in
[the W5.1 inventory](../../reports/w5.1-survey.json). Code here is independently
written from these wire facts; monobit supplies the external reader/writer oracle.

## Wire and model mapping

First line is decimal first and last code. Each glyph is `label: height width row...`; there are exactly height arbitrary-precision nonnegative decimal row integers. Bit zero is the leftmost pixel. Code positions follow record order; labels whose first character is not a decimal digit become glyph names. Glyphs are top-aligned to the maximum height.

Implicit code positions map to `Codepoint::Legacy`; explicit PSF Unicode maps to
Unicode aliases/sequences. Bitmap coordinates are left-to-right, top-to-bottom
on wire and use the model's bottom-left placement for metrics.

Text metadata uses validated UTF-8 (an optional UTF-8 BOM is accepted); raster
syntax and numeric fields retain their native ASCII spelling.

## Writing and information loss

Writer emits one contiguous code interval, fills absent positions with blank cells, and preserves labels whose first character is not a decimal digit. Decimal row arithmetic is bounded by input/output, pixel and cumulative working-memory limits. No magic exists: auto-detection is intentionally disabled; use explicit format.

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

The decimal parser/formatter also charges one unit per byte or limb arithmetic
step against `max_table_bytes`. This explicitly bounds computational work for
arbitrarily wide malicious rows, while supporting widths beyond machine integers.
