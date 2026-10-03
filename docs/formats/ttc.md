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

## WOFF2 collections

Feature `woff2` + `ttc` supports collection directories, 255UInt16 indices,
per-face ordering, TrueType transforms for every distinct glyf/loca pair, CFF,
shared tables, and signature removal. The native WOFF2 codec returns reconstructed
TTC bytes and the unified raster reader consumes them. Collections mandate glyf
transforms; requesting null transforms for a glyf collection errors. hmtx uses the
legal null transform so a shared table never acquires face-dependent context.
Only one of glyf/loca being shared is rejected as WOFF2 §4.2 requires; ordinary
TTC itself may legally share either table independently.

The installed OTS rejects our mixed TTF/CFF/TTF collection and the independently
written fontTools equivalent with the same maxp error. Every extracted face
passes OTS; homogeneous TTF and homogeneous CFF collections pass directly as both
TTC and WOFF2. Google's independent decoder reconstructs every mixed face with
matching HarfBuzz behavior. System FreeType 2.13.3 opens every raw TTC face but
rejects non-first CFF faces directly from WOFF2, including an independently Google-
compressed input. These are recorded external-consumer limitations, not waived
standalone-face sanitizer failures. Prefer homogeneous collections or raw TTC
when these particular deployed consumers are targets.
