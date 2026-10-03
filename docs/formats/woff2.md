# WOFF 2.0 design (W2.5)

Design recorded before implementation, 2026-10-03.

## Authority and boundaries

- W3C Recommendation: <https://www.w3.org/TR/WOFF2/>, especially §§3–7.
- RFC 7932 Brotli; pure-Rust `brotli` crate, compression quality 11 by default,
  configurable 0–11. FONT mode, deterministic data and options.
- fontTools' MIT WOFF2 implementation is an independent interoperability oracle
  and reference for the documented triplet equations; attribution is recorded.

`woff::woff2` operates on complete sfnt bytes. It supports all standalone sfnt
flavors accepted by W2.1 and both transformed and untransformed glyf/loca, plus
optional hmtx transforms. CFF, bitmap and unknown tables use the null transform.
W2.7 now adds native [FontSet raster/vector codecs](vector-raster.md), plus
[TTC/OTC collection envelopes](ttc.md) when `ttc` is enabled. Collection directories
permit repeated tags across faces, preserve table sharing and face order, and
require paired glyf/loca transforms. Per-face nominal metrics keep legal hmtx null
transforms. The standalone W2.5 semantics below remain unchanged.

## Fields and transform mapping

| Field | Meaning / validation |
|---|---|
| header + known/custom tags | WOFF2 header, unique bounded directory, shortest UIntBase128 encoding |
| original sfnt/glyf lengths | advisory only when transformed; actual reconstructed output is bounded |
| transform length | exact decompressed block accounting, loca is zero |
| glyf version / options / numGlyphs / indexFormat | transform 0; overlap bitmap; count and short/long loca |
| nContour / nPoints | signed contour count and 255UInt16 lengths |
| flag / glyph | all 128 triplet encodings, on/off-curve flags and instruction lengths |
| composite | exact component bytes, including placement, transform and semantic flags |
| bbox | original bounds when not inferable, mandatory for composites |
| instruction | retained opaque TrueType programs, never executed |
| hmtx transform 1 | omit either/both matching LSB arrays, reconstruct from xMin (0 for empty glyphs) |
| metadata / private | separately Brotli-compressed metadata and opaque private bytes |

The encoder always applies glyf/loca transforms when glyf exists unless the
caller explicitly disables them for comparison or exact table-byte preservation.
Triplets are emitted in their shortest defined class. Original bounding boxes,
hint programs and overlap markers are retained. Zero-contour glyphs with nonzero
bounds or instruction programs cannot be transformed losslessly and are rejected
with a clear error (the caller can explicitly select the null transform).
Short loca reconstruction uses optimal point flag runs to avoid gratuitous growth;
if the mandated short offsets cannot hold reconstructed glyph bytes, decoding
fails rather than changing indexFormat. Every WOFF2 output marks head flags bit 11
(lossless font-data transformation), including CFF and explicit null-transform
output, as required by WOFF2 §5. A null transform preserves table payload bytes
except for this required head flag and reconstructed checksum adjustment. Digital signatures cannot remain valid after
transforms/reordering: DSIG is removed with an explicit report entry.

Unknown/reserved transform versions, missing/mismatched table pairs, inconsistent
counts, unused transform stream bytes, decompression size mismatch, overflows and
invalid block layouts fail. WOFF2 header reserved is ignored as required. Declared
original sizes never control transform allocation. Limits cover input bytes,
actual decompressed/reconstructed bytes, table size, points, components and
instruction bytes. Malformed optional metadata follows WOFF1's report/omit policy.
No XML schema-validation claim is made.

## Speed strategy

Implement ordered, bounded multi-font compression with an explicit worker count;
never create a thread per font. Lower quality is caller-controlled. Research
single-font parallel Brotli only where the encoder produces one legal Brotli
stream; concatenating independent streams is invalid WOFF2. Benchmark five-run
wall-clock medians and output size on Tong square/CFF and round-dot/TrueType,
compare native Brotli and fontTools/Google Brotli, and record CPU/memory limits.
Do not change the required default quality to make timing claims look better.

## Verified interoperability details

For transformed output, normalize sfnt glyph records to four-byte boundaries
before deriving original glyf/font lengths (upgrade loca/head format if needed).
This is required for consistent deployed Google/FreeType behavior even though
W3C calls those lengths advisory. Compressed data receives final zero padding.
The native decoder still accepts valid unpadded final blocks and advisory-size
mismatches, and never uses those fields as allocation authorities.

fontTools 4.66.1 recompiles transformed hmtx to a minimal long-metric count without
updating exposed hhea bytes. For nonminimal source counts our encoder preserves
hmtx with a null transform; all original bytes and metrics remain available.
The reader continues accepting a transformed nonminimal count correctly.

Brotli 9.0.0 supports a legal single-stream parallel encoder. The explicit
`compression_threads` option uses that implementation, bounded to 1–16 threads;
independent multi-font jobs are separately bounded (product at most 64). Both
strategies retain deterministic results for fixed inputs/options. Quality remains
11. After the combined Phase 2 performance review, the default is two intra-font
workers for streams at least 128 KiB; smaller streams use the serial path to
avoid worker/dictionary overhead. This input-size threshold is deterministic,
not dependent on host CPU count. `--woff2-threads 1` keeps the serial choice.
No parallel feature is imposed on BDF/PCF.
