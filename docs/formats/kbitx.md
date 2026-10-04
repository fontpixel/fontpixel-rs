# Bits'N'Picas KBITX (W3.4)

## Evidence and license boundary

The supplied MIT catalogue `opf/ingest/kbitx.py` and `tests/test_kbitx.py`
define the conversion boundary and long-run fixtures. The upstream
[importer](https://github.com/kreativekorp/bitsnpicas/blob/master/main/java/BitsNPicas/src/com/kreative/bitsnpicas/importer/KbitxBitmapFontImporter.java),
[exporter](https://github.com/kreativekorp/bitsnpicas/blob/master/main/java/BitsNPicas/src/com/kreative/bitsnpicas/exporter/KbitxBitmapFontExporter.java),
and [DTD](https://github.com/kreativekorp/bitsnpicas/blob/master/main/java/BitsNPicas/src/com/kreative/bitsnpicas/importer/kbitx.dtd)
were read only as format evidence (MPL source is not copied). Rust implementation
and synthetic fixtures are original MIT project code. monobit 0.54.0 has no
KBITX reader or writer; independent fixtures/catalogue checks replace that oracle.

## Field mapping

UTF-8 XML has root `kbits`, not `kbitx`. `prop` records contain integer values;
`name` IDs follow sfnt name IDs. `g` has Unicode decimal `u` and/or glyph name
`n`, signed `x`, top bearing `y`, advance `w`, and base64 `d`. Native BBX is
(width,height,x,y-height); no scaling. Unencoded named glyphs are retained.

`d` decodes to height then width (unsigned LEB128) followed by row-major WIB.
An opcode's low 5 bits are the length; bit 5 multiplies length by 32 (not adds
32). Bits 6–7 choose zero, 255, repeated sample, or literal samples. Padding
base64 `=` is optional; XML whitespace is allowed. Runs must cover exactly
width*height pixels. Truncation, overflow, surplus opcodes and invalid base64
are errors, unlike the catalogue's tolerant decoder.

Binary images become Mono; other images retain exact Gray8 by default.
An explicit reader threshold maps >= threshold to ink and reports grayscale
loss. The catalogue-equivalent threshold is 128. Writers retain grayscale.

Known names map to normalized names, ascent/descent to em metrics, lineGap to
line gap. Other prop/name records stay in ordered `kbitx.prop.*`/`kbitx.name.*`
properties. Kerning, feature and PUAA XML stays in validated `kbitx.element`
properties; it is not executed or used for rendering. Unsupported elements
and attributes are diagnosed as loss. Duplicate scalar records use the last
value and report normalization; duplicate Unicode labels remain visible.

## Safety, choices and losses

Only UTF-8 XML is accepted. The ordinary external DOCTYPE is allowed as inert
syntax; internal DTDs, entity declarations, processing instructions, external
resource loading and deep XML are forbidden. Token preflight charges input,
DOM bookkeeping, attribute/text sizes and retained model memory before allocation.
Each bitmap, glyph, mapping and output has configurable limits.

The standard format cannot retain alias identity for multiple Unicode labels,
Unicode sequences, vertical/scalable metrics, arbitrary source metadata or all
font metrics. Writers report each affected glyph/field and never reinterpret
legacy codes as Unicode. Names and retained native properties survive. Strict
registry output rejects any read/write loss before returning assets. Aliases
are expanded as standard separate `g` records with explicit identity-loss reports.

## Deterministic native representation

All scalar records, including known names and metrics, are retained in source
order as `kbitx.prop.ID` (integer) and `kbitx.name.ID` (text). The last matching
record supplies the normalized model field. A writer updates the last matching
native record when a normalized field is edited, retaining earlier duplicates;
new normalized fields are appended. Native property values must use these types.
Name IDs are numeric: `+01` and `1` both normalize to property key `kbitx.name.1`;
the last numeric ID wins. Authored native name property keys must be canonical
decimal integers. This changes only lexical integer spelling, not name semantics.
Root/glyph unknown attributes and unsupported elements produce explicit losses.
Named unencoded glyphs are standard; a glyph with neither name nor Unicode label
cannot be represented and is omitted with a loss. XML comments and layout are
nonsemantic and are not retained. Unicode labels must be scalar values.

The writer emits a UTF-8 declaration and the standard inert external DOCTYPE.
Empty rasters use a two-dimension WIB header. Coordinates and native property
integers are restricted to the signed 32-bit range used by Bits'N'Picas; dimensions
must additionally satisfy resource limits. Gray8 rasters containing exclusively
0/255 normalize to Mono with a representation-loss diagnostic on write.
Auxiliary `k`, `feat`, and `puaa` elements are retained as UTF-8 XML text properties
only after validating their structure, attributes, depth and entity safety.
Malformed auxiliary structures and multiple PUAA records are rejected. Output
uses the schema's prop/name/glyph/kerning/feature/PUAA order; changing the order of
retained native metadata produces an explicit loss report. Repeated scalar
lookup uses a tree index to avoid quadratic work on hostile record counts.

Normalized point size uses the em pixel height at 72 dpi (zero if the em height
is absent), matching other pixel-font importers and making native KBITX directly
convertible to BDF. This convention is reproducible and has no native XML field;
any independently edited point size or resolution produces a write loss.
The writer verifies the generated XML and a conservative reader allocation bound
without materializing a second font. Its output is limited by `max_input_bytes`
as well as the configured output cap so it can be reopened under the same limits.
