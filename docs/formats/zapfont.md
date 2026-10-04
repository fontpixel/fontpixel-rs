# RISC OS ZapFont and ZapRedraw UCS (W5.A2)

Primary evidence recorded before implementation, 2026-10-04:
[RISC OS ZapFont and ZapRedraw UCS](https://raw.githubusercontent.com/jaylett/zap/master/dists/fonts/!ZapFonts/Fonts/!ReadMe,fff). Fixed monobit 0.54.0 provides an independent MIT-licensed
format implementation; it is inspected as an oracle and not copied as code.

Old ZapFont begins ZapFont+CR then six LE u32 fields: width, height, first, last-exclusive, zero, zero. ZRUF has a 32-byte header and 16-byte chunk entries (base code, count, offset, zero). Partial chunks have a 256-byte ordinal translation table; full chunks omit it. Entry values select actual glyph ordinals, not merely sorted present codes. Chunk 0 is full Acorn Extended Latin-1; later chunks are Unicode, with explicit validation and a native null glyph. Row bit order follows the ZapRedraw bitmap convention, including widths not divisible by eight.


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

`ZapVariant::{Old,Redraw}` selects output (Redraw default); the reader's optional
variant requires the corresponding magic. Old files retain numeric legacy
positions; Redraw chunk zero remains native Acorn legacy positions and later
chunks use Unicode scalar labels. Values outside Unicode are rejected rather
than wrapped. Native null glyphs, including an offset that shares a stored
raster, become the model's default glyph. Partial tables honor actual one-based
ordinals, repeated aliases and unreferenced stored glyphs. Invalid offsets,
overlaps between chunks and invalid ordinals fail. Unassigned bytes between or
after sections produce a read Loss. Native raster bytes use low-bit-left order,
including non-byte widths. Writers canonicalize partial tables and duplicate
aliased rasters as needed; changes to glyph identity/unencoded glyphs are audited.
Monobit's writer normalizes Acorn encoding: it can synthesize extra slots and replace native control-slot rasters. The original source-projection mismatch is retained; our reader independently matches the oracle's resulting output. The primary-spec ordinal/alias fixture additionally records the fixed oracle reader's sorted-presence limitation.
