# PC/GEOS 2+ BSWF fonts

W5.B research recorded before implementation, 2026-10-04.

Primary source: [PC/GEOS 2+ BSWF fonts authority](https://github.com/bluewaysw/pcgeos). Supplementary executable format evidence is MIT monobit 0.54, module `pcgeos`; the installed release fingerprints and exact external-oracle scope are recorded in `reports/w5.b-crosscheck.json`.

BSWF file header and FontInfo metadata lead to point-size entries and per-strike FontBuf sections. Native point size and multiple metrics use byte-fraction fixed-point values. Character tables reference signed-offset raster records; bitmap dimensions are bytes. KernPair and kern-value pointers are distinct tables and must be validated independently. Source field declarations are format authority; GPL production implementation code is not copied.

All native strikes and kerning associations must be retained in model properties. Fractional values are preserved exactly, with any integer geometry projection diagnosed. Bitmap v2+ is the specified scope; old non-BSWF and outline-only resources fail explicitly. Common family/name/ID, point size, style and default-character choices are audited.

All reads budget retained input, cumulative fonts/glyphs, table allocations, decompressed pixels and diagnostics before allocation. Every successful write is reopened under identical limits and compared field by field. Source resources and external fonts are never fetched implicitly. Synthetic fixtures are original MIT artwork; third-party tools remain outside the repository.

## Native layout corrections established before implementation

Pinned original declarations are `Include/Internal/fontDr.def` and `Library/Kernel/Graphics/graphicsConstant.def` in [the original PC/GEOS source](https://github.com/bluewaysw/pcgeos). `FontFileInfo` is 8 bytes. On-disk `FontInfo` excludes its 2-byte in-memory file handle and is **33 bytes** for SBCS, **57 bytes** for DBCS. Thus an in-memory point-table offset becomes a file offset by adding **6**, not 7. Native SBCS point entries are 10 bytes: style, three-byte WBFixed (fraction first, then signed integral word), size word, file offset dword. DBCS entries are 9 bytes: style, signed point-size byte, charset byte, size word, file offset dword. Major 1 uses SBCS and major 2 uses DBCS.

Native FontBuf is 71/70 bytes before the character array; descriptors are 8/6 bytes. WBFixed stores fraction byte plus signed integer word. Character bounds remain byte dimensions and signed byte offsets. Native flag bits are negative LSB 0x40, above 0x20, below 0x10, no-data 0x08, first kern 0x04, second kern 0x02, invisible 0x01. KernPair places the **right** character first, followed by the left; values are separately addressed signed 8.8 fixed-point numbers. Both pair and value tables are retained and independently bounded. Fractional point size is exact in the public rational metric; fractional integer-only metrics require explicit round-to-even permission and are retained exactly in native properties.

Monobit 0.54 instead introduces a trailing FontInfo padding byte, seeks point-table+7 and models a two-byte point size plus padding. Its size/data pointers consequently remain aligned, but it misinterprets native styles and fractional point sizes. Its named nonzero CharTableFlags also reverse the original bit order except no-data. Crosschecks must state those limitations; production follows the native declarations. No GPL implementation code is copied.

The codec preserves every native FontBuf entry as a separate model font, including DBCS charset sections; it does not infer section coalescing from equal point/style fields. DBCS codes are Unicode words and reject surrogates. FontBuf and relative pointers are limited to their native word sizes; region/outline-generated buffers are explicit errors because they do not contain the bitmap CharData layout. The earlier PC/GEOS 1.0/1.2 signatures (`80 10` / `EF BE`) are not BSWF resources and are rejected; no public field-complete declarations or independent runnable reader for those earlier layouts were found in the pinned survey sources. The covered native BSWF families are major 1 SBCS and major 2 DBCS.
