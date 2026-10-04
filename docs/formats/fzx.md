# FZX proportional bitmap fonts

Primary specification consulted before implementation: [FZX v1.0 specification by Andrew Owen (2013), distributed by z88dk](https://github.com/z88dk/z88dk/blob/master/libsrc/font/fzx/README.md). Its worked addresses clarify that each offset is relative to its own table entry, not the table origin. The standard is royalty-free. Implementation is original; monobit (MIT) is an independent oracle and was consulted for interoperability.

| Wire field | Model |
|---|---|
| byte height | baseline distance / line height; baseline at the bottom of that cell |
| byte tracking | unsigned horizontal gap, retained as `fzx.tracking` |
| byte lastchar | legacy codes 32 through lastchar, including empty entries |
| LE word, lower 14 bits | data offset from this entry's own position |
| word upper 2 bits | kern; BBX x and left bearing = -kern |
| byte high nibble | shift from cell top (0–15) |
| byte low nibble plus one | width 1–16; one or two bytes per raster row |
| final LE word | relative offset from this final word to the byte after all rasters |
| glyph extent | next absolute offset minus current offset; exact whole rows required |

BBX y is height-shift-raster_height; advance is width-kern+tracking. The header has no charset declaration: codes remain legacy and conversion to Unicode requires external charset knowledge. `unicode_as_legacy` is an explicit write projection. Tracking uses the full unsigned byte specified by the driver; monobit 0.54 exposes it as signed. Normal-range tracking is inherited into effective glyph advances; oracle checks compare those advances directly. Monobit collapses zero-height rasters to zero width, so comparison normalizes only zero-area rectangles, retaining their advance and all ink coordinates.

Trailing bytes, malformed/backward offsets, overlapping tables and partial raster rows fail. Padding bits are canonicalized with loss reporting. Writing preserves widths and pixel positions when representable, inserts top blank rows for shifts above 15 and left blank columns for positive bearings, represents width-zero spacing glyphs with legal width/kern combinations, fills range holes with width-one empty glyphs, and reports these geometry/identity changes. Normalized widths outside 1–16, negative bearings below -3, negative top shift, offsets beyond 14 bits and non-byte header metrics fail explicitly. A global tracking override is available; otherwise the retained value is used or a consistent per-glyph value is inferred. All unsupported model fields are audited against a reread of actual output. No extra dependencies; reader and writer share the same limits and cumulative accounting.
