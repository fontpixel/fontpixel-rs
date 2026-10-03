# sfnt common tables (W2.1)

## Authority and scope

Written before implementation. The normative sources are Microsoft's OpenType
1.9.1 specification: [file/container](https://learn.microsoft.com/en-us/typography/opentype/spec/otff),
[name](https://learn.microsoft.com/en-us/typography/opentype/spec/name),
[OS/2](https://learn.microsoft.com/en-us/typography/opentype/spec/os2),
[cmap](https://learn.microsoft.com/en-us/typography/opentype/spec/cmap),
[post](https://learn.microsoft.com/en-us/typography/opentype/spec/post),
[hhea](https://learn.microsoft.com/en-us/typography/opentype/spec/hhea),
[hmtx](https://learn.microsoft.com/en-us/typography/opentype/spec/hmtx),
[vhea](https://learn.microsoft.com/en-us/typography/opentype/spec/vhea),
[vmtx](https://learn.microsoft.com/en-us/typography/opentype/spec/vmtx),
[gasp](https://learn.microsoft.com/en-us/typography/opentype/spec/gasp),
[head](https://learn.microsoft.com/en-us/typography/opentype/spec/head), and
[maxp](https://learn.microsoft.com/en-us/typography/opentype/spec/maxp).
Checked 2026-10-02. Fontations `read-fonts` is the typed parser, not an independent
validation oracle. fontTools supplies the public OS/2 Unicode range definition
and Macintosh standard glyph-name data (MIT, attributed separately); production
code never invokes Python. No FontForge code or its code-page heuristic is used.

This is a public, feature-gated `sfnt` building block, not a registered bitmap
codec. It reads borrowed table records and typed common tables and assembles
caller-supplied tables. Higher-level CFF, glyph, bitmap, layout and webfont modules
remain separate. Single-font SFNT versions 0x00010000, OTTO and legacy `true` are
accepted here. W2.7 adds `sfnt::collection` for TTC/OTC and the independent
[vector-raster FontSet codecs](vector-raster.md), without changing this low-level
borrowed-container contract.

## Fields and internal meaning

| Wire data | Public representation / policy |
| --- | --- |
| sfntVersion, table directory | `Flavor`, `TableRecord`, `Sfnt`; unknown table bytes remain available |
| tag/checksum/offset/length | big-endian, checked bounds and overlaps; deterministic sorted writer, four-byte zero padding |
| head.checkSumAdjustment | zeroed for head checksum; final font sum is 0xB1B0AFBA |
| head/maxp | exposed through read-fonts; caller owns outline-dependent values |
| name platform/encoding/language/name ID | explicit records plus raw encoded bytes; Unicode convenience constructor uses UTF-16BE; format 1 language tags retained |
| OS/2 versions 0–5 | every defined field retained in an owned typed structure; version controls extension lengths |
| Unicode ranges | static OpenType blocks, OR all matching bits, non-BMP sets bit 57; scalar-only inputs |
| code-page ranges | separate explicit helper requires full printable repertoire coverage for selected supported native charsets, not a one-character guess |
| cmap formats 4/12 | scalar→nonzero u16 glyph ID; full Unicode always format 12, BMP also format 4 when representable |
| hhea/vhea | signed ascent/descent/gap and bearings, unsigned max advance, caret vector/offset and metric count |
| hmtx/vmtx | glyph-order advance/bearing pairs; optionally compact identical trailing advances |
| post | header fields plus explicit named format 2 or nameless format 3; reads format 1/2/3 |
| gasp | version 0/1, ordered inclusive ppem ranges with final 65535 sentinel |

All measurements here are already font design units. Higher-level codecs must
choose pixel scaling, glyph order, `.notdef`, names/style mapping and report
unrepresentable source fields. No common-table function silently converts model
units, discards invalid Unicode, clamps integers, or invents outlines.

## Ambiguity and validation

Structural parsing is separate from checksum validation: fonts encountered in
practice can have stale checksums or unsorted directories. Reading checks the
entire directory and all table spans before exposing them, rejects duplicate tags,
alias/overlap and out-of-file spans, and applies configured byte/count budgets.
Explicit checksum validation also checks alignment, sorted tags, search fields,
zero padding, every table checksum and the whole-font adjustment. This validates
the sfnt envelope, not glyph program safety or full OpenType conformance; OTS
remains an independent crosscheck.

The Unicode cmap reader selects Windows full Unicode, Unicode full repertoire,
then BMP records. It does not accidentally prefer a symbol/Mac legacy cmap.
Unsupported mapping formats and variation selectors remain accessible as raw and
read-fonts tables; a nominal map is not a claim to preserve UVS or legacy maps.
Full Unicode format 12 is always complete. If a BMP map cannot fit the 16-bit
format-4 length, the writer omits the entire format-4 subtable and explicitly
returns `has_bmp_subtable = false`, never a silently truncated map. U+FFFF is
representable in format 12; the format-4 terminal segment remains unmapped.

Name serialization sorts keys and shares identical string storage; callers may
retain non-Unicode bytes without lossy decoding. UTF-16 decoding is fallible.
Format-1 language tags are Unicode strings; invalid language indexes fail.
PostScript glyph-name writes use ASCII letters, digits, dot and underscore with
a maximum of 63 bytes, per the OpenType specification; reads also retain legacy
printable, delimiter-free Pascal names up to 255 bytes. Rewriting an incompatible
legacy name errors rather than renaming it silently. Duplicates may share a string index. Format 3 must be requested
explicitly (by omitting names). Unsupported historical post variants stay raw.

OS/2 Unicode bit 57 is set for any supplementary scalar; invalid source numbers
and surrogates error. Code-page coverage excludes Unicode control characters but
requires every other assigned code point in the requested native encoding.
Unsupported/uncompiled code pages error rather than silently claiming coverage;
OEM/symbol and unlisted legacy bits must be supplied explicitly by the caller.
This conservative policy differs intentionally from fontTools' heuristic. No
range or code-page bits assert that the font provides shaping support.

## Information loss and compatibility

Raw unknown tables and name-record bytes can be round-tripped through the table
builder; packing offsets, string sharing, padding and checksums are normalized.
An arbitrary table may contain absolute offsets or depend on glyph IDs: preserving
its bytes does not make a glyph-remapping operation safe. Such operations must
explicitly rewrite/drop it with diagnostics in later work items. Typed writers
only write their documented versions/fields; callers seeking unknown extension
preservation must keep raw bytes. Explicit post format 3 does not retain glyph
names. Nominal cmap projection excludes legacy maps/UVS and zero glyph mappings.

The SPEC compatibility TTF writer retains its historical table-specific choices
and byte sequence (including its narrower cmap policy). Only deterministic sfnt
assembly/checksum mechanics are shared. New native policy must not alter the
phase-1 byte-for-byte compatibility contract.
