# XBIN font and screen container (W5.A2)

Primary evidence recorded before implementation, 2026-10-04:
[XBIN font and screen container](https://web.archive.org/web/20120204063040/http://www.acid.org/info/xbin/x_spec.htm). Fixed monobit 0.54.0 provides an independent MIT-licensed
format implementation; it is inspected as an oracle and not copied as code.

The 11-byte header is XBIN+1a, LE u16 screen width/height, font height 1–32, and flags. Optional palette is 48 RGB components in 0–63. Font data is 256 or 512 fixed 8-pixel glyphs. Screen data is character/attribute pairs or four-mode RLE, with the count encoded as low-six-bits+1. Palette, screen geometry, flags and encoded screen data are retained as native properties; malformed screen streams fail. Typed font-only output explicitly reports dropping non-font sections.


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

`XbinWriteOptions.count` is `Auto`, `Chars256` or `Chars512`.
`screen` is `Preserve` (default) or `FontOnly`. Properties `xbin.header`,
`xbin.palette` and `xbin.screen` retain the native non-font sections; preserved
screen RLE is validated in all four modes without decompressing it into a large
buffer. A font-only container uses both screen dimensions zero. Files without
an embedded font, reserved flags, invalid palette components, excess RLE cells,
truncation and unassigned trailing bytes (including unsupported SAUCE trailers)
are rejected. monobit 0.54.0's 512-slot writer sets the wrong field name and emits
a 256-slot flag with 512 payloads: the original failure is recorded, not patched.
Our 256/512 outputs both pass the independent reader.
