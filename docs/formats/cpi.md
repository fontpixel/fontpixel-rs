# CPI / CP / CPX

Primary format reference: [John Elliott's CPI specification](https://www.seasip.info/DOS/CPI/cpi.html),
based on MS-DOS Programmer's Reference and observations of the original loaders.
All integers are little endian. FONT uses absolute offsets; FONT.NT uses offsets
relative to the codepage entry. DRFONT adds per-size bitmap tables and a shared
256-entry character index table per codepage. Each page and size becomes a
separate strike, with legacy positional codes and explicit codepage metadata.
Standalone CP omits the file-level header and its entry's pointers are ignored.

[FreeDOS author Eric Auer's CPX description](https://sourceforge.net/p/freedos/mailman/message/7492824/)
establishes CPX as CPI renamed to COM and compressed by UPX. Production must
neither execute its x86 stub nor invoke UPX. No GPL implementation is copied.

Writers report every unsupported semantic field and enforce the same resource
limits as the readers. Printer payloads are not silently treated as screen fonts.

Implementation API: `CpiCodec`, `CpiReadOptions`, `CpiWriteOptions`, and direct
`read`/`write`, all returning a `FontSet` and report. `PointerMode::Linear` is the
default; segment:offset pointers require `Segmented`. The reader bounds linked
entries, detects cycles, and validates shared bitmap indexes before slicing.
The observed 26-byte entry-size marker and Toshiba version zero are repaired
with diagnostics. Invalid or printer pages fail the whole read.

| Model field | CPI projection |
| --- | --- |
| Each font in set order | One codepage/size record |
| `FontCharset.code_page` and `name` | Numeric page and `cp<number>` identity |
| `Legacy(index)` | Positional cell index; no implicit Unicode conversion |
| Bounds / advance | Declared cell at zero origin; horizontal cell width |
| `cpi.device` / `cpi.entry_reserved` headers | Exact 8-/6-byte entry metadata |
| `cpi.xaspect` / `cpi.yaspect` headers | Nonzero byte-valued aspect fields |
| `cpi.file_reserved` container property | Nonzero reserved file-header bytes |
| `cpi.trailing` container property | Exact trailing copyright/opaque bytes |

Unreferenced nonzero interior bytes produce loss diagnostics. Nonzero bitmap
padding is cleared with a repair diagnostic. Writers retain listed properties,
normalize sparse positional cells with explicit losses, and report all omitted
mapping/property/metric/provenance fields. DRFONT export uses size-major shared
bitmap tables and 256-cell character-index tables, retaining page and strike
order; pages must share cell geometries. CP exports exactly one page/device.
FONT can exceed 64 KiB for readers supporting its documented 32-bit pointers;
legacy DOS consumers may require smaller files.

CPX import is native, bounded NRV2B with 16-bit little-endian control words
(UPX COM method 4, header versions 10+), with no executable filter or the
16-bit E8/E9 filter 6. Header checksum, compressed and expanded Adler-32, stream
termination, exact sizes, distances, and decoded CPI signature are checked.
Filter 6 uses the COM load origin `0x100`. The expanded checksum is checked
before reversing the executable filter; this was verified against UPX 5.2.1
on all 18 FreeDOS CPIDOS 3.0a files. Other methods/filters fail explicitly.
`Version::Cpx` exports FONT data inside a native NRV2B stream with an original
8086 executable loader. No UPX code or executable stub is embedded. The encoder
uses literals and bounded matches at distances up to 256 bytes, filter zero,
and a standard end marker. The loader implements precisely this emitted subset.
The first three CPI bytes are literals so that FreeDOS MODE can patch the first
byte to RETF before entering the loader. It preserves the caller's far-return
stack and finishes by executing the decoded first byte at CS:0100.

The loader begins with MODE's required stack-check instruction and patchable
conditional branch, keeps the UPX header within MODE's first-100-byte scan, and
relocates the compressed stream and decoder above their original positions.
The writer proves that unread compressed bytes cannot be overwritten at each
decode boundary. It rejects files exceeding MODE's 58,880-byte input limit or
its additional 320-byte work-buffer allowance. The compressed data must be no
larger than its decoded CPI because the independent UPX decoder enforces this.
Incompressible inputs or excessive overlap requirements return bounded failure
reports, rather than emitting files that native consumers cannot use.
Packed files shorter than 512 bytes receive zero fill after the executable,
outside the NRV stream, because UPX's top-level file reader requires that minimum.

These interoperability conditions come from the
[FreeDOS MODE consumer](https://gitlab.com/FreeDOS/base/mode/-/blob/master/SOURCE/MODE/CPITRICK.ASM),
its [buffer allocation](https://gitlab.com/FreeDOS/base/mode/-/blob/master/SOURCE/MODE/MODECP.C),
and UPX's [COM container](https://github.com/upx/upx/blob/v5.2.1/src/p_com.cpp)
and [header decoder](https://github.com/upx/upx/blob/v5.2.1/src/packhead.cpp).
Their implementation is not copied. Development-only NASM assembles the original
loader, and Unicorn tests execute both the original loader and an external build
of the actual FreeDOS MODE `cpx2cpi` consumer. Tests check restored caller
registers/flags/segments/stack, work-buffer guards and exact decoded bytes;
neither tool is a production dependency. External UPX verifies the generated
packed bytes against the complete original CPI, including metadata.

The bounded NRV algorithm and metadata interpretation were checked against
[RetDec's MIT implementation](https://github.com/avast/retdec/blob/master/src/unpacker/decompression/nrv/nrv2b_data.cpp).
Its [unmodified license](../../LICENSES/RetDec-MIT.txt) accompanies this work.
The CPX stub is neither copied nor executed. GPL UPX and CPIDOS files are used
only as independently downloaded test oracles outside the repository.

`tests/crosscheck/w41.py` performs bidirectional monobit checks and compares all
18 CPX files against external UPX output pixel-for-pixel and metric-for-metric.
monobit 0.54's multi-page DRFONT writer emits a wrong next-entry pointer and
page-major bitmap data. Its original invalid output and hash are kept in test
artifacts; it is not repaired or accepted silently. DRFONT writer-to-reader
checks cover native multi-page/multi-size output, plus bidirectional one-page,
one-size output. Independent synthetic DRFONT index fixtures and randomized
FONT/FONT.NT/CP tests cover native input separately.

Sparse output position tables are charged before allocation. Source, output
capacity, raster scratch, read-back models, and growing diagnostic storage share
one conservative peak-memory budget. `ResourceLimit` exits do not generate an
unbounded follow-up loss report.
