# Apple IIgs QuickDraw II bitmap fonts (`iigs`)

Specification written before implementation (2026-10-04). The codec reads and
writes standalone ProDOS type $C8, auxiliary type $0000 font data forks.

Primary specification: Apple, *Apple IIgs Toolbox Reference*, volume II, chapter
16, pp.16-41–16-54 ([scan](https://mirrors.apple2.org.za/ftp.apple.asimov.net/documentation/programming/65816_gs/Apple%20IIGS%20Toolbox%20Reference%20Volume%202.pdf));
volume III, chapter 43, p.43-5 (version 1.5 extended offset; the August 30, 1989
beta-draft [scan, PDF page 460](https://mirrors.apple2.org.za/ftp.apple.asimov.net/documentation/programming/65816_gs/Apple%20IIGS%20Toolbox%20Reference%20Volue%203%20part%201.pdf)).
The independently implemented MIT-licensed monobit 0.54 codec is an interoperability
oracle, not the definition of the wire format. No Apple font artwork is included.

| Wire field | Model or preservation |
| --- | --- |
| Pascal family name (before font record) | MacRoman family name |
| offsetToMF (word count), extension words | validated native header; source bytes |
| family, style, version, fbrExtent | `iigs.family-id`, `iigs.style`, `iigs.version`, `iigs.fbr-extent` |
| size | integral point size |
| optional v1.5 owTLocHigh | high 16 bits of unsigned word offset |
| fontType | ignored for IIgs raster interpretation; retained in source |
| firstChar, lastChar | explicit legacy byte codes; optional declared charset |
| widMax, kernMax, nDescent, fRectWidth/Height | native source and font/glyph geometry |
| signed ascent, descent, leading | font ascent/descent/line gap; negative values are legal |
| rowWords and bitImage | one-bit MSB-first, rows aligned to 16 bits |
| locTable | increasing bit positions; includes missing symbol and end |
| owTable | little-endian words: low byte advance, high byte offset from kernMax |
| missing symbol | unencoded default glyph |
| native bytes | `iigs.source`, enabling exact unchanged native roundtrip |

The fixed fields and tables are little endian; raster bytes are not word-swapped.
Apple volume II p.16-48 explicitly permits rectangles entirely above or below
the baseline. Ascent, descent, kernMax, nDescent and leading are signed words;
canonical output does not force the rectangle to include the origin. Raster
height supplies the model's pixel size. Defined width/offset bytes are 0–254;
the pair FF/FF denotes an absent character, and partial FF values are invalid.
The offset/width table offset is relative to its own header field, not the start
of the file. The location table immediately precedes the offset/width table;
legal padding between the raster and the two tables is retained. Version 1.5
extends that offset in its extra header word. Writers
choose 1.1 unless 1.5 is needed; explicit versions are validated. Character codes
remain legacy values; a Unicode mapping requires explicit charset conversion.

The reader validates ranges, monotone locations, absent-character locations,
missing symbol, terminators, table bounds and multiplication before allocation.
Unknown header extension/padding/trailing data are preserved in native source;
unchanged writes replay them exactly. Changed writes construct canonical records
and report source-record changes. Unsupported versions fail explicitly.

Writers keep one strike, up to 256 byte slots plus a required missing symbol,
16-bit strike coordinates, 8-bit advance/relative bearings and signed kernMax.
They align glyphs to a common baseline without clipping. Gray8 needs an explicit
threshold and produces a loss report. Unsupported names, properties, sequences,
alias identity, charset information, vertical/scalable metrics, geometric
normalization and defaults are compared against the actual reread model and
reported. Strict mode never silently discards them. Bounds and supplied ascent/
descent must enclose all glyphs. Canonical output increases a stale `fbrExtent`
when edited glyphs need more text-buffer space, reporting that property change.
Every successful write rereads under identical
limits; retained source, output, diagnostics and reread storage share a budget.

## BinSCII transport

`Container::BinScii` wraps the same data fork in the standard segmented text
transport; the reader recognizes the sentinel automatically. Specification:
[Neil Parker's 1992 BinSCII format description](https://mirrors.apple2.org.za/ground.icaen.uiowa.edu/Mirrors/uni-kl/doc/binscii.format),
with [CiderPress2's format notes](https://ciderpress2.com/formatdoc/BinSCII-notes.html)
as an independent published description. The 64-symbol dictionary is declared
per segment. Each 3-byte group becomes four reversed 6-bit digits. A 27-byte
header carries 24-bit total/start/segment lengths and ProDOS attributes, plus
CRC-16/XMODEM over its first 24 bytes. Data lines encode 48 bytes each, padded
with zeroes; the final CRC covers the padding too. Segment order is arbitrary.

Only one complete C8/0000 data fork is accepted. Missing segments, overlaps,
conflicting attributes/names, invalid dictionaries, invalid lengths and either
CRC failure are fatal. File and segment counts, expanded bytes, retained wrapper,
font model and generated output share limits. Output uses 12 KiB segments,
filename `FONT`, access E3, zero timestamps and LF line endings. Original wrapper
bytes are retained as `iigs.binscii-source` collection metadata and reused when
they decode to the current data fork. Bare output reports their removal as loss.
monobit 0.54 can independently decode BinSCII but has no BinSCII writer.
The original sciibin 3.1 consumer independently verifies generated single- and
multi-segment output. No third-party decoder implementation is redistributed.
