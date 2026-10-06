"""Charset support of the bdfparser-compatible API (SPEC-v3 §4, §11.1).

Every function delegates to the native charset API (`fontpixel.CharsetCodec`,
`fontpixel.canonical_charset`, ... over `fontpixel::charset`, generated from
CPython's codecs): canonical keys, `CHARSET_REGISTRY` /
`CHARSET_ENCODING` folding, strict one-character conversions and GL forms.
"""

from __future__ import annotations

import warnings
from dataclasses import dataclass, field
from functools import cache
from typing import Any

import fontpixel as _fp
from .errors import BDFParserWarning

__all__ = [
    'CharsetCodec',
    'sanitize_charset',
    'canonical_charset',
    'charset_codec',
    'charset_props',
    'charset_table',
    'charset_map',
    'codec_name',
    'gb18030_ranges',
    'is_unicode_charset',
    'char2cp',
    'cp2char',
    'UNICODE_PREFIXES',
]

#: any canonical key starting with one of these is a Unicode charset too
UNICODE_PREFIXES = ('utf16', 'utf32', 'ucs2', 'ucs4')


_KINDS = {
    'unicode': 'unicode',
    'single_byte': 'sbcs',
    'double_byte': 'dbcs',
    'gb18030': 'gb18030',
}


@cache
def _rows() -> dict[str, dict[str, Any]]:
    return {r['key']: r for r in _fp.charset_table()}


def sanitize_charset(registry: object = None, encoding: object = None) -> str:
    """
    Canonical charset key from `CHARSET_REGISTRY` / `CHARSET_ENCODING` values
    (`'iso10646'` when both are absent).
    """

    reg = None if registry is None else str(registry)
    enc = None if encoding is None else str(encoding)
    return _fp.sanitize_charset(reg, enc)


def is_unicode_charset(key: str) -> bool:
    """Whether a canonical key denotes Unicode identity (UTF prefixes included)."""
    return _fp.is_unicode_charset(key)


def codec_name(key: str) -> str | None:
    """CPython codec name of a known non-Unicode key (`None` otherwise)."""
    row = _rows().get(key)
    return None if row is None else row['python_codec']


def canonical_charset(name: str) -> str:
    """Canonical key of any charset spelling (`'GB2312'`, `'Shift-JIS'`, ...)."""
    return _fp.canonical_charset(name)


def charset_props(key: str) -> tuple[str, str]:
    """`(CHARSET_REGISTRY, CHARSET_ENCODING)` written by `Font.charsetconvert`."""
    return tuple(_fp.charset_props(key))  # type: ignore[return-value]


def charset_table() -> dict[str, dict[str, Any]]:
    """
    The canonical charset table `{key: info}`; `info` has `codec`, `kind`
    (`'unicode'`, `'sbcs'`, `'dbcs'`, `'gb18030'`), `gl`, `forcegl`, `props` and
    `aliases`.
    """

    return {
        k: {
            'codec': r['python_codec'],
            'kind': _KINDS[r['kind']],
            'gl': r['gl_capable'],
            'forcegl': r['force_gl'],
            'props': tuple(r['properties']),
            'aliases': sorted(r['aliases']),
        }
        for k, r in _rows().items()
    }


@dataclass(frozen=True)
class CharsetCodec:
    """
    Converter between glyph codepoints and characters for one charset: `name`
    (canonical key), `codec` (CPython codec name, `None` for Unicode), `gl`
    (2-byte codes stored in GL form), `known` (unknown charsets map as Unicode).
    """

    name: str
    codec: str | None = None
    gl: bool = False
    known: bool = True
    _raw: Any = field(default=None, init=False, repr=False, compare=False, hash=False)

    def _native(self) -> Any:
        raw = self._raw
        if raw is None:
            raw = _fp.CharsetCodec.from_key(self.name, self.gl)
            object.__setattr__(self, '_raw', raw)
        return raw

    @property
    def kind(self) -> str:
        """`'unicode'`, `'sbcs'`, `'dbcs'` or `'gb18030'`."""
        if self.codec is None:
            return 'unicode'
        row = _rows().get(self.name)
        return _KINDS[row['kind']] if row else 'dbcs'

    def char2cp(self, char: str) -> int | None:
        """Codepoint of a single character, `None` if not representable."""
        if not isinstance(char, str) or len(char) != 1:
            raise TypeError(f'char2cp() expected a single character, got {char!r}')
        if self.codec is None:
            return ord(char)
        return self._native().unicode2cp(ord(char))

    def cp2char(self, cp: int) -> str | None:
        """Character of a codepoint, `None` if not representable."""
        if cp is None or cp < 0:
            return None
        if self.codec is None:
            return chr(cp) if cp <= 0x10FFFF else None
        if cp > 0xFFFFFFFF:
            return None
        n = self._native().cp2unicode(cp)
        return None if n is None else chr(n)


