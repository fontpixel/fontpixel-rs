"""Bitmap outlines of the bdfparser-compatible API (SPEC-v3 §7.1, §7.2, §7.4).

`bitmap_contours` and `rasterize` run the Rust SPEC outline code. A contour is
a list of `(x, y, on_curve)` points in font units, y up.
"""

from __future__ import annotations

from collections.abc import Iterable, Sequence

from ..._native import _bdfparser as _n

__all__ = ['bitmap_contours', 'rasterize', 'Contour', 'Point']

Point = tuple[int, int, bool]
Contour = list[Point]

#: 1 / cos(22.5°)
K = 1.082392200292394
#: cos(45°)
D = 0.7071067811865476


def bitmap_contours(
    bindata: Sequence[str],
    shape: str = 'square',
    dotsize: float = 1.0,
    ppu: int = 1,
    xoff: int = 0,
    yoff: int = 0,
) -> list[Contour]:
    """
    Convert bitmap rows (top row first, any non-`'0'` pixel is ink) to
    TrueType-style contours: `shape` `'square'` (merged outline when
    `dotsize == 1`) or `'circle'`; `ppu` font units per pixel; `(xoff, yoff)`
    the bottom-left pixel position.
    """

    return _n.bitmap_contours(list(bindata), shape, float(dotsize), ppu, xoff, yoff)


def rasterize(
    contours: Iterable[Sequence[tuple[float, float, bool]]],
    ppu: float,
    xmin_px: int,
    ymin_px: int,
    w_px: int,
    h_px: int,
) -> list[str]:
    """
    Rasterize contours (nonzero winding, pixel centres, 16 chords per quadratic
    curve) into `h_px` rows of `w_px` pixels whose bottom-left pixel is at
    `(xmin_px, ymin_px)`.
    """

    cs = [[(float(x), float(y), bool(on)) for x, y, on in c] for c in contours]
    return _n.rasterize(cs, float(ppu), xmin_px, ymin_px, w_px, h_px).bindata
