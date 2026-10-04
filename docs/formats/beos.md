# BeOS |Be; bitmap fonts

W5.B research recorded before implementation, 2026-10-04.

Primary source: [BeOS |Be; bitmap fonts authority](http://www.eonet.ne.jp/~hirotsu/bin/bmf_format.txt). Supplementary executable format evidence is MIT monobit 0.54, module `beos`; the installed release fingerprints and exact external-oracle scope are recorded in `reports/w5.b-crosscheck.json`.

Big-endian 36-byte header, family/style lengths and null-terminated Latin-1 strings, sparse 8-byte Unicode location-hash entries, 24-byte per-glyph records with signed bounding edges and IEEE-754 advances, and row-aligned four-bit coverage. Empty hash entries use pointer 0xffffffff. Hash capacity, collisions, aliases and unencoded physical glyph records must all be bounded. Finite fractional metrics are preserved as raw float properties with explicit integer projection; NaN/infinity are rejected.

One strike; 16-bit Unicode map; explicit grayscale quantization to the selected palette; metadata and unused floating fields retained. The primary note was retrieved over HTTP after the HTTPS endpoint failed; MIT monobit provides a separate executable compatibility oracle.

All reads budget retained input, cumulative fonts/glyphs, table allocations, decompressed pixels and diagnostics before allocation. Every successful write is reopened under identical limits and compared field by field. Source resources and external fonts are never fetched implicitly. Synthetic fixtures are original MIT artwork; third-party tools remain outside the repository.

The original note was successfully fetched over **HTTP** on 2026-10-04 (the web proxy's forced HTTPS request still fails); raw UTF-8 evidence is retained externally and fingerprinted in the report. Its bitmap paragraph explicitly says eight levels 0..7 in four-bit storage. `Native8` therefore scales those levels to Gray8 using nearest `255*n/7`; an explicit `FullNibble` compatibility policy supports monobit's 16-level interpretation. Writer quantization requires opt-in whenever a Gray8 sample is outside the selected exact palette. Default native hash capacity is the smallest power of two at least equal to the number of mapped characters. Empty glyphs have left=top=0 and right=bottom=-1, with no pixels. Hash placements are checked for reachability across occupied collision chains in linear time. Writer placement uses a next-free disjoint-set table to avoid quadratic collision probing.
