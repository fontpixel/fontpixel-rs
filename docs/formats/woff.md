# WOFF 1.0 design (W2.5)

Design recorded before implementation, 2026-10-03.

## Authority and layering

- W3C Recommendation: <https://www.w3.org/TR/WOFF/> (header, directory,
  zlib table compression, extended metadata and private-data blocks).
- OpenType sfnt tables retain their existing semantics. `woff::woff1` wraps or
  unwraps an arbitrary supported standalone sfnt, including bitmap-only and CFF
  fonts. It never rasterizes outlines or projects arbitrary sfnt into FontSet.
  W2.7 adds separate native [FontSet reader/writer hooks](vector-raster.md).
  WOFF1 itself supports standalone fonts only; collections use TTC or WOFF2.

## Fields and mapping

| Wire field | API / validation |
|---|---|
| signature, flavor | wOFF, existing sfnt Flavor |
| length, numTables, reserved | exact file extent, bounded nonzero count, zero |
| totalSfntSize | exact canonical sfnt directory + aligned table-size sum |
| major/minorVersion | retained WebFont version, deterministic writer option |
| tag, offset, compLength, origLength, origChecksum | unique sorted tags, aligned non-overlapping blocks, bounded decompression, exact lengths/checksums |
| metaOffset/Length/OrigLength | bounded separately compressed metadata bytes |
| privOffset/Length | opaque private data, retained without interpretation |

The decoder reconstructs a tag-sorted sfnt directory while preserving physical
table order and recomputing head.checkSumAdjustment. Compression uses deterministic pure-Rust zlib; a table
is compressed only if smaller, otherwise it is stored verbatim. Compression
level is configurable 0–9. Unknown sfnt tables remain byte-identical.

## Fidelity, malformed input, and losses

Reordering tables, normalizing padding and correcting checksum adjustment can
change the sfnt envelope bytes, not table semantics. Bad source or container table checksums, source whole-font checksums, spans,
compression streams or declared lengths fail. Input/output/table/decompression
budgets are applied before allocation. Optional blocks are preserved separately
from sfnt; passing only the sfnt bytes to an unrelated writer discards them by
caller choice. Metadata is opaque UTF-8/XML content: transport decoding is
validated, but this library does not claim XML schema validation or display it.
A bad compressed metadata payload is reported and omitted rather than making an
otherwise usable font fail, as required of WOFF consumers. Writer accepts raw
metadata with an explicit caller obligation to provide valid WOFF XML.

Collections cannot be represented by WOFF 1.0. A TTC input is rejected.

## Physical order and digital signatures

WOFF1 §6 requires the input's physical table order to survive packaging even
though directory entries remain tag-sorted. Both directions use an explicitly
order-preserving sfnt assembler. For well-formed input with ordinary padding,
standalone sfnt bytes (including a DSIG table) roundtrip exactly. Non-table data
cannot be carried by WOFF; nonzero discarded bytes produce a loss report.
Checksum-valid directory/padding normalization is reported, and a DSIG invalidated
by normalization is removed with an explicit loss report. Source checksum errors
are rejected before normalization, following the explicit WOFF1 §5 MUST-reject
requirement; the later advisory wording about correcting checksums does not
relax that strict default. Every WOFF1 table, including the last, must contain
its full zero padding to a four-byte boundary. No signature validity or
cryptographic verification is claimed.
