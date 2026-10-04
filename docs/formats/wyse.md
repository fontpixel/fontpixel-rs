# Wyse WY-60 downloadable characters (W5.A2)

Primary evidence recorded before implementation, 2026-10-04:
[Wyse WY-60 downloadable characters](https://bitsavers.org/pdf/wyse/WY-60/880261-01A_WY-60_Programmers_Guide_Jan87.pdf). Fixed monobit 0.54.0 provides an independent MIT-licensed
format implementation; it is inspected as an oracle and not copied as code.

Programmer's Guide pp.48–52 defines ESC c A, a decimal bank digit 0–3, two hex digits for position 00–7f, 32 hexadecimal row digits, then CTRL-Y. A font codec preserves the native 8×16 wire matrix. Terminal display repeats the rightmost column (row bit 0) into one or two extra columns for 132/80-column screens; it is not stored raster data. Legacy codes combine bank*256+position. Other control sequences, truncated patterns, invalid banks and positions fail.


## API, projection and limits

The feature and registry ID equal this document's filename. `*Codec` exposes
`read_options` and `write_options`; the module also exposes `read` and `write`.
Readers take `ReadContext` and typed options. Native positions stay `Legacy`;
`baseline` and `charset` supply interpretation without guessing an encoding.
Writers accept only Mono unless `gray_threshold` explicitly converts Gray8.
`cell` selects the output rectangle and `unicode_as_legacy` explicitly permits
numeric Unicode projection into legacy slots. Unsupported labels, additional
fonts, geometry, aliases and metadata are compared with the decoded output and
reported as Loss; `WriteOptions.strict` rejects any such loss.

Input, source model, temporary maps/rasters, output, readback and reports share a
checked cumulative budget. Pixel, glyph, table and resource limits apply before
material allocation. A successful write is reopened with the selected limits.
The implementation uses ordered maps or linear scans, without pairwise glyph
matching. Original tests and crosschecks are in `tests/crosscheck/w5a2.py` and
`reports/w5.a2*`; external source and tools stay under `local-work/w5a2`.

`WyseReadOptions` and `WyseWriteOptions` use the native option types. Duplicate definitions use the final raster and report Loss. Wire dimensions remain 8×16; screen display expansion is outside the file bitmap. The monobit 0.54.0 reader appends two blank columns; crosschecks explicitly crop these display columns before comparing the native raster. Its direct load/save failure is retained in the report, followed by a successful re-emission of the explicit eight-column model.
