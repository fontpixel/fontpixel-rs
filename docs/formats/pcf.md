# X11 PCF (`pcf`)

## Sources and compatibility contract

Implement SPEC-v3 §§6, 11.2 and 11.6 against the supplied MIT-licensed
`reference/bdfparser/src/bdfparser/pcf.py` and its tests/goldens. The independent
wire-format reference is X.Org's Portable Compiled Format and bdftopcf 1.1
(libXfont bitmap code); bdftopcf is a test oracle, never a runtime dependency.
Defaults are MSB byte/bit order, glyph pad 4, scan unit 1. Match the reference's
actual tables and bytes, including the final accelerator's overdeclared size.

## Field mapping

- Magic and directory words are little-endian. Each table begins with a
  little-endian format word; other integers follow that table's byte order
- Metrics: signed 16-bit lsb, rsb, width, ascent, descent plus unsigned attributes;
  compressed metrics use five biased bytes. BBX becomes
  `(rsb-lsb, ascent+descent, lsb, -descent)`, DWIDTH `(width,0)`
- Bitmap rows are padded to 1/2/4/8 bytes. Normalize bit order and swap 1/2/4-byte
  scan units on the whole input bitmap block before extracting indexed glyphs
- Encodings map row/column to `(row<<8)|column`; 0xffff is an absent glyph index.
  The core keeps aliases on one glyph and preserves unreferenced glyphs
- Properties retain order, spelling, duplicates and signed integer/string types.
  Decode UTF-8 with per-string Latin-1 fallback. FONT also supplies XLFD
- Optional SWIDTHS supplies `(swidth,0)`; absent entries use SPEC §2.8. Optional
  names fall back to `uniXXXX`, `uXXXXX`, `charN` or `glyphN`
- ACCELERATORS, then BDF_ACCELERATORS, then computed bounds determine font BBX.
  POINT_SIZE/10 is rounded toward positive infinity at ties, PIXEL_SIZE/height
  are fallbacks; resolution defaults to 75. Missing ascent/descent/default
  properties are reconstructed from accelerator/encoding fields
- Ink metrics, attributes, accelerator flags/bounds and unknown tables are kept
  as field/table provenance (accelerator field bytes are normalized to big-endian,
  independently of each source table). The writer recomputes layout and derived fields;
  it never returns a saved input file instead of serializing the model
- PCF has no vertical metrics or nonzero y advance. These and other unsupported
  model values are diagnosed on writing

## Writer choices

Use SPEC's effective source mappings in source order, followed by unencoded
records. Preserve PCF aliases with shared glyph indices. Report shadowed BDF
records, out-of-range mappings and charset-conversion collisions. Keep a glyph
with no representable mapping as unencoded. `ENCODING -1 n` uses n when valid.

Reproduce generated properties and their insertion order, consumed
FONT_ASCENT/FONT_DESCENT/DEFAULT_CHAR, two-pass bounds and direction counts,
terminal-cell reshaping/clipping, ink bounds, metrics compression, zero padding,
and accelerator table sizes. Default output is deterministic and matches Python
and bdftopcf goldens. Nondefault flags intentionally follow Python's corrected
scan-unit ordering and pad-8 flag, rather than bdftopcf 1.1's documented bugs.
Reject scanunit > glyphpad if byte and bit orders differ.

BDF raw hexadecimal rows are interpreted using bdftopcf's explicitly lenient
nibble behavior only while their pixel/dimension snapshot is unchanged. Invalid
hex is reported; the generic model's placeholder raster is never treated as
validated source pixels. Edited pixels supersede that provenance.

## Loss and strict mode

Report dropped >16-bit codes, sequences, vertical/y metrics, non-PCF metadata,
comments, unsupported property types, normalization of names/strings/properties,
terminal reshaping or clipping, stale/unrepresentable provenance and explicit
Gray8 thresholding. PCF reconstructs font bounds and integral point size, so
changes to these values are reported. Conflicting FONT/resolution properties,
non-2.1 BDF source versions and materialized font-level metric inheritance also
produce loss diagnostics. Provenance alone does not waive a loss.
Registry `write_with_report` checks the combined read/write report in strict
mode, before returning output assets. A single PCF stores exactly one strike.

## Resource and transport policy

All offset/count arithmetic, table slices, model budgets and pixel expansion are
checked before allocations. Only actual accelerator data may be shorter than its
TOC size, preserving bdftopcf's last-table convention; truncation of required
fields is always an error. Duplicate tables use the first entry with diagnostics;
unsupported table formats fail. Direct `pcf::read` consumes uncompressed bytes;
registry detection/reading uses the shared bounded gzip transport. The `pcf`
feature enables gzip without enabling BDF capabilities. Writers have explicit
output/model budgets, typed byte/bit/pad/scan choices and no external processes.

## Verified coverage and intentional boundaries

455 committed synthetic byte/read/rewrite oracles include every valid combination
of the four pads, three scan units and independent byte/bit orders. All 139 supplied
PCF files and both 107/800-entry hash manifests are checked externally, including
hash-only fonts; see `tests/crosscheck/pcf*.py`. 512 seeded fresh-model property
cases confirm that the writer is independent of source-file caches. Tests also
cover optional tables, truncation at every byte of a complete small font, count/
offset attacks, limits, unterminated/Latin-1 strings, partial scan units, shared
aliases, metadata edits and strict/report behavior.

The core deliberately retains shared PCF aliases and attributes; the old Python
model expands aliases and omits attributes. Compatibility code can project those
core fields into legacy records later. The writer is canonical, not an arbitrary
layout-preserving PCF editor: custom table order/padding is normalized, unknown
tables are diagnosed and omitted, and custom ink/accelerator values are recomputed.
Unsupported extensions/format variants fail with an error. Duplicate table data
cannot be represented by a single effective table and is reported as read loss.

Unknown declared PCF charsets retain legacy code identity; missing glyph names
use `charN`, exactly as the PCF reference does. This is separate from BDF's
compatibility projection for unknown identity-fallback codecs. Accelerator loss
comparison is field-wise across independently mixed table byte orders, so a
change of output byte order cannot hide overwritten custom accelerator values.
