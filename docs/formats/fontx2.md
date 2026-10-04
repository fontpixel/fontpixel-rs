# DOS/V FONTX2

Primary description: ChaN, [Using FONTX font files](https://elm-chan.org/docs/dosv/fontx_e.html), 2012-10-21; consulted 2026-10-04. MIT monobit `storage/fontformats/fontx.py` is used as an independent oracle, not production code.

| Offset | Meaning |
| --- | --- |
| 0 / 6 | ASCII FONTX2 signature |
| 6 / 8 | Raw fixed-width name; Latin-1 gives reversible model text |
| 14 / 1, 15 / 1 | Width and height (nonzero) |
| 16 / 1 | 0: 256 single-byte slots; 1: double-byte block table |
| 17 / 1 | Double-byte block count |
| 18 / 4 per block | Inclusive start/end, little-endian u16 |
| bitmap | ceil(width/8) bytes per row, MSB first, top down |

Glyphs retain Legacy codes and fixed cells with origin (0,0) and width advance. Double-byte identity is Shift-JIS, never silently CP932. Single-byte ANK identity is deliberately `fontx2-ank` unless the caller declares a charset: ANK alone is insufficient to select CP437, JIS X 0201, or another mapping. Exact charset conversion uses the existing tables and needs `cjk-charsets` for CJK conversions. Block order is retained; reversed or overlapping blocks fail. Trailing data and nonzero padding are reported as loss.

Output options select single/double/automatic layout, charset, cell dimensions, and explicit gray threshold. Contiguous populated double-byte ranges are encoded without scanning 65536 empty slots; when more than 255 runs are needed, page ranges are used and inserted blank slots are reported. Single-byte output necessarily materializes 256 cells; inserted blanks, aliases split across slots, name truncation, metric normalization and unsupported metadata are reported. Native read/write/read preserves the model without loss.

Metrics convention: raw fixed cells have origin (0,0), ascent equal to height and descent zero. Nominal pixel and point size equal the cell height at 72 dpi. This documented convention permits immediate BDF export; it does not claim those metrics were recorded in the raw file.
