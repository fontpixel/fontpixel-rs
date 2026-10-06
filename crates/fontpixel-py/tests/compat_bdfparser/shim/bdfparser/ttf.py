"""Shim `bdfparser.ttf`: `fontpixel.compat.bdfparser.ttf` plus test-only helpers.

The reference tests import four private helpers of bdfparser's `ttf` module.
They are not part of bdfparser's public API and fontpixel has no public
equivalent, so they are provided here for the tests only:

- `_bindata(meta)`: a glyph's rows padded/cut to its BBX; implemented with the
  compatibility `Glyph.draw(mode=1)` (Rust).
- `_jsnum(v)`, `_parse_tables(data)`, `_parse_cmap(data)`: test inspection
  helpers ported from bdfparser 3.0 (MIT, Copyright (c) 2017-2026 Tom CHEN).
  Tests of these helpers exercise this shim, not fontpixel; see
  KNOWN_DIFFERENCES.md.
"""

from __future__ import annotations

import struct
import warnings
from decimal import Decimal
from typing import Any

from fontpixel.compat.bdfparser.font import Font
from fontpixel.compat.bdfparser.glyph import META_TITLES, Glyph
from fontpixel.compat.bdfparser.ttf import *  # noqa: F401,F403
from fontpixel.compat.bdfparser.ttf import read_ttf, write_ttf  # noqa: F401

_EMPTY = Font()


def _bindata(meta: Any) -> list[str]:
    """Equivalent of `Glyph.draw(mode=1).bindata` for a 17-slot meta list."""
    bbw = meta[2] or 0
    bbh = meta[3] or 0
    if bbw <= 0 or bbh <= 0:
        return []
    g = Glyph(dict(zip(META_TITLES, meta, strict=False)), _EMPTY)
    with warnings.catch_warnings():
        warnings.simplefilter('ignore')
        return g.draw(1).bindata


def _jsnum(v: float) -> str:
    """Text of JS `String(v)` (ECMAScript Number::toString) for a finite number."""
    f = float(v)
    if f.is_integer() and abs(f) < 2**53:
        return str(int(f))
    sign, digits, exp = Decimal(repr(f)).as_tuple()
    assert isinstance(exp, int)
    ds = ''.join(map(str, digits))
    n = exp + len(ds)
    ds = ds.rstrip('0') or '0'
    k = len(ds)
    neg = '-' if sign else ''
    if k <= n <= 21:
        return neg + ds + '0' * (n - k)
    if 0 < n <= 21:
        return neg + ds[:n] + '.' + ds[n:]
    if -6 < n <= 0:
        return neg + '0.' + '0' * (-n) + ds
    e = n - 1
    es = ('+' if e >= 0 else '-') + str(abs(e))
    if k == 1:
        return neg + ds + 'e' + es
    return neg + ds[0] + '.' + ds[1:] + 'e' + es


class _Reader:
    def __init__(self, data: bytes, name: str) -> None:
        self.data = data
        self.name = name

    def need(self, off: int, size: int) -> None:
        if off < 0 or off + size > len(self.data):
            raise ValueError(f'Malformed TrueType font: {self.name} table is truncated')

    def unpack(self, fmt: str, off: int) -> tuple[Any, ...]:
        size = struct.calcsize(fmt)
        self.need(off, size)
        return struct.unpack_from(fmt, self.data, off)


def _parse_tables(data: bytes) -> dict[str, bytes]:
    if len(data) < 12:
        raise ValueError('Malformed TrueType font: file too short')
    tag = data[:4]
    if tag == b'OTTO':
        raise ValueError('CFF-based OpenType is not supported')
    if tag not in (b'\x00\x01\x00\x00', b'true'):
        raise ValueError(f'Not a TrueType font (sfnt version {tag!r})')
    (num,) = struct.unpack_from('>H', data, 4)
    if 12 + 16 * num > len(data):
        raise ValueError('Malformed TrueType font: table directory is truncated')
    tables: dict[str, bytes] = {}
    for i in range(num):
        t, _cs, off, ln = struct.unpack_from('>4sIII', data, 12 + 16 * i)
        if off + ln > len(data):
            raise ValueError(
                f'Malformed TrueType font: table {t.decode("latin-1")!r} exceeds the file'
            )
        tables[t.decode('latin-1')] = data[off : off + ln]
    for req in ('head', 'maxp', 'hhea', 'hmtx', 'loca', 'glyf'):
        if req not in tables:
            raise ValueError(f'Malformed TrueType font: missing required table {req!r}')
    return tables


def _parse_cmap(data: bytes) -> dict[int, int]:
    r = _Reader(data, 'cmap')
    _ver, n = r.unpack('>HH', 0)
    recs = []
    for i in range(n):
        pid, eid, off = r.unpack('>HHI', 4 + 8 * i)
        recs.append((pid, eid, off))
    order: list[tuple[int, int, int]] = []
    for pref in ((3, 10), (0, 4), (0, 6), (3, 1), (0, 3)):
        order += [x for x in recs if (x[0], x[1]) == pref]
    order += [x for x in recs if x not in order]
    for _pid, _eid, off in order:
        (fmt,) = r.unpack('>H', off)
        if fmt in (0, 4, 6, 12):
            return _parse_cmap_sub(r, off, fmt)
    return {}


def _parse_cmap_sub(r: _Reader, off: int, fmt: int) -> dict[int, int]:
    m: dict[int, int] = {}
    if fmt == 0:
        r.need(off + 6, 256)
        for c in range(256):
            g = r.data[off + 6 + c]
            if g:
                m[c] = g
    elif fmt == 6:
        first, count = r.unpack('>HH', off + 6)
        gids = r.unpack(f'>{count}H', off + 10)
        for i, g in enumerate(gids):
            if g:
                m[first + i] = g
    elif fmt == 4:
        (segx2,) = r.unpack('>H', off + 6)
        n = segx2 // 2
        ends = r.unpack(f'>{n}H', off + 14)
        starts = r.unpack(f'>{n}H', off + 16 + segx2)
        deltas = r.unpack(f'>{n}H', off + 16 + 2 * segx2)
        roff_base = off + 16 + 3 * segx2
        roffs = r.unpack(f'>{n}H', roff_base)
        for i in range(n):
            s, e, d, ro = starts[i], ends[i], deltas[i], roffs[i]
            if s > e:
                continue
            for c in range(s, e + 1):
                if c == 0xFFFF:
                    continue
                if ro == 0:
                    g = (c + d) & 0xFFFF
                else:
                    (g,) = r.unpack('>H', roff_base + 2 * i + ro + 2 * (c - s))
                    if g:
                        g = (g + d) & 0xFFFF
                if g:
                    m[c] = g
    else:
        (ngroups,) = r.unpack('>I', off + 12)
        r.need(off + 16, 12 * ngroups)
        for i in range(ngroups):
            s, e, sg = r.unpack('>III', off + 16 + 12 * i)
            if e < s or e > 0x10FFFF:
                raise ValueError('Malformed TrueType font: bad cmap format 12 group')
            for c in range(s, e + 1):
                g = sg + c - s
                if g:
                    m[c] = g
    return m
