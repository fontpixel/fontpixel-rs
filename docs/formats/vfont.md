# Berkeley vfont

Specification consulted before implementation: [SunOS 4.1.3 vfont(5)](https://man.freebsd.org/cgi/man.cgi?apropos=0&manpath=SunOS+4.1.3&query=vfont&sektion=5), the original Sun manual mirrored by FreeBSD; [BRL-CAD's vfont documentation](https://brlcad.org/docs/api/d2/d3b/group__bu__vfont.html) confirms signed character metrics and MSB-left rasters. No GPL implementation is used.

| Wire field | Model |
|---|---|
| magic 0436; five 16-bit header words | little or big endian, magic detects the order |
| size | checked bitmap section length, at most 65535 bytes |
| maxx/maxy/xtend | signed original `vfont.*` properties; extrema are informational |
| 256 ten-byte dispatch entries | legacy codes 0–255, ascending file order |
| signed up/down/left/right | BBX width=left+right, height=up+down, x=-left, y=-down |
| signed width | advance.x; left bearing=-left; right bearing=width-left-right+left |
| addr/nbytes | checked bitmap offset/length; rows are MSB-left and byte aligned |

All-zero dispatch records are absent; zero-byte records with nonzero metrics represent blank spacing glyphs. Other zero-byte records retain their declared blank rectangle. Negative dimensions, negative lengths and truncated bitmaps fail. Extra bitmap bytes and trailing bytes survive in original source bytes; native writes canonicalize layout and disclose any source-property change. Shared raster regions are allowed and charged separately for each decoded glyph. Global ascent/descent, bounding box, line height, pixel size, spacing and space width are derived from decoded glyph metrics rather than the historically unused extrema header.

Writing defaults to little endian; explicit big endian is supported. Source extrema/xtend properties are retained. Legacy mappings only are accepted by default; `unicode_as_legacy` explicitly permits numeric Unicode projection. Unencoded glyphs, aliases/duplicate mappings, unsupported metadata and metrics, changed glyph order and source layout are reported by complete target-model comparison. Gray8 needs an explicit threshold. The format limits offsets to u16, individual byte counts to i16, bearings to i8 and advances to i16. Out-of-range geometry fails with the accumulated report. Output is decoded under the identical limits before success; aggregate budgets include source model, output, decoded target, index and diagnostic storage.
