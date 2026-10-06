"""Pixel TrueType output/input of the bdfparser-compatible API (SPEC-v3 §8, §8.1).

Both directions run the Rust compatibility writer/reader; output is
deterministic and byte-identical to bdfparser's.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from ..._native import _bdfparser as _n

if TYPE_CHECKING:
    from .font import Font

__all__ = ['write_ttf', 'read_ttf']


def _ttf_options(
    *,
    shape: str = 'square',
    dotsize: float = 1.0,
    ppu: int = 64,
    metadata: bool = True,
    timestamp: int = 0,
) -> dict[str, Any]:
    if isinstance(ppu, bool) or not isinstance(ppu, int) or ppu < 2 or ppu % 2:
        raise ValueError(f'ppu must be an even integer >= 2, not {ppu!r}')
    if isinstance(timestamp, bool) or not isinstance(timestamp, int):
        raise ValueError(f'timestamp must be an int, not {timestamp!r}')
    if isinstance(dotsize, bool) or not isinstance(dotsize, (int, float)):
        raise ValueError(f'dotsize must be a number, not {dotsize!r}')
    return {
        'shape': shape,
        'dotsize': float(dotsize),
        'ppu': ppu,
        'metadata': bool(metadata),
        'timestamp': timestamp,
    }


def write_ttf(
    font: Font,
    *,
    shape: str = 'square',
    dotsize: float = 1.0,
    ppu: int = 64,
    metadata: bool = True,
    timestamp: int = 0,
) -> bytes:
    """
    Convert a font to a TrueType font with square or round pixel dots.

    :param shape: `'square'` or `'circle'` dots.
    :param dotsize: dot size relative to a pixel, `(0, 1.5]`.
    :param ppu: font units per pixel (even integer >= 2).
    :param metadata: embed the private `BDFP` table (lossless `read_ttf` round trip).
    :param timestamp: Unix seconds for `head.created` / `head.modified`.
    """

    opts = _ttf_options(
        shape=shape, dotsize=dotsize, ppu=ppu, metadata=metadata, timestamp=timestamp
    )
    try:
        return _n.write_ttf(font._raw, opts)
    finally:
        font._drain()


def read_ttf(data: bytes | bytearray | memoryview, pixelsize: int | None = None) -> Font:
    """
    Read a TrueType (`glyf`) font. Fonts written by `write_ttf` with
    `metadata=True` are restored losslessly; others are rasterized at
    `pixelsize` pixels per em (required then).
    """

    from .font import Font

    return Font._from_raw(_n.read_ttf(bytes(data), pixelsize))
