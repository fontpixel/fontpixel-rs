# HZK and ASC raw character libraries

There is no self-describing HZK header or single universal wire standard. Geometry and placement are explicit options, never inferred from an extension as structural evidence. Sources consulted 2026-10-04: the original [Dot-matrix-font catalogue](https://github.com/zhangjirui/Dot-matrix-font) for conventional dimensions and the project `FORMATS.md` requirement for UCDOS print fonts (24-point files begin at row 16 and use transposed storage). The catalogue itself records both physical 16-bit rows and logical 12/14-pixel widths; these are distinguished here. No proprietary UCDOS bitmap is redistributed or treated as freely licensed.

| Family | Logical cell | Ordering |
| --- | --- | --- |
| HZK12, HZK14, HZK16 | 12², 14², 16² | Row-major; 12/14 padded to 16 bits |
| HZK24, HZK24S/K/H/F | 24² | UCDOS print: transposed, starts at zone 16 |
| HZK24T | 24² | Transposed symbols, zone 1 |
| HZK32, HZK40, HZK48 | 32², 40², 48² | Row-major, zone 1 |
| HZK40S, HZK48S | 40², 48² | Row-major, zone 16 |
| HZK40T, HZK48T | 40², 48² | Row-major symbols, zone 1 |
| ASC12/14/16 | 8 × height | Row-major single-byte slots |
| ASC24/32/40/48 | half-height × height | Row-major single-byte slots |

All preset parameters are overrideable through `Geometry`: width, height, first zone/cell or first ASCII slot, and row/column storage. A transpose exchanges raster axes, without rotation or reflection. HZK slots use the full 94×94 GB2312 code grid, including unassigned slots. Files can stop partway through that grid; byte counts must contain whole glyphs. The writer uses explicit `glyph_count`, or the last mapped slot, and zero-fills internal holes with a loss report. This preserves real-world 87-zone versus 94-zone lengths without fabricating missing tail glyphs. ASC has at most 256 slots, and codes above ASCII 127 remain raw Legacy values without guessed Unicode mappings.

Readers preserve exact `gb2312` Legacy codes; raw transfer does not require CJK tables. Actual Unicode↔GB2312 conversion requires `cjk-charsets`. GBK-only characters, unrelated Legacy charsets, and values outside the selected grid are reported individually and omitted; failure after all glyphs are omitted preserves the report. Source metadata absent from this headerless format, gray thresholding, aliases, duplicate mappings and fixed-cell metric changes are reported. Empty/truncated inputs, geometry overflow and resource excess return errors. There is intentionally no registered content detector.

Metrics convention: raw fixed cells have origin (0,0), ascent equal to height and descent zero. Nominal pixel and point size equal the cell height at 72 dpi. This documented convention permits immediate BDF export; it does not claim those metrics were recorded in the raw file.
