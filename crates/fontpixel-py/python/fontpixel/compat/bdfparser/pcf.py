"""PCF input/output of the bdfparser-compatible API (SPEC-v3 §6).

Reading and writing use the native, bounded PCF codec. With default options the
output is byte-identical to `bdftopcf` run on the same BDF.
"""

from __future__ import annotations

import gzip
import io
import zlib
from typing import TYPE_CHECKING, Any

from ..._native import _bdfparser as _n
from .errors import BDFParseError

if TYPE_CHECKING:
    from .font import Font

__all__ = ['PCFError', 'read_pcf', 'write_pcf']


class PCFError(BDFParseError):
    """
    Raised for malformed PCF data or fonts that cannot be written as PCF.
    Subclass of `BDFParseError` (itself a `ValueError`).
    """


def _order(value: Any, name: str) -> str:
    v = str(value).lower()
    if v in ('msb', 'big', 'm'):
        return 'msb'
    if v in ('lsb', 'little', 'l'):
        return 'lsb'
    raise ValueError(f"{name} must be 'msb' or 'lsb', got {value!r}")


def _pcf_options(
    *,
    byteorder: str = 'msb',
    bitorder: str = 'msb',
    glyphpad: int = 4,
    scanunit: int = 1,
) -> dict[str, Any]:
    msb = _order(byteorder, 'byteorder')
    bitmsb = _order(bitorder, 'bitorder')
    if glyphpad not in (1, 2, 4, 8):
        raise ValueError(f'glyphpad must be 1, 2, 4 or 8, got {glyphpad!r}')
    if scanunit not in (1, 2, 4):
        raise ValueError(f'scanunit must be 1, 2 or 4, got {scanunit!r}')
    if bitmsb != msb and scanunit > glyphpad:
        raise ValueError(
            'scanunit must not exceed glyphpad when bitorder != byteorder '
            '(bdftopcf output is undefined for that combination)'
        )
    return {
        'byte_order': msb,
        'bit_order': bitmsb,
        'glyph_pad': str(glyphpad),
        'scan_unit': str(scanunit),
    }


def write_pcf(
    font: Font,
    *,
    byteorder: str = 'msb',
    bitorder: str = 'msb',
    glyphpad: int = 4,
    scanunit: int = 1,
) -> bytes:
    """
    Convert a font to PCF bytes. `byteorder`/`bitorder`: `'msb'` or `'lsb'`
    (bdftopcf -M/-L, -m/-l); `glyphpad`: 1, 2, 4 or 8 (-p); `scanunit`: 1, 2 or
    4 (-u).
    """

    from .font import Font

    opts = _pcf_options(
        byteorder=byteorder, bitorder=bitorder, glyphpad=glyphpad, scanunit=scanunit
    )
    font = Font._coerce(font)
    font._check_slots()
    try:
        return font._raw.topcf(opts)
    finally:
        font._drain()


def read_pcf(data: bytes | bytearray | memoryview) -> Font:
    """Parse PCF (optionally gzip-compressed) bytes into a `Font`; `PCFError` on malformed input."""
    from .font import Font

    data = bytes(data)
    try:
        head = _head(data)
    except (OSError, EOFError, zlib.error) as e:
        raise PCFError(f'invalid gzip data: {e}') from e
    if head != PCF_MAGIC:
        raise PCFError('not a PCF file (bad magic)')
    raw = _n.RawFont()
    _load_pcf(raw, data)
    return Font._from_raw(raw)


PCF_MAGIC = b'\x01fcp'


def _head(data: bytes) -> bytes:
    """First 4 bytes of the (possibly gzip-compressed) data."""
    if data[:2] == b'\x1f\x8b':
        with gzip.GzipFile(fileobj=io.BytesIO(data)) as g:
            return g.read(4)
    return data[:4]


def _load_pcf(raw: Any, data: bytes) -> None:
    """Load PCF (or gzip PCF) bytes into a native handle; errors are `PCFError`s."""
    try:
        raw.load_bytes(data, None)
    except PCFError:
        raise
    except ValueError as e:
        raise PCFError(str(e)) from e
