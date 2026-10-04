# Windows FON containers

## Evidence (before implementation)

NE layout follows Microsoft's EXE format description
[archived specification](https://github.com/qb40/exe-format), and Microsoft's
[FONTDIRENTRY](https://learn.microsoft.com/en-us/windows/win32/menurc/fontdirentry)
and [font resources](https://learn.microsoft.com/en-us/windows/win32/menurc/resource-file-formats).
PE resource traversal follows the official
[PE/COFF specification](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format).
The MIT monobit NE implementation is consulted as an interoperability oracle,
not copied wholesale. FNT payload details are in [fnt.md](fnt.md).

| Wire fields | Handling |
| --- | --- |
| MZ e_lfanew | checked absolute offset to NE or PE signature |
| NE resource offset / resident-name offset | bounds of resource table |
| NE alignment shift | scales 16-bit file offsets and lengths, checked before shifting |
| NE type 8 / type 7 | FONT payloads / FONTDIR metadata |
| PE optional-header resource data directory | resource root RVA and size, section-backed mapping |
| PE resource Type / Name / Language | checked fixed-depth tree; FONT=8, bounded names and leaves |
| FONT resource | one FNT strike in resource order |

### W5 cross-container detection correction

The Microsoft NE description identifies the executable target at header offset
`0x36` (`2` is Windows), while OS/2 NE uses target `1` and a different segment
resource directory. Microsoft's [resource type list](https://learn.microsoft.com/en-us/windows/win32/menurc/resource-types)
identifies Windows FONT as type `8` and FONTDIR as type `7`; the OS/2 GPI family
uses native directory type `6` and font type `7`. An NE/PE signature by itself
does not establish a Windows font container.

Detection now validates the existing bounded Windows resource-directory layout
and requires a FONT resource with a recognizable FNT header, without decoding
glyphs. A normal Windows NE or PE FONT container is Certain. An OS/2-targeted
NE that nevertheless has valid Windows FONT resources is only Possible, so a
compatible or contradictory container remains eligible for explicit ambiguity.
Native OS/2 GPI resources do not match the Windows directory layout. This does
not change explicit FON reading/writing, execute a container, or consult its
filename. Directory traversal uses the input context's retained bytes and
resource/table/aggregate limits; invalid offsets and budgets give NoMatch.

## Decisions, robustness and losses

Read NE and PE32/PE32+ containers; write deterministic resource-only NE with an
MZ DOS exit stub, FONTDIR and FONT resources, resident module name, empty import
and entry tables, and alignment. Embedded resources are never executed.
NE uses the minimum sufficient alignment (at least 16 bytes). FONTDIR entries
contain an ordinal, the specified 113-byte header prefix and terminated device
and face strings; resource lengths describe only that resource plus padding.
A FON can contain multiple sizes/styles and each FNT writer reports its own loss
with the corresponding font index. The FNT options also govern FON payloads.

Writer accounting preflights the entire collection's source models and diagnostic
reservations, plus container metadata. Each FNT operation receives only its share
of the remaining aggregate budget. Aligned FONT and FONTDIR sizes must fit the
resource cap; FONTDIR's named-resource offset must fit 15 bits. A fatal FNT
omission report retains its collection font index and preceding diagnostics.

Input bytes, font count, cumulative glyph count, retained model data, resource
and table bytes all share caller limits. Every RVA and shifted file range must
be backed by bytes. PE directory depth is fixed to the standard three levels;
repeated directories/cycles, invalid leaf shape and overlapping ambiguous RVA
mappings fail without recursion. Resource names are validated. Unknown resources
are skipped with loss diagnostics, not interpreted as fonts. FONTDIR is advisory;
FONT resources are authoritative. Container executable metadata is not preserved
as runnable code. PE writing is intentionally outside W3.2's requested scope.

## Usage and verification

Enable `fon` (which enables `fnt`), then use `fon::read(bytes, &FntReadOptions,
&ResourceLimits)` or registry detection. `fon::write(&FontSet, &FntWriteOptions)`
returns bytes and a structured loss report. `registry.set_fnt_*_options` also
configures FON resources. The CLI enables both formats and accepts
`--fnt-version 1|2|3`, `--fnt-charset <byte>`, `--fnt-code-page <name>`, and
`--fnt-read-code-page <name>`; `--to fnt` is needed for ambiguous `.fnt` output.
Multiple `convert` inputs can be combined into one `--to fon` output.

Independent fixtures cover NE, PE32 and PE32+, cycles, invalid leaf shape,
missing resource bytes, total glyph/resource limits, shifted-offset overflow and
truncation. Monobit checks NE bidirectionally and FreeType checks generated NE
font loading. A real Windows installation/AddFontResource test has not been run
in this Linux environment; structural interoperability is verified, rather than
claiming an unperformed OS test. No external executable is run.
