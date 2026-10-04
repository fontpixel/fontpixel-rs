# Consoleet / vfontas directory (W5.A2)

Primary evidence recorded before implementation, 2026-10-04:
[Consoleet / vfontas directory](https://manpages.debian.org/bookworm/hxtools/vfontas.1.en.html). Fixed monobit 0.54.0 provides an independent MIT-licensed
format implementation; it is inspected as an oracle and not copied as code.

A native directory contains hexadecimal-named .txt files, each beginning PCLT, then decimal width and height, then exactly height rows of width doubled pixels: .. means clear and ## means ink. Declared dimensions and paired characters are validated. Filenames carry numeric codes, not an implicit character encoding: a typed Legacy/Unicode mode makes the interpretation explicit. A bounded ReadContext listing and one resource read per glyph are used. OutputBundle directory assets remain under the primary directory.


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

`ConsoleetReadOptions.codepoint_mode` and its write counterpart select
`CodepointMode::{Legacy,Unicode}`. Unicode mode requires scalar values, including
for filenames. Use `Input::directory(name, resolver)`; one bounded immediate
listing plus one read per glyph consumes `glyph_count + 1` resource operations.
Hexadecimal filenames are unique by numeric value, so `41.txt` and `0041.txt`
conflict. Nested paths, unrelated files and invalid pixel pairs are errors.
Output is `OutputBundle::directory`, with all assets under the primary path.
No cell option preserves individual glyph widths and heights. Baselines remain
an explicit input interpretation because the native text has no baseline field.
