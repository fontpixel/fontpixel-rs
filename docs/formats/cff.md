# CFF1 outline table (W2.3)

Implementation contract written before code, 2026-10-03. `sfnt::cff` is a
native table/geometry API behind `cff` (`otf` enables it). It is not a FontSet
bitmap codec: the `otf` registry hooks stay absent pending strike support (W2.4)
and the raster/grid/projection contract (W2.7). CFF2 and variable instances are
outside this CFF1 work item.

## Sources and attribution

- Adobe Technical Note 5176, *The Compact Font Format Specification*, version
  1.0, 4 December 2003: https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf
  Sections 3–10: binary numbers, DICT/INDEX/header/names; 11–16: glyph order,
  encodings, charsets, CharStrings and Private/local/global subroutines;
  18–19: CID dictionaries/FDSelect; Appendix A: standard strings.
- Adobe Technical Note 5177, *The Type 2 Charstring Format*, 16 March 2000:
  https://adobe-type-tools.github.io/font-tech-notes/pdfs/5177.Type2.pdf
  Sections 3–4: numbers, widths, drawing, hints, subroutines; Appendix B:
  operand-stack, hint and nesting limits. Documentation is used as specification,
  not copied implementation.
- OpenType CFF1 restrictions (consulted 2026-10-03):
  https://learn.microsoft.com/en-us/typography/opentype/spec/cff
  Single Name/Top DICT, Type 2, CharStrings count agrees with maxp.
- Tom Chen's MIT Tong Pixel Sans exporter, supplied read-only:
  `reference/tong-pixel-sans/tools/export-rs/src/cff.rs`, SHA-256
  `5c401a35a1b9493e94190b3750fdd99a9b5fe00593f75c98be51f643a77d664b`;
  `python_reference.py`, SHA-256
  `6781d2de177174160dec901c00820e6707a330d58156e7ee00ffe31941eb4e7f`.
  Encoding strategy and four-cubic shared circle are adapted with checked
  numeric conversions, configurable scale/diameter, bounded work and canonical
  layout. The Python exporter is an independent test oracle.
- Fontations read-fonts 0.44.0 (`ps::cff`, `ps::cs`) parses CFF metadata and
  evaluates validated Type 2 programs. MIT OR Apache-2.0, already locked. No
  FreeType/FontForge implementation is copied. FreeType is an external oracle.

## Fields and model

| CFF field | Native projection / writer choice |
|---|---|
| Header | CFF1 1.0, size 4, offSize 4; reader honors header size |
| Name INDEX | Exactly one caller-supplied PostScript name |
| Top DICT | FullName SID, FontBBox, FontMatrix, charset, CharStrings, Private |
| FontMatrix | Writer diagonal `1 / units_per_em`; reader applies top/FD matrices in design space |
| String INDEX | Full name followed by distinct non-.notdef glyph names, deterministic SIDs |
| Charset | Writer format 0; reader predefined or format 0/1/2, SID or CID |
| CharStrings INDEX | Glyph-ID order; GID 0 is `.notdef`; explicit widths, nominal/default 0 |
| Private DICT | Writer explicit zero widths; reader default/nominal widths and local subrs |
| Global Subrs | None for lines; one shared four-cubic circle for dot glyphs |
| FDArray/FDSelect | Read CID-keyed CFF1 with format 0/3 FDSelect; writer is name-keyed |
| Type 2 path | Absolute move/line/cubic/close commands on read; relative commands on write |
| maxp/head/hmtx | Returned maxp 0.5 and bounds; caller uses W2.1 sfnt APIs and matching metrics |

Square and separate-square inputs reuse W2.2 bitmap geometry. Dots use W2.2
pixel origins but the CFF-native four-cubic circle (Tong factor 0.5522847498),
not the TrueType four-quadratic approximation. Each dot is a relative move and
one `callgsubr`; the circle returns to its starting point. Type 2 integer and
16.16 encodings are different from DICT integer/BCD encodings; neither silently
casts a large coordinate/width to int16.

## Ambiguities, supported reading and resource policy

Reading is unhinted CFF1 design-space geometry, not PostScript execution or
pixel-grid inference. All indexes, structural offsets, DICT operands, charsets,
FD selection, glyph counts and budgets are checked before evaluation. Type 2
stack, hint-mask sizes, calls, nesting, operator arities and termination are
validated separately so Fontations' permissive legacy recovery cannot silently
turn malformed/truncated data into successful empty geometry.

The drawing operators, flex operators, hints (ignored geometrically), local and
global subroutines and `div` are supported. Subroutines must end in `return`; an `endchar` inside a subroutine is rejected
because the locked evaluator does not propagate that terminator to its caller.
Deprecated seac composites and other
arithmetic/transient-stack operators are rejected explicitly because the locked
Fontations evaluator does not implement all of those CFF1 semantics. Reserved
operators, Type 1 programs, synthetic fonts, CFF2 and unsupported FDSelect formats
error. These boundaries are tested and must not be advertised as arbitrary CFF
execution. Opaque original bytes remain available for preservation.

Limits cover table bytes, glyphs, INDEX items, metadata/DICT work, per-glyph
commands, cumulative commands, and executed-program bytes (including repeated
subroutine calls) per glyph and per whole-font read. Call depth has a hard
ceiling of ten. Width-only glyphs and empty subroutines consume work budget too.
A borrowed reader plus glyph-at-a-time access permits streaming large fonts.

## Determinism and loss

The writer is an unhinted, name-keyed CFF1 encoder. FullName output is printable ASCII; no implicit
Unicode-to-legacy-string conversion is attempted. It does not preserve original
subroutine layout, hints, CID organization, private tuning, arbitrary DICT fields,
original numeric encodings or unused strings. The raw borrowed table is retained
on read; re-encoding projected geometry is explicitly a lossy normalization.
There is no automatic lossy FontSet conversion and no silent Unicode guess from
glyph names. Unicode comes from the sfnt cmap, not the CFF Encoding.

Fractional coordinates are quantized to Type 2 16.16; emitted bounds include the
quantized curves. Non-finite/out-of-range operands error; they are not clamped.
Name-keyed output requires representable custom SIDs (at most 64999), unique
printable PostScript glyph names, and `.notdef` first. Coordinates and bounds
must fit the signed 16-bit OpenType design-space contract. Relative displacements that exceed signed 16.16 are rejected rather than wrapped.
Advance widths must also fit signed 16.16 (values at or above 32768 error). Empty outlines still
have an advance width. Dot diameter and units per pixel are explicit and stable.

## Reproducible validation

`crates/fontpixel/tests/cff.rs` and private module tests cover generated outlines,
fractional cubic bounds and widths, parser/operator failures and budgets.
`tests/crosscheck/cff.py` independently builds Tong/fontTools references, checks
all native paths and widths, renders every selected source ink pixel through
FreeType, runs OTS and six-language HarfBuzz comparisons, and reads/re-encodes
external fonts. It includes CID FDSelect 0/3, local subrs, masks, flex operators
and a non-default FontMatrix. `--font TongPixelSans14Large-SC.bdf --cjk-count 38653`
selects the full SC sample. `fuzz/fuzz_targets/cff.rs` fuzzes raw CFF and sfnt-CFF
entry points with bounded whole-font evaluation. No Python or external tool is
called by production fontpixel code.
