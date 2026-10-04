# GNU Unifont HEX

## Basis and scope

- GNU Unifont format description: https://unifoundry.com/unifont/index.html
- Original description: https://czyborra.com/unifont/
- PLAN/FORMATS require 8/16/24/32-wide, 16-high records. `HexReadOptions`
  and `HexWriteOptions` explicitly select another positive height. Height is
  never guessed from ambiguous byte counts. No GPL implementation is copied.

| Source | Model |
|---|---|
| hexadecimal scalar before `:` | one Unicode codepoint |
| hexadecimal bytes after `:` | top-down, MSB-left, byte-aligned rows |
| length / selected height | width, one of 8/16/24/32 |
| file order, repeated codes | ordered glyphs, last-mapping lookup |
| absent metrics | cell advance = width, origin `(0, -descent)` |

The default descent is zero because the file has no baseline. A caller can set
`descent` for a known font convention; height/descent must agree on roundtrip.
Blank lines, ASCII whitespace around fields, CRLF, a leading UTF-8 BOM and `#`
comment lines are accepted. Comments are retained as `hex.comment` headers.
Codepoints must be Unicode scalars (noncharacters are valid). Bad hex, extra
colons, truncated rows and unsupported widths are errors with line numbers.
Duplicates are retained with a warning, not silently coalesced.

## Writing and loss

Output is uppercase, at least four hexadecimal codepoint digits, LF, file-order
records. Aliases produce repeated raster records and report the lost shared
identity. Unencoded glyphs, non-Unicode/invalid mappings and sequences are
reported individually. Names, unsupported properties and metrics are reported;
`--strict` rejects all loss before replacing a destination. Whole-cell alignment
preserves pixels; cells use the selected height/descent, x=0, and the smallest
allowed width holding both the advance and raster. Ink outside that vertical
cell or to its left is refused rather than silently clipped. Gray8 requires an
explicit threshold and reports conversion. Empty fonts produce empty HEX and
need an explicit format or filename hint when read.

Input, text-record, glyph, pixel, cumulative decoded/model-memory and output
budgets are checked before allocations. Comment output is subject to the same
per-record cap as raster records. When gzip transport is enabled, retained
compressed bytes, decoded bytes and model charges share one aggregate budget. No per-glyph threads are created:
these tiny records are a linear byte task; independent fonts can be converted
concurrently with the same immutable codec.

## Interoperability

monobit 0.54 only understands 8/16-wide HEX and guesses nonstandard heights;
its 24/32-wide interpretation is not the PLAN-required fixed-height extension.
Crosschecks therefore cover its actual standard subset; extension widths and
explicit-height ambiguity have independent byte fixtures/property tests.
There is no corresponding importer in the supplied catalogue pipeline (see
manifest entry `trevorld--hexfont`); the BDF import contract is exercised after
conversion instead. Reproduction: `tests/crosscheck/hex_psf.py`.

For conversion into BDF, readers expose the conventional 72-dpi scale with
point size equal to cell height. This is a derived scale convention, not physical
resolution metadata claimed to exist in the source format.
