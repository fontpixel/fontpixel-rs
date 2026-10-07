# fontpixel for Python

`fontpixel` is the Python binding of the fontpixel Rust crates: the complete
native API (every format, the font model, bitmaps, rendering, OpenType tables
and containers, WOFF/WOFF2, variants, subsets, merge and every command-line
operation) plus `fontpixel.compat.bdfparser`, a drop-in bdfparser (SPEC-v3)
layer. It is a pyo3 extension built for the stable ABI (`abi3`) of CPython
3.10 and later, with type stubs for every native callable.

This page explains the concepts. The [reference](python-api-reference.md)
lists every public module, class, function, method, property and constant
with its typed signature and docstring; the
[examples](../crates/fontpixel-py/examples) are runnable scripts.

- [Install](#install)
- [Package layout](#package-layout)
- [Data model: handles, views, copies and dicts](#data-model-handles-views-copies-and-dicts)
- [Reading and writing](#reading-and-writing)
- [Options are dicts](#options-are-dicts)
- [Errors](#errors)
- [Reports, strict mode and limits](#reports-strict-mode-and-limits)
- [Resources and sidecars](#resources-and-sidecars)
- [Memory and filesystem](#memory-and-filesystem)
- [Operations (`fontpixel.ops`)](#operations-fontpixelops)
- [The bdfparser layer](#the-bdfparser-layer)
- [Type checking](#type-checking)
- [Platform notes](#platform-notes)
- [Examples](#examples)
- [Reference](#reference)

## Install

Nothing is published to PyPI. Build a wheel from this repository (Rust 1.99
per `rust-toolchain.toml`, and [maturin](https://www.maturin.rs/)):

```sh
python -m pip install maturin
cd crates/fontpixel-py
maturin build --release                 # wheel in target/wheels/
python -m pip install ../../target/wheels/fontpixel-*.whl
```

For development, `maturin develop --release` installs the package in editable
mode into the active virtual environment. One abi3 wheel serves CPython 3.10
and every later version on the same platform; a manylinux wheel is built with
`maturin build --release --zig --compatibility manylinux2014`.

## Package layout

| module | contents |
| --- | --- |
| `fontpixel` | the model handles (`FontSet`, `Font`, `Glyph`, `Bitmap`, `Codepoint`, `Properties`), the format registry (`Registry`, `read`, `write`, `detect`, `list_formats`), resources (`Input`, `ResourceSet`, `ReadContext`, `OutputBundle`, `OutputAsset`, `Written`), rendering (`Font.draw*`, `Rendered`, `to_png`, `to_svg`), charsets (`CharsetCodec`, ...), metadata (`apply_names`, `BitmapProjection`), report helpers and every exception class |
| `fontpixel.formats` | one namespace per Rust codec module (`fontpixel.formats.pcf`, `.psf`, `.bmfont`, ...; listed in `fontpixel.formats.MODULES`) with that module's own `detect`/`read`/`write` and extra functions; `normalize`, `option_types`, `configurable_formats`, `CATALOGUE`, `MODULES` |
| `fontpixel.sfnt` | the sfnt container (`Sfnt`, `SfntBuilder`), `collection`, `cff`, `subset` and table codecs (`cmap`, `name`, `os2`, `post`, `metrics`, `gasp`, `gsub`, `glyf`, `bitmap`) |
| `fontpixel.woff` | `woff1` and `woff2` containers |
| `fontpixel.outline` | bitmap contours, segments and rasterization; `outline.path` edge rasterizer |
| `fontpixel.variants` | regional/script alternatives with `locl` (`Variants`, `PreparedVariants`) |
| `fontpixel.merge` | glyph merging (`merge`, `Selection`) |
| `fontpixel.ops` | every command-line operation in-process (`convert`, `batch`, `render`, ...) |
| `fontpixel.compat.bdfparser` | bdfparser's `Font`, `Glyph`, `Bitmap` and module functions |
| `fontpixel.types` | `TypedDict`s and `Literal` aliases for every plain-data shape (typing only) |

Native submodules are registered in `sys.modules`, so `import fontpixel.sfnt.cmap`
and `from fontpixel.formats import pcf` work after `import fontpixel`.

## Data model: handles, views, copies and dicts

Fonts stay in Rust. Python holds *handles*:

- **`FontSet`** owns a list of strikes. `font_set[0]` (and iterating,
  `font_set.fonts`) returns a **view**: a `Font` handle that addresses strike 0
  of that set. Editing the view edits the set; no font is copied.
- **`Font`** is either owned (`fp.Font()`, `font.copy()`) or a view into a set
  (`font.is_view`). `font[gid]`, `font.glyph(gid)` and
  `font.glyph_for_codepoint("A")` return `Glyph` views addressed by the stable
  glyph ID.
- **`Glyph`** is owned or a view into a font. Removing a glyph keeps the other
  IDs (`font.slot_count` counts reserved slots); a view whose target is gone
  raises `InvalidInputError`.
- **`Bitmap`** is always owned. `glyph.bitmap` returns a *copy*; assign it back
  (`glyph.bitmap = edited`) to change the glyph. One class covers both pixel
  formats: `depth` is `"mono"` (values 0..1, packed rows) or `"gray8"` (0..255);
  conversions are explicit (`to_gray8`, `to_mono(threshold)`).
- Pixel data is copied out only on request: `to_rows()`, `to_list()`,
  `to_bytes(layout)`, `as_bytes()`, `to_text()`, `bitmap[x, y]`.
  Coordinates are `(x, y)` from the top-left pixel.
- **Plain data** (metrics, names, charset, bounds, reports, option structs)
  crosses the boundary as dicts with the Rust field names. Reading
  `font.metrics` returns a copy; edit it and assign it back
  (`font.metrics = metrics`). Every handle has `to_dict()`/`from_dict()` with
  the serde shape.
- **Codepoints** are tagged: `Codepoint.unicode(0x41)` or `Codepoint.legacy(0x41)`
  (a code in the font's own charset). Functions that take a codepoint accept a
  `Codepoint`, an `int` or a one-character `str` (Unicode), or the plain-data
  dict `{"Unicode": n}` / `{"Legacy": n}` (the Rust variant names; note the
  capital letter).
- `Properties` is a live list handle over `font.properties`, `font.headers`,
  `font_set.properties` or `glyph.properties`. `get(name)` returns the value in
  tagged form (`{"Text": "Spleen"}`, `{"Integer": 8}`, ...).

Handle arguments are copied before the receiver is locked, so passing a view
of the same font (`font.replace_glyph(gid, font[gid])`) never deadlocks.
`copy.copy`/`copy.deepcopy` and `.copy()` produce independent owned values.

## Reading and writing

```python
import fontpixel as fp

loaded = fp.read(data, name="font.bdf")     # bytes, bytearray, memoryview, Input, OutputBundle, Written
fonts = loaded.fonts                         # FontSet handle (Font after loaded.into_single())
print(loaded.format, loaded.report["entries"])

written = fp.write(fonts, "pcf", {"glyph_pad": "4"})   # a Written
pcf = written.output.into_single()                       # bytes (one-file formats)
for asset in written.output:                             # every file of a multi-file format
    print(asset.path, len(asset.data))
```

`fp.detect(data, name=...)` returns `{"format": FormatInfo, "confidence": ...}`.
Content detectors decide first; the name is only a hint, and equally strong
candidates raise `AmbiguousFormatError` (with `candidates`). A `Registry`
(`fp.Registry()` is the built-in registry; `Registry(builtin=False)` the
catalogue without codecs) carries per-format options for its reads and writes and accepts
formats implemented in Python (`Registry.register(format, detector=, reader=,
writer=)`).

The per-codec namespaces bind each Rust module's direct functions with their
native shapes: `fontpixel.formats.psf.write(fonts, options)` returns
`(bytes, report)`, bundle writers return a `Written`, readers return
`(model, report)`. See the docstrings in the reference.

## Options are dicts

Every option struct is a dict with the Rust field names; omitted keys take
the documented Rust defaults, unknown keys and unknown enum names raise
`ValueError`, and integers out of range raise `OverflowError`/`ValueError`.
Enum values are their serde strings (`"msb"`, `"true_type"`, `"ascending"`).

To discover option shapes at run time:

```python
fp.formats.option_types()                         # every Rust type name normalize() accepts
fp.formats.normalize("formats::pcf::PcfWriteOptions")         # its defaults, as a dict
fp.formats.normalize("formats::pcf::PcfWriteOptions", {"glyph_pad": "4"})  # validate + fill
fp.formats.configurable_formats()                 # {"read": [...], "write": [...]} format IDs
fp.default_resource_limits(), fp.default_render_options(), fp.default_image_options()
fp.sfnt.defaults(), fp.woff.defaults(), fp.variants.defaults(), fp.outline.defaults(), fp.merge.defaults()
```

The serde error names the accepted values (``unknown variant `9`, expected `1`
or `2` ``, ``unknown field `x`, expected one of ...``).

For static checking, `fontpixel.types` has a `TypedDict` for every serde struct
and a `Literal` alias for every string enum (generated from the Rust serde
derives), and `fontpixel.types.SERDE_TYPES` maps each Rust path to its Python
type:

```python
from fontpixel.types import PcfWriteOptions, RenderOptions

options: PcfWriteOptions = {"byte_order": "msb", "glyph_pad": "4"}   # checked by mypy/pyright
```

Data-carrying enums are unions of one-key dicts (`{"Unicode": 65}`); keys that
may be omitted on input are not required in the `TypedDict`.

## Errors

Every exception derives from `fontpixel.FontpixelError` and carries `kind` (the
Rust error variant) and `exit_code` (what the command line would exit with):

| exception | when | extra attributes |
| --- | --- | --- |
| `UnknownFormatError` | unknown format ID | `format` |
| `FeatureDisabledError` | format compiled out | `format` |
| `NotImplementedFormatError` | format lacks the capability | `format`, `operation` |
| `AmbiguousFormatError` | several formats match | `candidates` |
| `UnrecognizedFormatError` | no detector matched | |
| `InvalidInputError` | invalid data, options, paths, stale views | |
| `BdfParseError` | fatal BDF syntax error | `line` |
| `ResourceLimitError` | a configured budget was exceeded | `resource`, `limit` |
| `MissingResourceError` | a sidecar was not supplied | `path` |
| `MultipleAssetsError` | several assets where one was expected | `count` |
| `StrictLossError` | strict mode rejected an information loss | `report` |
| `ReportedErrorsError` | a codec reported errors (no output) | `report` |
| `FontpixelIoError` | operating-system or resolver I/O | |
| `UnsupportedError` | capability unavailable on this platform/build | |
| `CancelledError` | an `ops` `progress=` callback returned `False` (nothing written) | |

Python-level argument problems use the built-in classes: `TypeError` for wrong
argument types, `ValueError` for dicts that do not deserialize, `KeyError`
for unknown names in lookups (`normalize("no::Such")`), `OverflowError` for
integers outside the Rust range. Exceptions raised by `ops` file providers and
Python-implemented codecs (`Registry.register`) propagate unchanged; an
exception from a sidecar resolver becomes `FontpixelIoError` with the original
message (`resolver raised: RuntimeError: ...`).
`fontpixel.compat.bdfparser` raises bdfparser's exception classes, which also
derive from the matching fontpixel class.

## Reports, strict mode and limits

Reports are dicts `{"entries": [...]}`; each entry has `level`
(`"info"`/`"warning"`/`"error"`), `impact` (`"none"`/`"repair"`/`"loss"`), a
stable `code`, a `location` (`font_index`, `glyph_index`, `codepoint`, `asset`)
and a `message`. Nothing is truncated. `fp.report_to_text`, `report_has_loss`,
`report_has_errors`, `report_push` and `report_extend` work on these dicts.

Lenient calls return the output and the report. `strict=True` (on `write`,
codec writers, `ops` requests and `RenderOptions`) turns any loss into
`StrictLossError` with the complete `report`; codecs that report errors raise
`ReportedErrorsError` and return nothing.

Budgets are dicts too: `ResourceLimits` (input, sidecar, decompressed, glyph,
pixel and table bytes; `limits=` on reads), `TransportLimits` (gzip
transport; `transport_limits=` on writes), `BitmapLimits` (per `Bitmap`),
`RenderLimits` (in `RenderOptions`), and the per-area limits of `sfnt`,
`woff`, `outline` and `variants` (see their `defaults()`). Exceeding one raises
`ResourceLimitError` naming the resource and limit. `retained_usage()` /
`cloned_usage()` report the memory a model holds or a copy would need.

## Resources and sidecars

Some formats are several files (BMFont pages, image atlases, Mac resource
forks, directory inputs). Writers return all of them in an `OutputBundle`
(primary entry point plus assets). Readers resolve sidecars through an explicit
*resources* argument; nothing reads the filesystem implicitly:

- `None`: standalone input; a sidecar request raises `MissingResourceError`;
- a `ResourceSet` or `{name: bytes}` dict of named in-memory files;
- a resolver object with `resolve(path, max_bytes) -> bytes` (and optionally
  `list(directory, max_entries, max_bytes) -> list[str]`), or a callable
  `f(path, max_bytes) -> bytes`.

Paths are relative, `/`-separated and may not contain `.`/`..`
(`fp.resource_path` validates one). An `Input(data, name=..., resources=...)`
carries all three together, `Input.directory(name, resources)` names a
directory entry point, and a `Written`/`OutputBundle` passed to `fp.read` is
read back with its own entry point and sidecars. Byte inputs (`bytes`,
`bytearray`, `memoryview`) are copied once into Rust; the sfnt parsers
(`sfnt.Sfnt`, `Collection`, `Cff`) keep a reference to a `bytes` object
instead of copying it.

## Memory and filesystem

The native API works on memory only: bytes in, bytes or bundles out. Files are
read and written by the caller (or a resolver it supplies).

`fontpixel.ops` adds the command line's filesystem behaviour. With `files=None`
request paths are real paths (`str` or `os.PathLike`), sidecars are found
beside their primary file, sources are never overwritten and every output group
is written transactionally. With `files={name: bytes}` (or a `ResourceSet`, or
a provider object with `entry`/`read`/`list`) the same operation runs entirely
in memory: paths are keys, and `result.outputs` / `result.bundles` return the
prepared files. `ops.capabilities()` reports what the platform supports.

## Operations (`fontpixel.ops`)

Each command-line command is a function taking a request dict (the
command-line option names in snake_case) and/or the same keys as keyword
arguments: `convert`, `vectorize`, `merge`, `variants`, `subset`, `charset`,
`render`, `validate`, `diff`, `info`, `preview`, `formats`, and `batch`,
`batch_toml`, `batch_file` for job lists. Results reproduce the command line:
`result.json()` is the `--report json` document, `result.text()` the text
output, byte for byte; `exit_code`, `report`, `summary` and `document` give
structured access. `parse_args(argv)` parses a command line into a request,
`inspect_request(operation, request)` validates and normalizes one and
`parse_batch_toml(text)` prepares a TOML job file without running it.
Requests the command line would reject raise `InvalidInputError` with
`exit_code` 2. The request shapes are `fontpixel.types.ConvertRequest`,
`MergeRequest`, ... (and `BatchRequest`).

## The bdfparser layer

`fontpixel.compat.bdfparser` implements bdfparser 3 (SPEC-v3): `Font`,
`Glyph`, `Bitmap`, `BDFParseError`, `BDFParserWarning` and the `encoding`,
`outline`, `pcf`, `ttf` and `cli` helpers, with bdfparser's behaviour
(including its warnings and exception classes). Font data stays in a native
model; `Font`/`Glyph` keep handles and `Bitmap` is a native raster of labels
0, 1 and 2, converted to row strings only on request. The native handles are
`unsendable`: use a compat font from the thread that created it. The
unmodified bdfparser test suite runs against this layer through an import
shim (`crates/fontpixel-py/tests/compat_bdfparser`, with the remaining
differences in its `KNOWN_DIFFERENCES.md`).

## Type checking

The package ships `py.typed`, a `.pyi` stub for every native module
(`fontpixel/_native/*.pyi`, `fontpixel/formats/*.pyi`, `fontpixel/sfnt/*.pyi`,
...) and the runtime module `fontpixel.types`. Type checkers see every
class, method, property, function, constant and exception with real types:
handles as classes, plain data as `TypedDict`s, enum strings as `Literal`s.
The examples and `tests/typing_smoke.py` are checked with mypy 2.4
(`--strict`) and pyright 1.1.414 (no errors). The stubs import `Self` and
`disjoint_base` from `typing_extensions`, so the checker's bundled typeshed
needs `typing_extensions` 4.15 or later (mypy 1.18+, current pyright). pyright
warns `reportMissingModuleSource` for the native submodules
(`fontpixel.formats.pcf`, `fontpixel.sfnt.cmap`, ...): they are stubs over an
extension module and have no Python source. Keys that may be omitted on input
are not required in the `TypedDict`s even though results contain them;
pyright's `reportTypedDictNotRequiredAccess` therefore prefers
`metrics.get("pixel_size")` over `metrics["pixel_size"]`.

The stubs are generated, not written by hand:
`tools/bindings/gen_pyi.py` introspects the built extension, reads the Rust
parameter/result types of every binding from `crates/fontpixel-py/src`, and
derives the dict and enum shapes from the serde derives in nightly rustdoc
JSON (built by `tools/bindings/gate.sh`). `--check` fails when a committed stub
is out of date; `tools/bindings/stubtest.py` runs `mypy.stubtest` with the
generated allowlist (`tools/bindings/stubtest-allowlist.txt`, every entry with
its reason). The test suite runs both, plus `mypy --strict` over the examples
and `tests/typing_smoke.py`.

After a binding or a serde type changes, regenerate in this order (the test
suite's `--check` tests fail until then):

```sh
T=target/rustdoc-json                               # or $FONTPIXEL_RUSTDOC_TARGET
tools/bindings/gate.sh python || true               # builds $T/doc/*.json (the check itself may fail)
(cd crates/fontpixel-py && maturin develop --release)
python tools/bindings/gen_pyi.py --rustdoc $T/doc --strict   # stubs, types.py, allowlist
python tools/bindings/gen_pyref.py                  # docs/python-api-reference.md + index below
python tools/bindings/stubtest.py
```

`--strict` fails on any new parameter or result the generator cannot type
(an unconverted `&Bound<PyAny>`); add its type to
`tools/bindings/pyi_overrides.py`. The mypy-based tests skip when mypy is not
installed, and the stub check skips when the rustdoc JSON is missing.

Some results are unions that need narrowing: `fp.read(...).fonts` is
`FontSet | Font` (a `FontSet` unless `into_single()` was called);
`isinstance(fonts, fp.FontSet)` narrows it.

## Platform notes

- **Python**: CPython 3.10+ through the stable ABI; one wheel per platform.
  PyPy and free-threaded builds are not targeted.
- **Threads**: the model and resource handles (`FontSet`, `Font`, `Glyph`,
  `Bitmap`, `Properties`, `ResourceSet`, ...) are immutable Python objects over
  lock-protected shared values, so they can be used from several threads, and
  long-running calls release the GIL (codec reads and writes, operations, WOFF
  compression, variants planning), so independent work proceeds in parallel.
  `Registry` and `sfnt.SfntBuilder` are mutable objects: mutating one from two
  threads at once fails pyo3's borrow check with `RuntimeError`; give each
  thread its own. The bdfparser layer's handles are bound to their creating
  thread.
- **Integers** map to Rust's fixed widths (`u8`, `u16`, `u32`, `i64`,
  `usize`); values outside a parameter's range raise `OverflowError`.
- **Strings** are UTF-8 Unicode; four-byte OpenType tags accept a four-character
  Latin-1 `str` (`"OS/2"`) or `bytes` and come back as `str`.
- **Filesystem**: only `fontpixel.ops` (filesystem mode) and resolver callbacks
  the caller writes touch files; `ops.batch_file` resolves paths against the
  job file's directory. All 99 catalogue formats, CJK charset tables and the
  bdfparser layer are compiled into the wheel.

## Examples

Each example is a script `python examples/NAME.py [INPUT] [OUTPUT_DIR]`
(defaults: a font of this repository and `./fontpixel-examples`), and
`tests/test_examples.py` runs every one on repository fixtures:

| example | shows |
| --- | --- |
| [read_inspect_convert.py](../crates/fontpixel-py/examples/read_inspect_convert.py) | detect, read, inspect names/metrics/glyphs, write PCF/PSF/YAFF, round trip |
| [edit_glyphs.py](../crates/fontpixel-py/examples/edit_glyphs.py) | views vs copies, pixel edits, new glyphs, metrics dicts, save |
| [render_text.py](../crates/fontpixel-py/examples/render_text.py) | `draw` with `RenderOptions`, PNG/SVG export, vertical text, `draw_all` |
| [typed_options.py](../crates/fontpixel-py/examples/typed_options.py) | option discovery, `TypedDict` options, codec namespaces, registry options, validation |
| [opentype.py](../crates/fontpixel-py/examples/opentype.py) | outline TTF, OTB, sfnt tables, WOFF2, subsets, `locl` variants |
| [merge_fonts.py](../crates/fontpixel-py/examples/merge_fonts.py) | `merge.merge` and `ops.merge` in memory |
| [batch_jobs.py](../crates/fontpixel-py/examples/batch_jobs.py) | `ops.batch`, `batch_toml`, `batch_file`, per-task failures |
| [resources_sidecars.py](../crates/fontpixel-py/examples/resources_sidecars.py) | output bundles, sidecar resources, memory vs filesystem |
| [compat_bdfparser.py](../crates/fontpixel-py/examples/compat_bdfparser.py) | bdfparser code on the Rust core |
| [errors_limits.py](../crates/fontpixel-py/examples/errors_limits.py) | exception classes, strict mode, resource limits, exit codes |

## Reference

The generated [API reference](python-api-reference.md) lists every public
callable with its signature and docstring (`tools/bindings/gen_pyref.py`,
checked by the test suite). Modules:

<!-- reference index: generated by tools/bindings/gen_pyref.py -->

| module | public callables |
| --- | ---: |
| [`fontpixel`](python-api-reference.md#fontpixel) | 459 |
| [`fontpixel.compat`](python-api-reference.md#fontpixelcompat) | 0 |
| [`fontpixel.compat.bdfparser`](python-api-reference.md#fontpixelcompatbdfparser) | 87 |
| [`fontpixel.compat.bdfparser.bitmap`](python-api-reference.md#fontpixelcompatbdfparserbitmap) | 1 |
| [`fontpixel.compat.bdfparser.cli`](python-api-reference.md#fontpixelcompatbdfparsercli) | 1 |
| [`fontpixel.compat.bdfparser.encoding`](python-api-reference.md#fontpixelcompatbdfparserencoding) | 15 |
| [`fontpixel.compat.bdfparser.errors`](python-api-reference.md#fontpixelcompatbdfparsererrors) | 0 |
| [`fontpixel.compat.bdfparser.font`](python-api-reference.md#fontpixelcompatbdfparserfont) | 0 |
| [`fontpixel.compat.bdfparser.glyph`](python-api-reference.md#fontpixelcompatbdfparserglyph) | 1 |
| [`fontpixel.compat.bdfparser.outline`](python-api-reference.md#fontpixelcompatbdfparseroutline) | 2 |
| [`fontpixel.compat.bdfparser.pcf`](python-api-reference.md#fontpixelcompatbdfparserpcf) | 3 |
| [`fontpixel.compat.bdfparser.ttf`](python-api-reference.md#fontpixelcompatbdfparserttf) | 2 |
| [`fontpixel.formats`](python-api-reference.md#fontpixelformats) | 3 |
| [`fontpixel.formats.amiga`](python-api-reference.md#fontpixelformatsamiga) | 3 |
| [`fontpixel.formats.bbc`](python-api-reference.md#fontpixelformatsbbc) | 3 |
| [`fontpixel.formats.bdf`](python-api-reference.md#fontpixelformatsbdf) | 5 |
| [`fontpixel.formats.beos`](python-api-reference.md#fontpixelformatsbeos) | 3 |
| [`fontpixel.formats.bmfont`](python-api-reference.md#fontpixelformatsbmfont) | 3 |
| [`fontpixel.formats.c`](python-api-reference.md#fontpixelformatsc) | 3 |
| [`fontpixel.formats.catalyst`](python-api-reference.md#fontpixelformatscatalyst) | 3 |
| [`fontpixel.formats.chiwriter`](python-api-reference.md#fontpixelformatschiwriter) | 3 |
| [`fontpixel.formats.comfont`](python-api-reference.md#fontpixelformatscomfont) | 3 |
| [`fontpixel.formats.consoleet`](python-api-reference.md#fontpixelformatsconsoleet) | 3 |
| [`fontpixel.formats.cpi`](python-api-reference.md#fontpixelformatscpi) | 3 |
| [`fontpixel.formats.cybiko`](python-api-reference.md#fontpixelformatscybiko) | 3 |
| [`fontpixel.formats.daisy`](python-api-reference.md#fontpixelformatsdaisy) | 3 |
| [`fontpixel.formats.dashen`](python-api-reference.md#fontpixelformatsdashen) | 3 |
| [`fontpixel.formats.dosstart`](python-api-reference.md#fontpixelformatsdosstart) | 3 |
| [`fontpixel.formats.draw`](python-api-reference.md#fontpixelformatsdraw) | 3 |
| [`fontpixel.formats.drcs`](python-api-reference.md#fontpixelformatsdrcs) | 3 |
| [`fontpixel.formats.drhalo`](python-api-reference.md#fontpixelformatsdrhalo) | 3 |
| [`fontpixel.formats.edwin`](python-api-reference.md#fontpixelformatsedwin) | 3 |
| [`fontpixel.formats.embedded`](python-api-reference.md#fontpixelformatsembedded) | 7 |
| [`fontpixel.formats.fbit`](python-api-reference.md#fontpixelformatsfbit) | 5 |
| [`fontpixel.formats.figlet`](python-api-reference.md#fontpixelformatsfiglet) | 3 |
| [`fontpixel.formats.fnt`](python-api-reference.md#fontpixelformatsfnt) | 5 |
| [`fontpixel.formats.fon`](python-api-reference.md#fontpixelformatsfon) | 3 |
| [`fontpixel.formats.fontx2`](python-api-reference.md#fontpixelformatsfontx2) | 3 |
| [`fontpixel.formats.fzx`](python-api-reference.md#fontpixelformatsfzx) | 3 |
| [`fontpixel.formats.gdr`](python-api-reference.md#fontpixelformatsgdr) | 3 |
| [`fontpixel.formats.gem`](python-api-reference.md#fontpixelformatsgem) | 3 |
| [`fontpixel.formats.geos`](python-api-reference.md#fontpixelformatsgeos) | 3 |
| [`fontpixel.formats.gf`](python-api-reference.md#fontpixelformatsgf) | 3 |
| [`fontpixel.formats.gpi`](python-api-reference.md#fontpixelformatsgpi) | 3 |
| [`fontpixel.formats.grasp`](python-api-reference.md#fontpixelformatsgrasp) | 3 |
| [`fontpixel.formats.hanfont`](python-api-reference.md#fontpixelformatshanfont) | 4 |
| [`fontpixel.formats.hbf`](python-api-reference.md#fontpixelformatshbf) | 3 |
| [`fontpixel.formats.hex`](python-api-reference.md#fontpixelformatshex) | 3 |
| [`fontpixel.formats.hmzk`](python-api-reference.md#fontpixelformatshmzk) | 3 |
| [`fontpixel.formats.hppcl`](python-api-reference.md#fontpixelformatshppcl) | 3 |
| [`fontpixel.formats.hrcg`](python-api-reference.md#fontpixelformatshrcg) | 3 |
| [`fontpixel.formats.hzk`](python-api-reference.md#fontpixelformatshzk) | 6 |
| [`fontpixel.formats.iigs`](python-api-reference.md#fontpixelformatsiigs) | 3 |
| [`fontpixel.formats.image`](python-api-reference.md#fontpixelformatsimage) | 3 |
| [`fontpixel.formats.kbits`](python-api-reference.md#fontpixelformatskbits) | 3 |
| [`fontpixel.formats.kbitx`](python-api-reference.md#fontpixelformatskbitx) | 3 |
| [`fontpixel.formats.lisa`](python-api-reference.md#fontpixelformatslisa) | 3 |
| [`fontpixel.formats.mgtk`](python-api-reference.md#fontpixelformatsmgtk) | 3 |
| [`fontpixel.formats.mkwinfont`](python-api-reference.md#fontpixelformatsmkwinfont) | 3 |
| [`fontpixel.formats.nfnt`](python-api-reference.md#fontpixelformatsnfnt) | 4 |
| [`fontpixel.formats.oberon`](python-api-reference.md#fontpixelformatsoberon) | 3 |
| [`fontpixel.formats.palm`](python-api-reference.md#fontpixelformatspalm) | 3 |
| [`fontpixel.formats.pbf`](python-api-reference.md#fontpixelformatspbf) | 3 |
| [`fontpixel.formats.pcf`](python-api-reference.md#fontpixelformatspcf) | 3 |
| [`fontpixel.formats.pcgeos`](python-api-reference.md#fontpixelformatspcgeos) | 3 |
| [`fontpixel.formats.pcr`](python-api-reference.md#fontpixelformatspcr) | 3 |
| [`fontpixel.formats.pf2`](python-api-reference.md#fontpixelformatspf2) | 3 |
| [`fontpixel.formats.pfr`](python-api-reference.md#fontpixelformatspfr) | 3 |
| [`fontpixel.formats.pilfont`](python-api-reference.md#fontpixelformatspilfont) | 3 |
| [`fontpixel.formats.pk`](python-api-reference.md#fontpixelformatspk) | 3 |
| [`fontpixel.formats.plan9`](python-api-reference.md#fontpixelformatsplan9) | 3 |
| [`fontpixel.formats.playdate`](python-api-reference.md#fontpixelformatsplaydate) | 3 |
| [`fontpixel.formats.plus3dos`](python-api-reference.md#fontpixelformatsplus3dos) | 3 |
| [`fontpixel.formats.polyprint`](python-api-reference.md#fontpixelformatspolyprint) | 3 |
| [`fontpixel.formats.prebuilt`](python-api-reference.md#fontpixelformatsprebuilt) | 3 |
| [`fontpixel.formats.printshop`](python-api-reference.md#fontpixelformatsprintshop) | 3 |
| [`fontpixel.formats.psf`](python-api-reference.md#fontpixelformatspsf) | 3 |
| [`fontpixel.formats.psf2txt`](python-api-reference.md#fontpixelformatspsf2txt) | 3 |
| [`fontpixel.formats.pxl`](python-api-reference.md#fontpixelformatspxl) | 3 |
| [`fontpixel.formats.qpf`](python-api-reference.md#fontpixelformatsqpf) | 3 |
| [`fontpixel.formats.qpf2`](python-api-reference.md#fontpixelformatsqpf2) | 3 |
| [`fontpixel.formats.raw`](python-api-reference.md#fontpixelformatsraw) | 4 |
| [`fontpixel.formats.riscos`](python-api-reference.md#fontpixelformatsriscos) | 3 |
| [`fontpixel.formats.rockbox`](python-api-reference.md#fontpixelformatsrockbox) | 3 |
| [`fontpixel.formats.sbf`](python-api-reference.md#fontpixelformatssbf) | 3 |
| [`fontpixel.formats.sfd`](python-api-reference.md#fontpixelformatssfd) | 3 |
| [`fontpixel.formats.sfont`](python-api-reference.md#fontpixelformatssfont) | 3 |
| [`fontpixel.formats.signum`](python-api-reference.md#fontpixelformatssignum) | 3 |
| [`fontpixel.formats.snf`](python-api-reference.md#fontpixelformatssnf) | 3 |
| [`fontpixel.formats.ssfn`](python-api-reference.md#fontpixelformatsssfn) | 3 |
| [`fontpixel.formats.svg`](python-api-reference.md#fontpixelformatssvg) | 6 |
| [`fontpixel.formats.symbos`](python-api-reference.md#fontpixelformatssymbos) | 3 |
| [`fontpixel.formats.tasprint`](python-api-reference.md#fontpixelformatstasprint) | 3 |
| [`fontpixel.formats.u8m`](python-api-reference.md#fontpixelformatsu8m) | 3 |
| [`fontpixel.formats.vector_raster`](python-api-reference.md#fontpixelformatsvector_raster) | 6 |
| [`fontpixel.formats.vfont`](python-api-reference.md#fontpixelformatsvfont) | 3 |
| [`fontpixel.formats.vlw`](python-api-reference.md#fontpixelformatsvlw) | 3 |
| [`fontpixel.formats.vtfont`](python-api-reference.md#fontpixelformatsvtfont) | 3 |
| [`fontpixel.formats.writeon`](python-api-reference.md#fontpixelformatswriteon) | 3 |
| [`fontpixel.formats.wsfont`](python-api-reference.md#fontpixelformatswsfont) | 3 |
| [`fontpixel.formats.wyse`](python-api-reference.md#fontpixelformatswyse) | 3 |
| [`fontpixel.formats.x11`](python-api-reference.md#fontpixelformatsx11) | 2 |
| [`fontpixel.formats.xbin`](python-api-reference.md#fontpixelformatsxbin) | 3 |
| [`fontpixel.formats.xerox`](python-api-reference.md#fontpixelformatsxerox) | 3 |
| [`fontpixel.formats.yaff`](python-api-reference.md#fontpixelformatsyaff) | 3 |
| [`fontpixel.formats.zapfont`](python-api-reference.md#fontpixelformatszapfont) | 3 |
| [`fontpixel.merge`](python-api-reference.md#fontpixelmerge) | 12 |
| [`fontpixel.ops`](python-api-reference.md#fontpixelops) | 44 |
| [`fontpixel.outline`](python-api-reference.md#fontpixeloutline) | 7 |
| [`fontpixel.outline.path`](python-api-reference.md#fontpixeloutlinepath) | 3 |
| [`fontpixel.sfnt`](python-api-reference.md#fontpixelsfnt) | 14 |
| [`fontpixel.sfnt.bitmap`](python-api-reference.md#fontpixelsfntbitmap) | 12 |
| [`fontpixel.sfnt.cff`](python-api-reference.md#fontpixelsfntcff) | 15 |
| [`fontpixel.sfnt.cmap`](python-api-reference.md#fontpixelsfntcmap) | 4 |
| [`fontpixel.sfnt.collection`](python-api-reference.md#fontpixelsfntcollection) | 9 |
| [`fontpixel.sfnt.gasp`](python-api-reference.md#fontpixelsfntgasp) | 2 |
| [`fontpixel.sfnt.glyf`](python-api-reference.md#fontpixelsfntglyf) | 8 |
| [`fontpixel.sfnt.gsub`](python-api-reference.md#fontpixelsfntgsub) | 4 |
| [`fontpixel.sfnt.metrics`](python-api-reference.md#fontpixelsfntmetrics) | 4 |
| [`fontpixel.sfnt.name`](python-api-reference.md#fontpixelsfntname) | 8 |
| [`fontpixel.sfnt.os2`](python-api-reference.md#fontpixelsfntos2) | 9 |
| [`fontpixel.sfnt.post`](python-api-reference.md#fontpixelsfntpost) | 2 |
| [`fontpixel.sfnt.subset`](python-api-reference.md#fontpixelsfntsubset) | 2 |
| [`fontpixel.variants`](python-api-reference.md#fontpixelvariants) | 29 |
| [`fontpixel.woff`](python-api-reference.md#fontpixelwoff) | 1 |
| [`fontpixel.woff.woff1`](python-api-reference.md#fontpixelwoffwoff1) | 2 |
| [`fontpixel.woff.woff2`](python-api-reference.md#fontpixelwoffwoff2) | 5 |

127 modules, 1070 documented callables, classes and properties.

<!-- end reference index -->
