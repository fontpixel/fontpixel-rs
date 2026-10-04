# DosStart bitmap DSF (`dosstart`)

## Sources and scope

The independent implementation is monobit 0.54.0, MIT,
[`text/dosstart.py`](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/text/dosstart.py).
The precise PyPI source hashes and upstream/source distinctions are recorded in
[the W5.1 inventory](../../reports/w5.1-survey.json). Code here is independently
written from these wire facts; monobit supplies the external reader/writer oracle.

## Wire and model mapping

ASCII `DosStartFont`, name, then `1,ascent`. Glyphs start at code 32. Each record has decimal width/height (three digits each), row-major G/H pixels, comma and advance. The stroke (`0`) variant is recognized but rejected explicitly; it is not a bitmap. A short historical vector is padded at the top, as in the independent reader.

Implicit code positions map to `Codepoint::Legacy`; explicit PSF Unicode maps to
Unicode aliases/sequences. Bitmap coordinates are left-to-right, top-to-bottom
on wire and use the model's bottom-left placement for metrics.

## Writing and information loss

Writer emits CRLF and 96 positions (32–127); dimensions must fit three decimal digits. Glyph y is ascent minus stored height. Names use ASCII. Advance is explicit; left bearing is normalized and audited.

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