def charset_codec(key: str, gl: bool | None = None) -> CharsetCodec:
    """
    `CharsetCodec` for a canonical key. `gl=None`: GL form only for charsets that
    are always GL (`iso2022cn`); GL is ignored for charsets that are not 94x94.
    """

    raw = _fp.CharsetCodec.from_key(key, gl)
    name = raw.name
    row = _rows().get(name)
    codec = CharsetCodec(
        name,
        None if raw.kind == 'unicode' or not raw.known else (row['python_codec'] if row else None),
        raw.gl,
        raw.known,
    )
    object.__setattr__(codec, '_raw', raw)
    return codec


_warned_unknown: set[str] = set()


def _module_codec(charset: str, gl: bool | None) -> CharsetCodec:
    codec = charset_codec(canonical_charset(charset), gl)
    if not codec.known and codec.name not in _warned_unknown:
        _warned_unknown.add(codec.name)
        warnings.warn(
            f"Unknown charset '{codec.name}', characters are mapped to codepoints as Unicode",
            BDFParserWarning,
            stacklevel=3,
        )
    return codec


def char2cp(char: str, charset: str = 'iso10646', gl: bool | None = None) -> int | None:
    """Character -> codepoint in a charset (any spelling); `None` if not representable."""
    return _module_codec(charset, gl).char2cp(char)


def cp2char(cp: int, charset: str = 'iso10646', gl: bool | None = None) -> str | None:
    """Codepoint in a charset -> character; `None` if not representable."""
    return _module_codec(charset, gl).cp2char(cp)


def charset_map(key: str) -> dict[int, str]:
    """
    Every valid 1- and 2-byte code of a non-Unicode charset (non-GL form) ->
    its character, as `charset_codec(key, gl=False).cp2char` converts it.
    """

    codec = charset_codec(key, False)
    if codec.codec is None:
        raise ValueError(f'{key!r} is not a known non-Unicode charset')
    raw = codec._native()
    top = 0x100 if codec.kind == 'sbcs' else 0x10000
    out = {}
    for cp in range(top):
        n = raw.cp2unicode(cp)
        if n is not None:
            out[cp] = chr(n)
    return out


def gb18030_ranges() -> list[tuple[int, int, int]]:
    """
    The 4-byte GB18030 codes as runs `(index, codepoint, length)`, where for a
    code `b0 b1 b2 b3`:
    `index = (((b0 - 0x81) * 10 + (b1 - 0x30)) * 126 + (b2 - 0x81)) * 10 + (b3 - 0x30)`.
    """

    raw = charset_codec('gb18030')._native()

    def index(code: int) -> int:
        b = code.to_bytes(4, 'big')
        return (((b[0] - 0x81) * 10 + (b[1] - 0x30)) * 126 + (b[2] - 0x81)) * 10 + (b[3] - 0x30)

    runs: list[list[int]] = []
    for c in range(0x80, 0x10000):
        if 0xD800 <= c <= 0xDFFF:
            continue
        code = raw.unicode2cp(c)
        if code is None or code <= 0xFFFF:
            continue
        idx = index(code)
        if runs and runs[-1][0] + runs[-1][2] == idx and runs[-1][1] + runs[-1][2] == c:
            runs[-1][2] += 1
        else:
            runs.append([idx, c, 1])
    first = index(raw.unicode2cp(0x10000))
    runs.append([first, 0x10000, 0x100000])
    return [(a, b, n) for a, b, n in runs]
