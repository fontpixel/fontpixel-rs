"""bdfparser-compatible API: existing bdfparser code runs on the Rust core.

    python compat_bdfparser.py [INPUT] [OUTPUT_DIR]

`fontpixel.compat.bdfparser` provides bdfparser's `Font`, `Glyph` and
`Bitmap` (SPEC-v3) with the same behaviour; replacing
`from bdfparser import Font` by `from fontpixel.compat.bdfparser import Font`
is the whole migration. Writes `hello.txt`, `hello.bdf` and `hello.pcf` to
OUTPUT_DIR.
"""
from __future__ import annotations

import sys

from _common import arguments
from fontpixel.compat.bdfparser import Bitmap, Font


def main(argv: list[str] | None = None) -> int:
    source, out = arguments(__doc__ or "", argv)

    font = Font(str(source))                       # parse a BDF file path
    print("glyphs:", len(font), "| FONT:", font.headers["fontname"])
    print("properties:", {k: font.props[k] for k in list(font.props)[:3]})

    glyph = font.glyph("A")
    assert glyph is not None
    print("A:", glyph.meta["glyphname"], "bbx", glyph.meta["bbw"], "x", glyph.meta["bbh"])
    print(glyph.draw())                            # Bitmap; str() shows # and .

    # Draw text, then edit the bitmap with the bdfparser methods.
    hello = font.draw("Hello", linelimit=200)
    framed = hello.crop(hello.width() + 2, hello.height() + 2, -1, -1).enlarge(2, 2)
    print(framed.shadow(1, -1))
    (out / "hello.txt").write_text(str(framed))

    # Bitmaps can be built from strings and combined.
    tile = Bitmap(["010", "111", "010"])
    row = Bitmap.concatall([tile, tile, tile], 1, 1)
    print(row)

    # Serialize like bdfparser: BDF text and PCF bytes.
    small = Font()
    small.headers = dict(font.headers)
    small.props = dict(font.props)
    for code in range(0x48, 0x50):                 # H..O
        meta = font.glyphbycp(code)
        if meta is not None:
            small.addglyph(meta.meta)
    (out / "hello.bdf").write_text(small.tobdf())
    (out / "hello.pcf").write_bytes(small.topcf())
    print("subset written with", len(small), "glyphs")
    return 0


if __name__ == "__main__":
    sys.exit(main())
