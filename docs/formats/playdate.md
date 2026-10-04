# Playdate source and compiled bitmap fonts

Source `.fnt` files are UTF-8 text: glyph token plus width, two-character token
plus kerning adjustment, tracking and cell dimensions. `space` and `U+HEX` avoid
ambiguous characters. Comments start with `--`. Standalone files contain base64
PNG `data` and its `datalen`; split fonts resolve a caller-authorized PNG atlas.
Canonical extensions are `.fnt` for source and `.pft` for compiled output;
sidecars follow `<stem>-table-W-H.png` and are resolved through explicit paths
or a bounded caller-provided directory listing.
The atlas uses row-major fixed cells. Width plus tracking is the advance.
Caps `--metrics` JSON carries authoring baseline/kerning information; supported
native properties are preserved, unsupported metadata changes are Loss.

Compiled `.pft` uses `Playdate FNT` and flags, followed by a page usage bitmap,
page offsets, glyph usage maps and per-glyph records. The high compression flag
adds four u32 words (raw size, width, height, reserved zero) and a zlib stream. Pages cover 256 Unicode values;
a wide flag extends the page range. Glyph records carry advance, short/long
kerning entries and a native image cell with clipped geometry, explicit row
stride and optional mask. Official SDK 3.1.2 emits tightly packed rows (including
one-byte strides); the writer uses four-byte strides. Both page and glyph
offset arrays contain cumulative ends, relative to their payload start.
A glyph-count byte of zero denotes all 256 slots. Same-page kerning belongs
in the short table; the runtime ignores same-page targets placed in the long
table, so these noncanonical records are rejected. Native outputs are checked
with SDK 3.1.2 at runtime: every glyph pixel, advance and kerning pair matches
the original source fixture. Coverage is projected from native black pixels and mask;
white opacity/color becomes a diagnosed alpha-weighted darkness projection.
Sixteen-bit PNG samples produce a precision Loss when reduced to Gray8. Native writer output
is checked with an independent consumer wherever available.

Primary [Panic font documentation](https://sdk.play.date/Inside%20Playdate.html#_fonts)
defines source files and runtime behavior. Compiled field descriptions come from
the [original format investigation](https://github.com/cranksters/playdate-reverse-engineering/blob/main/formats/pft.md)
and [image-cell notes](https://github.com/cranksters/playdate-reverse-engineering/blob/main/formats/pdi.md).
SDK compiler output is an external fixture oracle, never implementation code.
The compressed header and row-stride facts above were verified against original
SDK 3.1.2 output, correcting older investigation notes.

## Model, loss, and resource contract

Each file is one strike. Bitmap coordinates use the model’s baseline and upward
positive y; native top-down coordinates are converted explicitly. Unsupported
names, metadata, Unicode sequences, bearings, vertical metrics, and mapping
changes are diagnosed by comparing the source with a native readback. Strict
writes reject every Loss. Grayscale reduction requires an explicit threshold
unless the selected wire layout stores grayscale. No private metadata envelope
is emitted.

Input, tables, mapping expansion, output, conversion scratch, diagnostics, and
writer readback share a cumulative ResourceLimits budget. Counts and products
are checked before allocation; writer output must be readable with the same
limits. Selection uses ordered maps, and mapping trees are traversed at bounded
depth rather than scanning the Unicode address space. Outputs are deterministic.
Original fixtures and external tool observations are recorded in reports/w5.e1.
Reference implementations are external consumers and sources of field facts;
no GPL/MPL implementation is incorporated into the Rust codec.
