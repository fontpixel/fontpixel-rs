"""`Bitmap` of the bdfparser-compatible API.

`Bitmap` is the native class `fontpixel._native._bdfparser.Bitmap`: the pixels
live in a Rust raster of labels 0, 1 and 2; `bindata` builds the row strings on
request. Rows must be rectangular and made of `'0'`, `'1'` and `'2'`.
"""

from __future__ import annotations

from ..._native import _bdfparser as _n

__all__ = ['Bitmap', 'fmtnum']

Bitmap = _n.Bitmap


def fmtnum(n: float) -> str:
    """
    Shortest text of a number as written in SVG output: integral values without
    a fraction (`3.0` -> `'3'`, `-0.0` -> `'0'`), others as Python `repr`.
    """

    # `+ 0.0` turns -0.0 into 0.0 (bdfparser writes '0')
    return _n.fmtnum(float(n) + 0.0)
