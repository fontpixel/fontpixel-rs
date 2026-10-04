# AppleSoft Toolkit Hi-Res Character Generator (W5.A2)

Primary evidence recorded before implementation, 2026-10-04:
[AppleSoft Toolkit Hi-Res Character Generator](https://mirrors.apple2.org.za/ftp.apple.asimov.net/documentation/programming/basic/Apple%20II%20Applesoft%20Toolkit.pdf). Fixed monobit 0.54.0 provides an independent MIT-licensed
format implementation; it is inspected as an oracle and not copied as code.

The SET payload is exactly 768 bytes: 96 eight-row glyphs at positions 32–127. Bits 0–6 are left-to-right dots; bit 7 shifts the row by one half-dot. The model uses a 14×8 interlaced grid, with the seven dots at even or odd columns. Per-row phase bits are preserved as native glyph properties, including empty rows. A row containing ink at both parities is unrepresentable and fails instead of silently merging it.


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

`HrcgReadOptions` and `HrcgWriteOptions` use the native option types. The `hrcg.phase` glyph byte property retains eight phase bits, including empty rows. Explicit output cells must be 14×8. Length-only detection is deliberately excluded from registry autodetection; select `hrcg` explicitly. No dithering or merging of mixed-phase rows is implicit.
