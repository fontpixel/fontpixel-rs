# Image atlas + explicit mapping

## Evidence and field mapping (W3.3, 2026-10-04, recorded before code)

Behavioral source: supplied MIT `reference/fontpixel/pipeline/opf/ingest/spritesheet.py`
and `tests/test_spritesheet.py`. Container coding uses the MIT/Apache `image` crate
with only PNG, BMP, GIF, TGA and PNM codecs enabled. Netpbm P1–P6 are image inputs;
P4 PBM and P5 PGM are output options. No image-format implementation is copied.

A mapping is UTF-8 JSON or TOML. `pixel_size`, `baseline`, `foreground` (RGB list),
`cell_size`, `characters`, `cells`, `omit_blank` match the catalogue importer.
`rect` is [left, top, width, height]; `bearing_x` is horizontal BBX offset;
`baseline - height` is bottom bearing; `advance` defaults to width. Empty char
with a name means an unencoded glyph. One Unicode scalar per char, no surrogate.
The regular layout is row-major. Nontransparent pixels count as ink unless an
explicit foreground RGB palette is supplied. No image resizing or antialiasing.

Native output adds `fontpixel_image = 1`, exact font metadata and per-cell labels,
metrics, properties and depth. It retains aliases, sequences, unencoded glyphs,
charset tags, absent metrics, gray8 bytes and ordered duplicate properties.
The atlas contains actual glyph pixels, not embedded serialized bitmap data.
JSON is default; TOML is selectable. The native extension is versioned and validated.

## Ambiguities and choices

Image input needs an explicit mapping object/path, or a same-stem `.json` sidecar
(then `.toml` only if JSON is absent). No character guessing. Reader also accepts a
mapping as primary input when its `image` field identifies a safe relative image.
Assets resolve only through `ReadContext`; includes cannot escape caller authority.
Regular dimensions must divide BOTH image dimensions. Visible pixels outside any
mapped rectangle, including irregular sheets, are rejected. Overlaps are rejected;
blank unused pixels are allowed. This intentionally tightens the supplied importer.
Transparent colored background pixels never become ink. Native gray8 preserves
zero by opaque black; mono uses black/white. Native mode determines interpretation,
so exported opaque BMP/PNM backgrounds are not mistaken for foreground.
Only a single frame is accepted from animated GIF; multiple frames are rejected
rather than silently discarded. Input metadata is bounded before decode/model
allocation. Image dimensions, decoded bytes, table bytes, glyph pixels, models and
output bytes have independent and aggregate checks. There is no filesystem I/O in
the codec.

## Write losses

One font per atlas; multi-font sets fail instead of selecting a strike silently.
Native sidecars preserve metadata; model slot holes/default IDs must be remapped or
reported. Gray8 through monochrome PBM or binary GIF requires an explicit threshold
and reports every affected glyph. Source container provenance is not font content.
Unsupported metadata must never be silently discarded. Coded colors beyond the
monochrome/grayscale model are reduced according to the requested foreground rule.

### Native TOML envelope (2026-10-04, before implementation)

Native TOML output uses `fontpixel_image = 1` and `payload_json = "..."`.
The payload is the same complete JSON mapping as native JSON output. This avoids
TOML's signed-integer ceiling and restrictions on heterogeneous arrays of enum
properties; in particular `PropertyValue::Unsigned(u64::MAX)` stays exact.
External catalogue TOML continues to use ordinary grid/cell fields. The envelope
accepts exactly these two keys, requires version 1, and cannot contain another
envelope. Both serializations are streamed into bounded sinks; no intermediate
unbounded TOML tree is constructed. The JSON-string escaping emitted here is also
valid TOML basic-string escaping.

Native TGA output uses uncompressed 8-bit grayscale. `image 0.25.10`'s default
RLE writer emits packets across scanlines; a real 128×304 atlas roundtripped in
Rust but failed Pillow 12.3 with "buffer overrun". The uncompressed variant also
avoids a four-channel scratch copy. Independent Pillow checks cover this choice.

