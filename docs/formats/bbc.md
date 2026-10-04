# BBC Micro VDU 23 (W5.A2)

Primary evidence recorded before implementation, 2026-10-04:
[BBC Micro VDU 23](https://www.riscos.com/support/developers/bbcbasic/part2/vducontrol.html). Fixed monobit 0.54.0 provides an independent MIT-licensed
format implementation; it is inspected as an oracle and not copied as code.

A standalone font stream consists of byte 23, a printable character position 32–255, and eight top-down MSB-left rows, repeated per glyph. Other VDU commands or arbitrary intervening bytes are rejected rather than scanned past. Duplicate definitions follow the stream's last definition with a Loss report. The eight-byte raster is an 8×8 cell; codes remain legacy positions.


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

`BbcReadOptions` and `BbcWriteOptions` share the native option types. Only standalone printable VDU 23 records are accepted; this is not a general terminal-command interpreter. Output codes are sorted.
