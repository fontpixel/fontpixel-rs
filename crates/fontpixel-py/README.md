# fontpixel (Python)

Bitmap font toolkit for Python on a Rust core: read, edit, render and write
bitmap fonts in 99 formats (BDF, PCF, PSF, YAFF, FNT/FON, BMFont, image
atlases, OpenType bitmap and outline fonts, WOFF/WOFF2, ...), with OpenType
`locl` variants, subsets, merging and every `fontpixel` command-line
operation available in-process. `fontpixel.compat.bdfparser` is a drop-in
replacement for [bdfparser](https://github.com/tomchen/bdfparser) 3.

- CPython 3.10+ (one stable-ABI wheel per platform), no runtime dependencies
- fonts stay in Rust; Python holds handles and copies pixels out on request
- typed: stubs for every native callable, `TypedDict`/`Literal` shapes for every
  option and data structure (`fontpixel.types`), `py.typed`
- memory-first API with explicit sidecar resources; filesystem mode in
  `fontpixel.ops`

## Install

Nothing is published to PyPI; build a wheel from the repository:

```sh
python -m pip install maturin
cd crates/fontpixel-py
maturin build --release
python -m pip install ../../target/wheels/fontpixel-*.whl
```

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

bdfparser code keeps working:

```python
from fontpixel.compat.bdfparser import Font

font = Font("spleen-5x8.bdf")
print(font.draw("Hello").shadow())
```

## Documentation

- [Python guide](../../docs/python-api.md): data model, options, errors,
  limits, resources, memory vs filesystem, typing, platform notes
- [API reference](../../docs/python-api-reference.md): every module, class,
  function, method and property with its typed signature
- [Examples](examples): runnable scripts for common tasks

MIT license.
