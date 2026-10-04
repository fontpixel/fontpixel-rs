# PolyPrint

Primary specification: [John Elliott's Polyprint font format](https://www.seasip.info/Unix/PSF/Amstrad/PolyPrint/index.html). The previously catalogued `/ZX/polyprint.html` URL is obsolete. The MIT monobit loader is an independent oracle.

A 128-byte header stores a little-endian font ID, a name-length byte, 25 padded name bytes and opaque flags/data. Then 144 records correspond to legacy codes 32–175. Each 84-byte record contains a left-marker byte, 20 top-pass-one column bytes, width byte, 20 top-pass-two bytes, right-margin byte, 20 bottom-pass-one bytes, opaque byte and 20 bottom-pass-two bytes. Alternate passes interleave into a 20×32 raster. Keep all header bytes and glyph fields as properties. The effective mapping follows the established loader: x=1-left-marker, advance=1+width+right-margin. Global baseline/line height are explicit options; defaults follow the observed flag-bit convention (18 or 36 rows), whose status is an inference rather than a documented font field.

Writing preserves opaque fields, uses a padded Latin-1 name, inverts the same byte/metric mapping, and audits unsupported data against the actual output. A fixed-size match is only Possible detector confidence; malformed name lengths, records or bounds fail. No charset guessing: the historical PolyPrint encoding needs explicit conversion. Gray conversion is opt-in, and cumulative budgets include the source/target models, metadata, output and reports.

The primary author's real `002.fnt` uses CP/M 128-byte record padding: its12224-byte font payload occupies12288 bytes. Both exact payloads and this64-byte trailer are accepted; trailer bytes are retained separately and copied on write, including nonzero opaque bytes. Other trailing lengths fail.
