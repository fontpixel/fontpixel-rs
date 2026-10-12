# fontpixel (Python)

Bitmap font toolkit for Python on a Rust core: read, edit, render and write
bitmap fonts in 99 formats (BDF, PCF, PSF, YAFF, FNT/FON, BMFont, image
atlases, OpenType bitmap and outline fonts, WOFF/WOFF2, ...), with OpenType
`locl` variants, subsets, merging and every `fontpixel` command-line
operation available in-process. `fontpixel.compat.bdfparser` is a compatible
replacement for [bdfparser](https://github.com/tomchen/bdfparser) 3.

- CPython 3.10+ (one stable-ABI wheel per platform), no runtime dependencies
- fonts stay in Rust; Python holds handles and copies pixels out on request
- typed: stubs for every native callable, `TypedDict`/`Literal` shapes for every
  option and data structure (`fontpixel.types`), `py.typed`
- memory-first API with explicit sidecar resources; filesystem mode in
  `fontpixel.ops`

## Install

```sh
pip install fontpixel
```

Wheels: Linux (manylinux and musllinux, x86_64 and aarch64), macOS (x86_64
and arm64), Windows (x64). Elsewhere pip builds the source distribution, which
needs a Rust toolchain (1.99 or newer).

## Quick start

```python
import fontpixel as fp

fonts = fp.read(open("spleen-5x8.bdf", "rb").read(), name="spleen-5x8.bdf").fonts
font = fonts[0]                                   # a view into the FontSet
print(len(font), font.names.get("family"), font.metrics.get("pixel_size"))

glyph = font.glyph_for_codepoint("A")
print(glyph.bitmap.to_text())                     # .##.. / #..#. / ...

rendered = font.draw("Hello", {"letter_spacing": 1})
open("hello.png", "wb").write(fp.to_png(rendered.bitmap, {"scale": 4}))

pcf = fp.write(fonts, "pcf", {"glyph_pad": "4"})  # options are typed dicts
open("spleen.pcf", "wb").write(pcf.output.into_single())

from fontpixel import ops                         # the command line, in-process
result = ops.convert(inputs=["s.bdf"], output="s.ttf", to="ttf",
                     files={"s.bdf": open("spleen-5x8.bdf", "rb").read()})
ttf = result.outputs["s.ttf"]
```

## bdfparser compatibility

`fontpixel.compat.bdfparser` is the API of
[bdfparser](https://github.com/tomchen/bdfparser) 3.x on the Rust core
(`Font`, `Glyph`, `Bitmap`, the encoding helpers, PCF and TrueType reading
and writing, the outline functions and the command line). Existing code only
changes its import:

```python
from fontpixel.compat.bdfparser import Font      # was: from bdfparser import Font

font = Font("spleen-5x8.bdf")
print(font.draw("Hello").shadow())
```

The submodules `fontpixel.compat.bdfparser.{bitmap, cli, encoding, errors,
font, glyph, outline, pcf, ttf}` mirror bdfparser's, and
`python -m fontpixel.compat.bdfparser` is its command line. This import path
is stable within fontpixel 0.x; bdfparser 4.x is a thin package over it.
The behaviour differences from bdfparser 3.x are listed in
[bdfparser-compat-differences.md](https://github.com/fontpixel/fontpixel-rs/blob/main/docs/bdfparser-compat-differences.md).

## Documentation

- [Python guide](https://github.com/fontpixel/fontpixel-rs/blob/main/docs/python-api.md):
  data model, options, errors, limits, resources, memory vs filesystem,
  typing, platform notes
- [API reference](https://github.com/fontpixel/fontpixel-rs/blob/main/docs/python-api-reference.md):
  every module, class, function, method and property with its typed signature
- [Examples](https://github.com/fontpixel/fontpixel-rs/tree/main/crates/fontpixel-py/examples):
  runnable scripts for common tasks
- [Changelog](https://github.com/fontpixel/fontpixel-rs/blob/main/CHANGELOG.md)

## License

MIT. The charset tables derive from CPython's codecs and some semantics from
other projects; the wheels carry `THIRD_PARTY_NOTICES.md` and `LICENSES/`.
