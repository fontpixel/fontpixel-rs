# Dr. Halo bitmap FON (`drhalo`)

## Sources and scope

The independent implementation is monobit 0.54.0, MIT,
[`raw/drhalo.py`](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/raw/drhalo.py).
The precise PyPI source hashes and upstream/source distinctions are recorded in
[the W5.1 inventory](../../reports/w5.1-survey.json). Code here is independently
written from these wire facts; monobit supplies the external reader/writer oracle.

## Wire and model mapping

20-byte header begins AH; dimensions are signed 16-bit width/height at 16/18. Bytes per glyph is at 12. Header byte 2 equals C8 for first position zero, otherwise first position 32. Positive dimensions identify bitmaps; zero dimensions identify unsupported strokes. Packed row-major MSB-left payload count is inferred from exact length.

Implicit code positions map to `Codepoint::Legacy`; explicit PSF Unicode maps to
Unicode aliases/sequences. Bitmap coordinates are left-to-right, top-to-bottom
on wire and use the model's bottom-left placement for metrics.

## Writing and information loss

Writer authors a 20-byte native header from field semantics, never copying the historical copyrighted protection stub. Source header fields are retained when possible. Typed first-codepoint override supports externally identified variants. AH alone is weak evidence: a full structure probe returns Possible.

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
