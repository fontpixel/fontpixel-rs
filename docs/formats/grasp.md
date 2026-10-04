# PCPaint / GRASP original fixed-cell format (W5.A2)

Primary evidence recorded before implementation, 2026-10-04:
[PCPaint / GRASP original fixed-cell format](http://www.textfiles.com/programming/FORMATS/glformat.txt). Fixed monobit 0.54.0 provides an independent MIT-licensed
format implementation; it is inspected as an oracle and not copied as code.

The seven-byte header contains LE u16 total length followed by byte count, first code, width, height, and bytes per glyph. Count is 1–255; first+count must not exceed 256. Glyphs are consecutive top-down MSB-left, byte-aligned rows. Declared glyph size and total file size must agree with geometry. This format is distinct from the ChiWriter/Fontrix-derived new GRASP form.


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

`GraspReadOptions` and `GraspWriteOptions` use the native option types. Output fills holes in the contiguous native range with blank glyphs and reports the resulting model projection. Width, height and glyph byte count must fit byte fields; a range of 256 entries cannot fit the original count byte and is rejected. Detection validates all size fields and returns only Possible confidence.
