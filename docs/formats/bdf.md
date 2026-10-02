# BDF 2.1/2.2 (W1.2)

## Authority and scope

The supplied `reference/bdfparser/docs/SPEC-v3.md` §§1, 3–5, 11 and
`src/bdfparser/font.py` / `glyph.py` are the normative behavioral oracle.
Adobe BDF 2.1/2.2 defines the wire fields. The supplied MIT catalogue parser
`reference/fontpixel/pipeline/opf/parsers/bdf.py` informs repair diagnostics,
but where it disagrees (first/last duplicate, Latin-1/UTF-8, missing metrics)
SPEC and Python win. These are local, immutable reference sources, not a
claim that a remote version was checked. Attribution: THIRD_PARTY_NOTICES.md.

`bdf` is a single-strike text codec. UTF-8 replacement decoding, optional initial
BOM, CR/LF/CRLF, ASCII whitespace only, strict signed-32 numeric fields, quoted
properties, properties-block recovery, unencoded alternates, global and glyph
vertical metrics all follow Python. Gzip is an independent bounded transport
layer; enabling it does not implement PCF or any other font codec.

## Model mapping

| BDF field | Core field / retained source |
|---|---|
| STARTFONT | ordered header, binary64 canonical version spelling on output |
| FONT | names.xlfd and ordered header |
| SIZE | metrics.point_size (integer Rational), resolution and ordered header |
| FONTBOUNDINGBOX | metrics.bounds and ordered header |
| METRICSSET | metrics.metrics_set when representable, original signed header always |
| CONTENTVERSION | ordered header |
| SWIDTH/DWIDTH/SWIDTH1/DWIDTH1/VVECTOR | metrics.glyph_defaults / glyph.metrics |
| Header COMMENT | ordered header records with single-pair unquoting |
| Properties | ordered original-case typed Properties, including duplicates |
| Property COMMENT | individual ordered comment records, emitted before properties |
| FONT_ASCENT/DESCENT, PIXEL_SIZE, DEFAULT_CHAR, SPACING, underline and names | corresponding normalized semantic fields plus original properties |
| CHARSET_REGISTRY/ENCODING | FontCharset, exact canonical identity; source code is never automatically remapped |
| STARTCHAR | glyph.name |
| ENCODING | Unicode/Legacy aliases; negative codes are unencoded with source value and optional alternate retained |
| BBX | glyph.metrics.bounds, bitmap width/height |
| BITMAP | packed Mono plus individually retained stripped source rows and semantic snapshot |

Source provenance keys prefixed `bdf.` are reserved in headers/glyph properties.
They are not output as BDF extensions. Their documented role is to reconcile
original fields with later edits, not to store or return an original whole file.
An unmodified glyph's hex rows are emitted verbatim, including case, odd widths,
dirty padding and extra/missing rows. Changed pixels/dimensions are encoded as
uppercase byte-padded rows. Advances, offsets and names are independently editable.

Normalized font fields and retained properties can both be edited. A source
snapshot allows the writer to distinguish unchanged derived values from explicit
semantic changes; explicit semantic changes take precedence when both changed.
Unknown headers/glyph fields remain available as source records but are omitted
by the SPEC writer with loss diagnostics. Duplicate source glyphs remain in the
core's file-order table; effective lookup and canonical BDF output choose the last,
in its later position. The compatibility facade must use effective glyphs.

## Parsing, diagnostics, and malformed rasters

Parser warning messages and encounter order follow Python exactly, with stable
`bdf.parse.*` codes and one-based line numbers in the message. Extra core-model
materialization diagnostics use distinct `bdf.bitmap.*` / `bdf.metadata.*` codes.
The core needs a rectangular raster immediately; missing/short rows are padded,
long/extra rows cropped, with source rows retained. Non-hex rows are retained and
represented by zero pixels with an explicit diagnostic; `validate_bitmap` raises
the exact SPEC glyph/row error for consumers requiring drawable data. This keeps
parse→write parity without claiming the malformed glyph is drawable. Future
rendering/compatibility callers must call that guard before using source pixels.

