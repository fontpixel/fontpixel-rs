# Windows raster FNT

## Evidence (before implementation)

The byte layout is based on Microsoft's Windows Developers Notes, KB 65123,
[archived original](https://ftp.zx.net.nz/pub/Patches/ftp.microsoft.com/MISC/KB/en-us/65/123.HTM),
and the Windows 3.1 Programmer's Reference volume 4. Version 1's packed strike
layout and version-specific table sizes are independently checked against the
MIT-licensed monobit Windows FNT implementation (installed 0.54). No GPL code
is used. `THIRD_PARTY_NOTICES.md` records the consulted implementation.

The common little-endian header is 117 bytes. Versions 2 and 3 extend it to 118
and 148 bytes. Version words are 0x100, 0x200 and 0x300. The bitmap flag is the
low bit of dfType, and vector fonts are explicitly unsupported. dfSize bounds
all offsets, even inside an aligned FON resource.

| Wire fields | Model / handling |
| --- | --- |
| dfFirstChar / dfLastChar | inclusive legacy byte codes; zero-width table slots are absent |
| dfCharSet | Windows charset identifier, retained; known charset names label legacy codes |
| dfPixHeight / dfAscent | shared raster height and baseline; bounds y = ascent - height |
| dfPoints / dfHorizRes / dfVertRes | exact integer point size and DPI |
| dfExternalLeading | line gap and baseline distance |
| dfFace / dfCopyright | family and copyright, byte-preserving Latin-1 text |
| dfDevice | retained device text |
| dfDefaultChar | relative byte index converted to absolute legacy code |
| style, decoration, internal leading, weight, family class | named `fnt.*` source properties |
| dfBitsOffset / character table | structural offsets regenerated from model |
| dfFlags / ABC / reserved extension | retained where harmless; unsupported ABC/color layouts rejected |

v1 fixed fonts have no character table. Proportional v1 uses n+1 little-endian
16-bit horizontal bit positions in a single row-major strike with dfWidthBytes
row stride. v2/v3 table entries are width:u16 and absolute offset:u16/u32; glyph
bytes are column-major **byte** columns, MSB left, with all rows of one byte
column preceding the next. A final guaranteed-blank character is emitted.

## Decisions and ambiguities

Reads keep legacy codes instead of silently interpreting symbol, OEM or unknown
charsets as Unicode. Known Windows identifiers map to exact existing charset
tables. Unknown/OEM/symbol identifiers remain explicitly unknown unless the
caller chooses a code page. Unicode writes use the selected Windows charset;
legacy writes require compatible source encoding or explicit charset selection.
ANSI means cp1252, a documented modern mapping rather than historical 1.x/2.x
variant guessing. DBCS identifiers permit only their representable single-byte
subset; enabling CJK tables is required for Unicode mapping through those pages.

The model's ascent is raster-top-to-baseline, not ascent minus internal leading;
the latter remains an independent property. Zero-width slots are not glyphs.
A valid final blank sentinel is emitted but not exposed as an encoded glyph.
Unsupported vector, color and ABC table layouts return an explicit error.
No memory pointers are dereferenced. Trailing resource padding is ignored.

## Writing and losses

Write options select v1/v2/v3 and Windows charset ID, an optional explicit code
page for OEM/symbol/unknown identifiers, gray threshold and resource limits.
The writer normalizes to a common baseline and each glyph's advance-width cell.
It reports clipped pixels, geometry/metric changes, unsupported or duplicate
mappings, Unicode sequences, alias identity, unencoded glyphs, foreign metadata
and any nonrepresentable strings. Version 1/2 limits and offsets are checked,
never truncated. Strict registry conversion rejects all reported information loss
before returning output for persistence. Source properties are validated before
reuse and structural fields are recomputed after edits.

## Verification and oracle differences

`crates/fontpixel/tests/windows.rs` authors header fields independently and
checks v1 fixed/proportional strikes, v2/v3 byte columns, charset preservation,
exact native model roundtrips, malformed bounds, losses and generated glyphs.
`tools/gen_windows_cases.py` supplies original fuzz seeds. No external font
license is needed for those data.

`tests/crosscheck/windows.py` checks both directions with monobit 0.54.0 and
loads v2/v3 and NE output in FreeType. Monobit warns about zero v3 flags although
Microsoft explicitly permits the reserved extension to be zero. It may normalize
imported raster height when exporting; tests compare its actual serialized
pixels and metrics. Its v1 writer throws `Rasters must be of same height` on the
imported proportional fixture, so its independent generated fixed-cell fixture
is used for the reverse direction. Our independently authored proportional v1
wire and monobit reading of our proportional output still test that layout.

The supplied catalogue has no FNT/FON importer. Its real BDF importer verifies
normalized bitmap rows and legacy numbers after conversion, including mini.bdf's
reported omission of CJK/unencoded glyphs. That importer rejects the existing
core BDF writer's `MICROSOFT` / `CP1252` registry spelling, so these boundary
checks explicitly use its `remap_charset=False` mode; they do not claim its
Unicode mapping path passed. Unicode mapping is separately tested with Rust and
monobit. The reference code and samples are unchanged.

Already encoded single-byte glyphs under a DBCS charset can roundtrip with the
standalone FNT feature. Exact Unicode conversion for those pages requires
`cjk-charsets`; absent tables cause an error, never an approximate mapping.

Zero-advance source glyphs are explicitly omitted with per-code and per-glyph
loss entries, because zero-width FNT slots represent absence. If no representable
nonzero-width mapping remains, writing fails rather than creating an empty font.
An entirely unrepresentable source returns `ReportedErrors` with every omission
and `fnt.no_representable_glyphs`, without output bytes. Container properties
share the source/model/diagnostic budget even through the registry API. Final
local review, full checks, external crosschecks, Tong verification, fuzz and
timing are recorded in [W3.2 review](../../reports/w3.2-review.md).
