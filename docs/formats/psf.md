# Linux PC Screen Fonts (PSF1 / PSF2)

## Basis

https://kbd-project.org/docs/font-formats/font-formats-1.html is authoritative.
The implementation is original Rust; no GPL implementation is copied.

| Field | PSF1 | PSF2 / model |
|---|---|---|
| magic | `36 04` | `72 B5 4A 86` |
| header | mode, byte height | eight LE u32 fields, version 0 |
| count | 256/512 from mode bit 0 | `length`; file order is glyph position |
| geometry | width 8, height=charsize | positive width/height, exact row-byte product |
| raster | top-down MSB-left | same, each row byte aligned |
| Unicode table | mode bits 1 or 2 | flag bit 0 |
| scalar | LE u16, never UTF-16 surrogate pairs | strict UTF-8 scalar |
| sequence marker | LE `FFFE` = `FE FF` | byte `FE` |
| glyph terminator | `FF FF` | byte `FF` |

Aliases precede zero or more nonempty sequences. Sequence-only and unencoded
entries are valid. PSF1 `HASSEQ` implies a table, including legacy mode 4/5;
mode 6/7 is accepted for interoperability. Unknown bits, versions, zero geometry,
inconsistent charsize, truncated headers/rasters/tables, invalid scalars/UTF-8,
empty sequences and trailing bytes are rejected. Extended PSF2 headers are
skipped with a loss diagnostic (not interpreted as pixels). Nonzero unused row
padding bits are normalized with a repair diagnostic.

No table means positional **legacy** codes with explicitly unknown charset,
never guessed Unicode (the generic model's missing-charset fallback is avoided). With a table,
only actual Unicode mappings are added; position is file order, so an empty
entry remains unencoded. Baseline is zero, advance=cell width, ascent=height.

## Writing and loss

PSF2 is the default. Typed options select PSF1, a 256/512 PSF1 count, Unicode
`Auto`/`Include`/`Omit`, an explicit Gray8 threshold and resource/output budgets.
PSF1 automatically pads to 256/512 entries; excessive count, width above eight
or height above 255 is refused. No glyph is silently discarded. Raster cells
cover the font bounds, glyph bounds and advances; negative bearings/baselines
are translated into the cell and reported. Arbitrary PSF2 widths are supported.

Auto includes a table if actual Unicode aliases/sequences exist; otherwise
positional legacy glyphs retain their positions. Include may be used for an
all-unencoded font. Omit reports every discarded mapping/sequence. PSF1 reports
and omits non-BMP scalars and its reserved FFFE/FFFF markers, without truncation.
Malformed model sequences are refused. Table order, aliases and sequences are
stable. Metadata, names and metrics unavailable in PSF are diagnosed. Registry
strict mode rejects loss before any destination is replaced.

Readers account for glyph bookkeeping, decoded pixels and mapping/sequence
indexes against cumulative memory limits, separately from wire table limits.
Writers validate counts and output size before raster/table construction. Codecs
are immutable and safe to call concurrently; no per-cell thread overhead.

## Independent checks and known oracle limitations

Bidirectional monobit 0.54 checks cover plain PSF1 and PSF2 rasters/metrics and
standard labels. That release misidentifies the PSF1 sequence marker as `FF FE`
and its generic sequence splitting/writing does not implement the kbd grammar.
Independent specification fixtures verify complete aliases/multiple sequences,
PSF1 markers and supplementary PSF2 scalars; monobit shortcomings are recorded
without altering correct files to fit the oracle. The supplied catalogue has no
PSF importer (Tamsyn manifest note explicitly excludes PSF); converted output
is checked against its actual BDF importer contract. Reproduction:
`tests/crosscheck/hex_psf.py`.

For conversion into BDF, readers expose the conventional 72-dpi scale with
point size equal to cell height. This is a derived scale convention, not physical
resolution metadata claimed to exist in the source format.