Limits are checked during parsing, before expanding BBX or storing a glyph. No
allocation uses declared CHARS/STARTPROPERTIES counts. Primary/decompressed input,
metadata, bitmap, line, row and glyph totals are bounded. Required invalid numbers,
negative sizes and missing BBX are fatal parse errors naming the source line.
Unexpected EOF in headers/properties is fatal; later recoveries follow Python.

## Canonical output and losses

Writer order is exactly SPEC §5: fixed headers, optional METRICSSET/CONTENTVERSION,
global metrics, quoted header comments, properties (comments first, count excludes
comments), CHARS, effective encoded glyphs in insertion order, then unencoded,
ENDFONT. Lines are UTF-8 with LF and a final LF. Property values are flags, integers,
or strings with doubled quotes. Repeated property keys collapse case-insensitively
to the last value/spelling while retaining their first insertion position.

BDF can represent only one strike, one source charset and no sequence mapping.
Multiple strikes fail. Additional aliases are emitted as additional glyph records;
unrepresentable charset mappings, sequences, Gray8 quantization, unsupported extra
metrics/metadata, discarded duplicate records and unknown records receive explicit
loss entries. Grayscale requires an explicit threshold option. Strict rejects all
loss before output is handed to the caller. Numeric fields that cannot be read
back by the signed-32 parser fail instead of wrapping. Non-scalar Unicode source
numbers within signed-32 are retained, as required by the reference.

The CLI buffers and validates complete output before persistence; failed parsing,
strict rejection or encoding never truncates the destination. BDF round-trip
canonicalization is not preservation of comments in ignored locations, original
whitespace, duplicate records, quoting style or line endings; their relevant
losses are reported. Benchmark and validation results are in reports/w1.2.md.

## Integration details

- `fontpixel::formats::bdf::read` consumes uncompressed bytes; normal
  `fontpixel::read` / `registry().read` apply transport and then the codec
- `BdfWriteOptions` controls explicit Gray8 threshold and maximum output bytes;
  direct `bdf::write` returns `(Vec<u8>, Report)`. `BdfCodec` implements the
  registry traits and can carry these typed options in custom registrations
- Font header snapshots are `bdf.source.headers` and `bdf.source.properties`,
  each a list of `[original_name, PropertyValue]` pairs. `bdf.source.gl` records
  the initially inferred GL flag. Normalized edits win if changed from the
  snapshot; otherwise property/header edits and deletion are honored
- Glyph `bdf.bitmap.rows` is stripped rows joined with a trailing LF per row;
  `bdf.bitmap.image` is width/height as two little-endian u32 values followed by
  canonical packed bytes. `bdf.encoding` retains a negative source integer;
  `bdf.altcode` retains the optional second number. Source keys colliding with
  reserved names have raw records and are diagnosed, never mistaken for internals
- CJK mapping tables are unnecessary for preserving raw Legacy BDF encodings.
  Actually converting Unicode aliases needs `cjk-charsets`. Signed-32 ENCODING
  follows SPEC even for charsets whose numeric wire codes can exceed that bound
- Source parser diagnostics are exactly the `bdf.parse.*` subset. Additional
  core materialization notes are Info, after parse warnings. Unknown charset
  identity fallback is described by a metadata note; the future facade emits its
  lazily triggered Python warning when opening that mapping
- Fresh/edited rasters are streamed as hex rows, without constructing a height-
  sized row array. Zero-width rasters emit no rows (the parser skips empty rows),
  retaining explicit BBX dimensions and stable canonical output
- The core writer rejects newly authored line breaks in single-line wire fields
  and out-of-range required integers. It does not claim the later compatibility
  facade's unrestricted, possibly malformed transient editing API
- Raw record trivia such as line endings, whitespace and quoting style is
  canonicalized without a semantic-loss warning. Unsupported source records,
  duplicates and unsupported model fields are separately reported as losses
- monobit 0.54 reads both real fixtures and writes readable BDF with identical
  encoded/unencoded pixels and metrics. It changes some glyph tag/name metadata;
  this is recorded as an external-tool difference, not asserted to be name parity
