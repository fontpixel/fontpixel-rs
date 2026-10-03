# TrueType/OpenType collections (W2.7 design)

Based on the OpenType [font collection specification](https://learn.microsoft.com/en-us/typography/opentype/spec/otff#font-collections).

TTC v1/v2 use `ttcf`, a version, count and absolute face-directory offsets. Each
face directory uses standalone sfnt records whose table offsets are absolute from
the collection start. Identical tables can be shared. TrueType, CFF and mixed
collections are accepted; `.otc` is the same container.

The public bounded container API reads any selected face or all faces, exposes
borrowed tables, and extracts a standalone checksum-correct sfnt. Writing accepts
standalone sfnts, shares identical same-tag table bytes deterministically, sets
collection head.checkSumAdjustment to zero, and emits sorted aligned directories.
Native FontSet writing vectorizes each Font into one face, then uses that API.

Readers validate all directories before exposing a face. Exact identical shared
spans are legal; partial overlaps, directory/table overlap, inconsistent shared
lengths, duplicate tags, malformed v2 DSIG fields, out-of-bounds offsets and limits
fail. Collection-wide face/table-reference/retained-byte budgets prevent repeated
references from amplifying tiny inputs into unbounded native output.

Reassembly/extraction invalidates digital signatures: DSIG is removed with a loss
report. TTC v2 signature data is read only as bounded opaque bytes, never asserted
cryptographically valid. Native raster projection does not preserve layout or
arbitrary tables as editable semantics; its report states those losses. Standalone
container extraction otherwise retains exact table bytes (head adjustment aside).
