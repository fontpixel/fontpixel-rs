# Embedded monochrome/grayscale sfnt (`otb`)

## Sources and scope (2026-10-03, documentation before implementation)

Primary specifications: Microsoft OpenType
[EBDT](https://learn.microsoft.com/en-us/typography/opentype/spec/ebdt),
[EBLC](https://learn.microsoft.com/en-us/typography/opentype/spec/eblc), and
[EBSC](https://learn.microsoft.com/en-us/typography/opentype/spec/ebsc).
The read-only task-package Tong `tools/export-rs/src/otb.rs` is an independent
interoperability oracle; it is not the implementation of the native codec.
Shared sfnt assembly, names, metrics, cmap and checksums use W2.1.

Table APIs operate on glyph IDs, allowing callers to attach strikes to existing
TTF/OTF outlines without changing glyph ordering. The `otb` native codec reads
embedded EBDT/EBLC strikes and writes a bitmap-only sfnt. It does not synthesize
outlines. W2.7's [unified outline reader](vector-raster.md) now prefers matching
embedded images and supplies supported missing-glyph outline fallback. An explicit
`otb` request still projects only the requested bitmap family and reports omitted
tables/outline fallbacks.

## Wire mapping

- A 48-byte size record supplies two ppem values, depth, layout flags, glyph range,
  and two 12-byte line records. Each strike becomes a separate Font.
- Index formats 1/3 use successive 32/16-bit offsets; 2 shares metrics and fixed
  image size; 4 supplies sparse glyph/offset pairs; 5 sparse IDs and fixed size.
  Offsets are relative to the structures specified by the standard, not sfnt.
- Images 1/2 carry five-byte metrics; 6/7 eight-byte metrics; 5 shares metrics.
  Formats 1/6 pad each row; 2/5/7 concatenate rows. Bits run most-significant first.
- Images 8/9 reference component glyphs and signed top-left offsets. They resolve
  to pixels, with checked graph depth/work and cycle detection; overlap uses bitwise OR of coverage samples (verified against FreeType for
overlapping gray components). Out-of-bounds components are rejected, not clipped.
- Horizontal bearings map to BBX `(x=bearingX, y=bearingY-height)`; advance maps
  to `(advance,0)`. Vertical big metrics map to a downward advance and an explicit
  vertical origin. Small vertical metrics have swapped bearing axes per the spec;
  no fabricated horizontal advance is claimed.
- Depth 1 stays packed Mono; depths 2/4/8 expand exactly to Gray8 by multiplying
  by 85/17/1. Writing may use any of these depths only when all values are exact.
- EBSC records preserve target/source ppem and both line records. They are scaling
  instructions, not extra physical strikes: reading does not invent a resampler.

## Canonical writing and ambiguity

Use version 2, index 1, and byte-aligned image 1 or 6, splitting metric kinds into
separate contiguous ranges. Sparse gaps get zero-length offsets. Both zero-area
and nonempty all-white glyphs remain explicit images. The table API preserves
line records; the FontSet codec derives their available fields. Counts, uint8/
int8 metrics, input/table/output/model bytes, pixel work and graph visits are
bounded before allocation. Unsupported image 3/4, nonzero colorRef, unknown
versions/flags, overlapping or unsorted index ranges and missing dependencies
produce errors. No partial successful font is returned.

All strikes in a native write share glyph slots/cmap; inconsistent mappings are
rejected rather than guessed. Missing glyphs in one strike are represented as
sparse ranges. The encoder reserves glyph zero for .notdef and keeps aliases and
unencoded glyphs. Global design metrics use an explicit 100 units per reference
pixel convention (as Tong), while strike metrics remain exact pixels.

## Loss policy

Canonical encoding changes table/index layout and expands composite graphs.
Native reading preserves original common/unknown sfnt bytes as provenance only
where documented by the implementation; it reports unprojected semantics.
Native writing reports unsupported source properties, metrics, names, sequences,
legacy codes and duplicate mappings individually rather than silently deleting
information. Strict registry output rejects accumulated conversion loss. Native
pixel/metric round trips are distinct from arbitrary source-file byte equality.

## Independent implementation findings

FreeType 2.13.3 misaligns the second row of width-3 bit-aligned 2/4-bit fixtures;
fontTools and the native reader agree with the specification's contiguous packing.
The integration script asserts the exact external discrepancy, rather than
weakening all pixel comparisons. Canonical writes are byte-aligned and their
pixels match FreeType at every tested depth. OTS 9.2.0 rejects bitmap-only sfnt
(`no supported glyph data table(s) present`) and strips EBDT/EBLC when sanitizing
an otherwise valid outline-bearing container. OTS cannot validate these bitmap
payloads; fontTools, FreeType, monobit and structural tests provide that evidence.

Global metric design scale is 100 units per reference pixel, or 64 for a reference
ppem above 163 so unitsPerEm stays legal. Sparse glyphs absent from the first
strike take their design metrics from the first strike containing that glyph;
anisotropic/scaled inexact conversions are reported. Pixel data remains exact.
Those are the default metric policies. Explicit UPM, reference-strike priority
and pixel-to-design rounding are available through `EmbeddedWriteOptions` and
the shared CLI `--otb-*` controls; see the [API contract](../api.md#explicit-output-names-and-embedded-metrics).
Metric-reference selection does not change the codec's physical strike ordering;
Apple/bdat retains its separate canonical ppem sort.
The native reader preserves original table bytes as `sfnt.table.*` container
provenance; a rewrite reports their replacement instead of allowing stale bytes
to override edited model fields. This is not a byte-identical arbitrary-sfnt
roundtrip. Original glyph ID metadata is also provenance, not an instruction to
ignore edited glyph labels. Gray threshold is optional and never implicit.