### Aggregate memory accounting

Native reads move glyph metadata directly into stable font slots, avoiding a
second glyph vector. JSON preflight charges vector element sizes by schema key;
raw byte properties are charged as bytes, not as full property enums per sample.
Encoder scratch depends on the selected format, and metadata clones are released
before encoding. Encoder and reader peak budgets are checked separately. This
allows the full 38,653-glyph Tong SC font to roundtrip at the default 256 MiB
aggregate limit. A 4,000-glyph regression also checks a 32 MiB limit with retained
raw bitmap properties. Output sinks cap vector growth before allocating.

All nested native metadata objects reject unknown fields, including shared metrics,
name, charset and property structures. Unknown future fields cannot disappear in a
strict conversion. The same model schema is used by YAFF's native metadata extension.

## Native imageset directory (W5.E2, recorded before implementation)

The existing atlas API remains the default. A separate imageset layout follows
monobit's [native directory format](https://github.com/robhagemans/monobit/blob/040b1738085ce7967478499eef2835d2cffc95fd/monobit/storage/fontformats/image/image.py):
one glyph image per file, with the filename stem containing its numeric code.
Prefix, radix, Unicode/Legacy domain and background interpretation are explicit
options. Native writing uses deterministic hexadecimal filenames and the chosen
real image container. It returns OutputBundle::directory without a synthetic
header, manifest or atlas mapping file. Directory reads require explicit format
selection and bounded nonrecursive resource listing.

Imageset has no native metric/name metadata. Glyph dimensions remain exact;
default x/y are zero, advance is image width, and a supplied baseline can define
vertical placement. Empty dimensions cannot be stored by ordinary PNG/BMP and
require a diagnosed transparent-cell projection or a clear error. Aliases become
separate files with separate native glyph associations; unsupported mappings,
sequences, names, absent metrics and other changes produce Loss. Strict writes
reject these changes. Coverage remains Mono/Gray8; color/background conversion
is documented and diagnosed rather than fabricated as RGB model data.

Listing metadata, filenames, retained resource bytes, each decoder/encoder's
scratch, decoded glyphs, diagnostics, output files and same-limits readback are
cumulatively bounded. Sorting and keyed membership avoid quadratic behavior for
large directories. Individual files never obtain filesystem authority; only the
provided resolver handles enumeration and access. Monobit's actual imageset
container API supplies the independent full-pixel/mapping oracle.

The typed `ImageSetOptions` defaults are prefix empty, radix 16, Legacy labels,
Black background, and no supplied baseline. Radix 2..36 is supported; numeric
filenames must be unique after parsing. Black uses grayscale intensity; White
inverts it; Alpha uses the alpha sample. Colored opaque pixels use integer
299/587/114 luminance and generate Loss; greater-than-eight-bit source samples
also generate Loss. Transparent pixels contribute zero under either grayscale
profile. Binary coverage becomes Mono; intermediate samples become Gray8.
A Gray8 source containing only binary samples therefore has an explicit depth
change if the native reader reconstructs Mono.

PNG is the default directory output. PNG/BMP/GIF/TGA/PBM/PGM remain available;
Gray8 GIF/PBM needs the existing explicit threshold. Alpha output is limited to
PNG and uncompressed RGBA TGA. Zero-sized glyph images fail clearly. Directory
listing is one resource operation and each image read is another; a successful
writer requires at least glyph-file-count plus one operations. Files are named
without unnecessary zero padding, in deterministic numeric order. No file is
created at the directory primary path itself. Listing is sorted and indexed,
and decoding/encoding work scales with total image pixels and resource bytes.

Generic PNG/BMP/GIF/PNM signatures provide only Possible detection evidence:
they can be an SFont/RFont as well as an atlas image. An explicit format/profile
resolves that ambiguity. RFont's structurally valid native rfVM chunk provides
Certain evidence in its own codec, taking precedence over generic image magic.
Filename extensions never override contradictory content evidence.
