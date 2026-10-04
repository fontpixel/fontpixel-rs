# HBF 1.1 (W3.5)

## Evidence, before implementation

Primary format authority: [Hanzi Bitmap Font 1.1, September 1994](https://www.ibiblio.org/pub/packages/ccic/software/info/HBF-1.1/HBF.html), particularly Format, Code Schemes, Bounding Boxes and Bitmap File Format. Implementation is independently written from that specification. monobit 0.54.0 (MIT), `storage/fontformats/xlfd/hbf.py`, is the external interoperability oracle, not the format authority. No GPL source is copied. The supplied catalogue pipeline has no HBF importer; its BDF boundary and independent HBF specification fixtures will be checked instead.

## Field map

| Wire | Model |
|---|---|
| HBF_CODE_SCHEME | original header plus explicit FontCharset name/GL; no implicit Unicode conversion |
| FONT | names.xlfd |
| SIZE | integral point_size and resolution |
| HBF_BITMAP_BOUNDING_BOX | identical glyph bounds, row-packed Mono pixels |
| FONTBOUNDINGBOX | font bounds; character advance = its width |
| XLFD properties | ordered typed properties, projected names/metrics/default character |
| COMMENT | ordered hbf.comment headers |
| Byte 2 / 3 ranges | ordered hbf.byte2 / hbf.byte3 header range lists |
| Code ranges | FontCharset.code_ranges; glyphs in increasing declared code order |
| File names / offsets | transport layout, regenerated on output |

## Choices and ambiguity

* Support 1-, 2- and 3-byte coding spaces. Required sections, declared counts, increasing disjoint ranges, valid endpoints, dimensions and containment are checked. Unknown structural keywords and non-ASCII/control text fail rather than silently disappearing. Comments and quoted doubled quotes survive semantically. Input line endings and integer spelling are canonicalized.
* Integer grammar is C-style decimal, octal and hexadecimal; signed displacements/properties are checked. SIZE is positive integral. Version 1.1 is supported; other versions fail explicitly.
* All non-Unicode codes remain Legacy, including unrecognized/vendor schemes and CNS planes. Never invent a plane prefix or substitute GBK for GB2312. Known standard GB/JIS/KSC GL/GR is inferred only from consistently low/high two-byte ranges; inconsistent schemes fail. Unknown/vendor schemes remain explicit and diagnostic. Unicode permits only scalar values in its coding space.
* Bitmap paths are validated relative ResourcePaths and resolved only through ReadContext. No filesystem access occurs in the codec. Cache each distinct bitmap once; checked offsets and sizes must fit actual data. Context input, sidecars, metadata, glyph/index estimates, bitmaps and output are cumulatively bounded before allocation.
* HBF permits distinct bitmap/font boxes. Preserve both without cropping and use font-box width for horizontal advance. Right padding bits normalize to zero with a loss diagnostic if nonzero.
* Writers require uniform positive monochrome glyph bounds fitting font bounds; no silent clipping or implicit charset remapping. Gray8 requires an explicit `gray_threshold`; output packs each row directly and reports a loss for every converted glyph. One code per glyph is representable. Unencoded glyphs, multiple aliases, sequences and duplicate codes are errors. File-order changes are reported. Writers can split output across multiple bitmap assets. Default names are relative to the primary header directory.

## Losses

Known XLFD fields are regenerated from semantic edits; unrelated typed properties are retained. Unsupported property types, unsupported header/glyph/container metadata, unsupported metrics, localized names and nonrepresentable charset details receive explicit loss diagnostics. Native HBF metadata and bitmap roundtrips are semantic, not byte-for-byte source-layout roundtrips. File name, offset, comment placement and equivalent range partitioning are transport presentation rather than glyph information. Resource and malformed-data errors never become successful partial fonts. Registry strict mode rejects accumulated conversion losses before handing the bundle to persistence.

## Implementation notes and API

`HbfWriteOptions` controls `bitmap_name` (relative to the header directory),
`glyphs_per_file`, explicit `gray_threshold`, and `ResourceLimits`. The default
is one `<header-stem>.bin` asset. Split output appends `-0001`, `-0002`, etc.
All bitmap paths use the scoped resolver; output paths are checked by
`OutputBundle`. No code in this codec opens a filesystem path.

The reader uses dense ranks for valid second/third bytes, so a sparse code range
is traversed in O(number of glyphs), independently of holes in numeric codes.
The writer copies monochrome rows; it packs thresholded Gray8 directly into the
output buffer. Both paths include metadata, glyph-index estimates, cached input
assets and accumulated bitmap bytes in the configured aggregate budget.

The specification's `Unicode 1.1` scheme is retained on native inputs and is
limited to scalar values in the two-byte UCS-2 space. Newly
created Unicode output uses the unversioned `Unicode` label, including when a
three-byte coding space is required. There is no character-set conversion.

Validation scripts use specification-authored synthetic fixtures and monobit
0.54 as an independent oracle. monobit's GB2312-to-GBK substitution and CNS plane
prefixing are deliberately excluded from expected character codes. Its writer
omits the mandatory `HBF_END_FONT` terminator; crosscheck preserves the raw
artifact and appends this terminator before strict Rust parsing. It also emits an
absolute bitmap path that its own reader cannot reopen reliably; the crosscheck
replaces that path with the verified same-directory basename, leaving metrics,
codes and bitmap bytes untouched.
