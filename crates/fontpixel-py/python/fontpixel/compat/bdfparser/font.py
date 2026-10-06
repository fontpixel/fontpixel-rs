"""`Font` of the bdfparser-compatible API (handle over the Rust compat font).

The font data stays in the Rust model (`fontpixel::compat::bdfparser::Font`).
`headers`, `props`, `glyphs` and `unencoded` are live views: reading them
projects the current model, writing through them edits it in place (glyph
handles stay attached). Values read from them are fresh copies (editing a
returned list does not change the font, except for the 17-slot meta lists of
`font.glyphs[cp]` and `font.unencoded[i]['meta']`, whose item assignment is
written back). Warnings collected by the Rust model are re-emitted as
`BDFParserWarning`s through the `warnings` module after each call.
"""

from __future__ import annotations

import io
import os
import warnings
from collections.abc import Callable, Iterable, Iterator, Mapping, MutableMapping
from typing import Any, Union

from ..._native import _bdfparser as _n
from .bitmap import Bitmap
from .encoding import canonical_charset, is_unicode_charset
from .errors import BDFParserWarning
from .glyph import META_TITLES, Glyph

__all__ = ['Font']

Range = Union[int, tuple[int, int], list[tuple[int, int]]]

_GZIP_MAGIC = b'\x1f\x8b'


def fmtver(v: float) -> str:
    """`2` -> `'2.0'`, `2.1` -> `'2.1'` (BDF version number formatting)."""
    return _n.fmtver(float(v))


def _range(r: Any) -> Any:
    if r is None or isinstance(r, int):
        return r
    if isinstance(r, tuple):
        return list(r)
    return [list(t) for t in r]


class _HeadersView(MutableMapping):
    """Live view of `Font.headers` (writes edit the native model in place)."""

    def __init__(self, font: Font) -> None:
        self._font = font

    def _data(self) -> dict[str, Any]:
        return self._font._raw.headers()

    def __getitem__(self, key: str) -> Any:
        return self._data()[key]

    def __setitem__(self, key: str, value: Any) -> None:
        self._font._raw.set_header(key, value)
        self._font._changed()

    def __delitem__(self, key: str) -> None:
        if not self._font._raw.remove_header(key):
            raise KeyError(key)
        self._font._changed()

    def __iter__(self) -> Iterator[str]:
        return iter(list(self._data()))

    def __len__(self) -> int:
        return len(self._data())

    def __repr__(self) -> str:
        return repr(self._data())

    def copy(self) -> dict[str, Any]:
        return self._data()

    def update(self, other: Any = (), /, **kwargs: Any) -> None:
        d = self._data()
        d.update(other, **kwargs)
        self._font._set_headers(d)


class _PropsView(MutableMapping):
    """Live view of `Font.props` (lower-case keys; writes call `setprop`)."""

    def __init__(self, font: Font) -> None:
        self._font = font

    def _data(self) -> dict[str, Any]:
        return self._font._raw.props()

    def __getitem__(self, key: str) -> Any:
        return self._data()[key]

    def __setitem__(self, key: str, value: Any) -> None:
        names = self._font._propnames
        self._font._raw.setprop(names.get(key, key.upper()) if key != 'comment' else key, value)
        self._font._changed()

    def __delitem__(self, key: str) -> None:
        if key not in self._data():
            raise KeyError(key)
        self._font._raw.delprop(key)
        self._font._changed()

    def __iter__(self) -> Iterator[str]:
        return iter(list(self._data()))

    def __len__(self) -> int:
        return len(self._data())

    def __repr__(self) -> str:
        return repr(self._data())

    def copy(self) -> dict[str, Any]:
        return self._data()


class _MetaList(list):
    """A glyph's 17 meta values; item assignment is written back to the font."""

    def __init__(self, font: Font, cp: int, values: Iterable[Any]) -> None:
        super().__init__(values)
        self._font = font
        self._cp = cp

    def __setitem__(self, index: Any, value: Any) -> None:
        super().__setitem__(index, value)
        if isinstance(index, int) and -17 <= index < 17:
            raw = self._font._raw.lookup(self._cp)
            if raw is None:
                return
            raw.setmeta({META_TITLES[index]: value})
            self._font._changed()


class _UnencodedMetaList(list):
    """An unencoded glyph's 17 meta values; item assignment is written back to
    that glyph (through an attached handle, so it follows the glyph)."""

    def __init__(self, font: Font, handle: Any, values: Iterable[Any]) -> None:
        super().__init__(values)
        self._font = font
        self._g = handle

    def __setitem__(self, index: Any, value: Any) -> None:
        super().__setitem__(index, value)
        if isinstance(index, int) and -17 <= index < 17 and self._g.is_attached():
            self._g.setmeta({META_TITLES[index]: value})
            self._font._changed()


class _GlyphsView(MutableMapping):
    """Live view of `Font.glyphs`: codepoint -> 17-slot meta list, file order."""

    def __init__(self, font: Font) -> None:
        self._font = font

    def __getitem__(self, cp: int) -> list:
        return self._font._meta(cp)

    def __contains__(self, cp: object) -> bool:
        return cp in self._font._cpset()

    def __setitem__(self, cp: int, meta: Iterable[Any]) -> None:
        meta = list(meta)
        fields = dict(zip(META_TITLES, meta, strict=False))
        fields['codepoint'] = cp
        self._font._raw.addglyph(fields, True)
        # the native model always holds 17 slots; remember a shorter list for
        # the PCF writer's check (bdfparser stores the list as given)
        if len(meta) < len(META_TITLES):
            self._font._short.add(cp)
        else:
            self._font._short.discard(cp)
        self._font._changed()

    def __delitem__(self, cp: int) -> None:
        if cp not in self:
            raise KeyError(cp)
        self._font._raw.removeglyph(cp)
        self._font._short.discard(cp)
        self._font._changed()

    def __iter__(self) -> Iterator[int]:
        return iter(self._font._cplist())

    def __len__(self) -> int:
        return self._font._raw.length()

    def items(self) -> list[tuple[int, list]]:  # type: ignore[override]
        return list(self._font._model()['items'])

    def values(self) -> list[list]:  # type: ignore[override]
        return [m for _, m in self.items()]

    def clear(self) -> None:
        for cp in self._font._cplist():
            self._font._raw.removeglyph(cp)
        self._font._short.clear()
        self._font._changed()

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Mapping):
            return NotImplemented
        return dict(self.items()) == dict(other.items())

    def __repr__(self) -> str:
        return repr(dict(self.items()))

    def copy(self) -> dict[int, list]:
        return {cp: list(m) for cp, m in self.items()}


class _UnencodedView(list):
    """Snapshot of `Font.unencoded` (`{'altcode', 'meta'}` dicts; item assignment
    into a `meta` list is written back); `append`, `extend`, `clear` and `del`
    edit the font."""

    def __init__(self, font: Font) -> None:
        super().__init__(font._unencoded())
        self._font = font

    def append(self, entry: Mapping[str, Any] | Iterable[Any]) -> None:
        if not isinstance(entry, Mapping):  # a bare meta list (accepted by bdfparser writers)
            entry = {'altcode': None, 'meta': list(entry)}
        meta = list(entry['meta'])
        fields = dict(zip(META_TITLES, meta, strict=False))
        fields['codepoint'] = -1
        altcode = entry.get('altcode')
        try:
            self._font._raw.put_with_alt(fields, altcode)
        finally:
            self._font._changed()
        super().append({'altcode': altcode, 'meta': meta})

    def extend(self, entries: Iterable[Mapping[str, Any]]) -> None:
        for e in entries:
            self.append(e)

    def clear(self) -> None:
        self._font._raw.remove_unencoded(None)
        self._font._changed()
        super().clear()

    def __delitem__(self, index: Any) -> None:
        n = len(self)
        indexes = range(n)[index] if isinstance(index, slice) else [range(n)[index]]
        for i in sorted(indexes, reverse=True):
            self._font._raw.remove_unencoded(i)
        self._font._changed()
        super().__delitem__(index)


class Font:
    """
    `Font` object

    https://font.tomchen.org/bdfparser_py/font
    """

    def __init__(self, source: Any = None) -> None:
        """
        Initialize a `Font`. `source` (optional) may be a file path (`str` or
        `os.PathLike`), a text or binary file object, or `bytes` (BDF, PCF or TTF,
        auto-detected).

        https://font.tomchen.org/bdfparser_py/font#font
        """

        self._raw = _n.RawFont()
        self._wcur = 0
        self._rev = 0
        self._epoch = 0
        self._cache: dict[str, Any] | None = None
        self._short: set[int] = set()
        self._char2cp_override: Callable[[str], int | None] | None = None
        self._cp2char_override: Callable[[int], str | None] | None = None
        self._charset_override: str | None = None
        if source is None:
            return
        if isinstance(source, (str, os.PathLike)):
            self.load_file_path(source)
        elif isinstance(source, (bytes, bytearray, memoryview)):
            self.load_bytes(source)
        elif hasattr(source, 'read') or isinstance(source, io.IOBase):
            self.load_file_obj(source)
        else:
            raise TypeError(f'Cannot load a font from {type(source).__name__}')

    @classmethod
    def _from_raw(cls, raw: Any, like: Font | None = None) -> Font:
        font = cls()
        font._raw = raw
        if like is not None:
            font._char2cp_override = like._char2cp_override
            font._cp2char_override = like._cp2char_override
            font._charset_override = like._charset_override
        font._drain()
        return font

    # -- plumbing -----------------------------------------------------------

    def _drain(self) -> None:
        msgs, self._wcur = self._raw.warnings_since(self._wcur)
        for m in msgs:
            warnings.warn(m, BDFParserWarning, stacklevel=3)

    def _changed(self) -> None:
        self._rev += 1
        self._drain()

    def _state(self) -> dict[str, Any]:
        # projections of the native model, reused until the next edit through
        # this wrapper (so repeated reads return the same list objects, as with
        # bdfparser's stored lists)
        st = self._cache
        if st is None or st['rev'] != self._rev:
            st = {
                'rev': self._rev,
                'cps': None,
                'metas': {},
                'items': None,
                'unencoded': None,
                'entries': None,
            }
            self._cache = st
        return st

    def _cps(self) -> tuple[list[int], frozenset[int]]:
        st = self._state()
        if st['cps'] is None:
            cps = self._raw.itercps(0, None)
            st['cps'] = (cps, frozenset(cps))
        return st['cps']

    def _cplist(self) -> list[int]:
        return list(self._cps()[0])

    def _cpset(self) -> frozenset[int]:
        return self._cps()[1]

    def _meta(self, cp: int) -> list:
        st = self._state()
        m = st['metas'].get(cp)
        if m is None:
            if cp not in self._cpset():
                raise KeyError(cp)
            m = _MetaList(self, cp, self._raw.lookup(cp).meta())
            st['metas'][cp] = m
        return m

    def _model(self) -> dict[str, Any]:
        st = self._state()
        if st['items'] is None:
            model = self._raw.model()
            metas = st['metas']
            st['items'] = [
                (cp, metas.setdefault(cp, _MetaList(self, cp, m))) for cp, m in model['glyphs']
            ]
            st['unencoded'] = [{'altcode': alt, 'meta': m} for alt, m in model['unencoded']]
            st['propnames'] = model['propnames']
        return st

    def _unencoded(self) -> list[dict[str, Any]]:
        """`unencoded` entries whose `meta` lists write back through handles."""
        st = self._model()
        if st['entries'] is None:
            handles = self._raw.unencoded_handles()
            st['entries'] = [
                {'altcode': e['altcode'], 'meta': _UnencodedMetaList(self, g, e['meta'])}
                for e, g in zip(st['unencoded'], handles, strict=True)
            ]
        return st['entries']

    def _set_raw(self, raw: Any) -> Font:
        self._raw = raw
        self._short = set()
        self._epoch += 1
        self._wcur = 0
        if self._charset_override:
            raw.encoding(self._charset_override)
        self._apply_converters()
        self._changed()
        return self

    def _apply_converters(self) -> None:
        if self._char2cp_override is not None or self._cp2char_override is not None:
            self._raw.set_converters(self._char2cp_override, self._cp2char_override)

    def _set_headers(self, headers: Mapping[str, Any]) -> None:
        self._raw.set_headers(dict(headers))
        self._changed()

    # -- data views ---------------------------------------------------------

    @property
    def headers(self) -> MutableMapping[str, Any]:
        """Global headers (`bdfversion`, `fontname`, `pointsize`, `fbbx`, ...)."""
        return _HeadersView(self)

    @headers.setter
    def headers(self, value: Mapping[str, Any]) -> None:
        self._set_headers(dict(value))

    @property
    def props(self) -> MutableMapping[str, Any]:
        """Properties (lower-case keys, `comment` included)."""
        return _PropsView(self)

    @props.setter
    def props(self, value: Mapping[str, Any]) -> None:
        value = dict(value)
        for k in list(self._raw.props()):
            self._raw.delprop(k)
        for k, v in value.items():
            self._raw.setprop(k.upper() if k != 'comment' else k, v)
        self._changed()

    @property
    def glyphs(self) -> MutableMapping[int, list]:
        """Encoded glyphs: codepoint -> 17 meta values (`META_TITLES` order)."""
        return _GlyphsView(self)

    @glyphs.setter
    def glyphs(self, value: Mapping[int, Iterable[Any]]) -> None:
        items = [(cp, list(m)) for cp, m in value.items()]
        view = _GlyphsView(self)
        view.clear()
        for cp, m in items:
            view[cp] = m

    @property
    def unencoded(self) -> list[dict[str, Any]]:
        """Unencoded glyphs (`ENCODING -1`) as `{'altcode': int | None, 'meta': list}`."""
        return _UnencodedView(self)

    @unencoded.setter
    def unencoded(self, value: Iterable[Mapping[str, Any]]) -> None:
        entries = [dict(e) for e in value]
        view = _UnencodedView(self)
        view.clear()
        view.extend(entries)

    @property
    def _propnames(self) -> dict[str, str]:
        return dict(self._model()['propnames'])

    @classmethod
    def _coerce(cls, font: Any) -> Font:
        """A compatibility `Font` for bdfparser's duck-typed writer input (any
        object with `headers`, `props`, `glyphs` and optionally `unencoded`,
        `_propnames`)."""
        if isinstance(font, Font):
            return font
        out = cls()
        out.headers = dict(font.headers)
        names = dict(getattr(font, '_propnames', None) or {})
        for k, v in dict(font.props).items():
            out._raw.setprop(k if k == 'comment' else names.get(k, k.upper()), v)
        view = out.glyphs
        for cp, m in dict(font.glyphs).items():
            view[cp] = list(m)
        out.unencoded.extend(getattr(font, 'unencoded', None) or [])
        out._changed()
        return out

    # -- loading -------------------------------------------------------------

    def load_file_path(self, file_path: str | os.PathLike) -> Font:
        """
        Load a font file (BDF, PCF, `.pcf.gz` or TTF, auto-detected by content).

        https://font.tomchen.org/bdfparser_py/font#load_file_path
        """

        from . import pcf

        raw = _n.RawFont()
        try:
            raw.load_file_path(os.fspath(file_path))
        except pcf.PCFError:
            raise
        except ValueError as e:
            try:
                with open(file_path, 'rb') as f:
                    is_pcf = pcf._head(f.read(1 << 16)) == pcf.PCF_MAGIC
            except Exception:
                is_pcf = False
            if is_pcf:  # malformed PCF data raises PCFError, as bdfparser
                raise pcf.PCFError(str(e)) from e
            raise
        return self._set_raw(raw)

    def load_file_obj(self, file_obj: Any) -> Font:
        """
        Load a font from a file object: a text file is parsed as BDF, a binary
        file is auto-detected (BDF, PCF, TTF).

        https://font.tomchen.org/bdfparser_py/font#load_file_obj
        """

        if isinstance(file_obj, io.TextIOBase) or not hasattr(file_obj, 'read'):
            return self.load_lines(file_obj)
        if isinstance(file_obj, (io.RawIOBase, io.BufferedIOBase)):
            # binary stream: bounded chunked reads in Rust
            raw = _n.RawFont()
            raw.load_file_obj(file_obj)
            return self._set_raw(raw)
        data = file_obj.read()
        if isinstance(data, str):
            return self.load_string(data)
        return self.load_bytes(data)

    def load_string(self, text: str) -> Font:
        """
        Load a BDF font from a `str`.

        https://font.tomchen.org/bdfparser_py/font#load_string
        """

        if text.startswith('﻿'):
            text = text[1:]
        raw = _n.RawFont()
        raw.load_string(text)
        return self._set_raw(raw)

    def load_lines(self, lines: Iterable[str]) -> Font:
        """
        Load a BDF font from an iterable of lines (`str`s, line endings optional).

        https://font.tomchen.org/bdfparser_py/font#load_lines
        """

        rows = [line.rstrip('\r\n') for line in lines]
        if rows and rows[0].startswith('﻿'):
            rows[0] = rows[0][1:]
        raw = _n.RawFont()
        raw.load_lines(rows)
        return self._set_raw(raw)

    def load_bytes(self, data: bytes | bytearray | memoryview, **ttfoptions: Any) -> Font:
        """
        Load a font from `bytes`, auto-detected: PCF, gzip-compressed PCF/BDF,
        TrueType (`pixelsize=` may be needed, see `read_ttf`), else BDF (UTF-8).

        https://font.tomchen.org/bdfparser_py/font#load_bytes
        """

        unknown = set(ttfoptions) - {'pixelsize'}
        if unknown:
            raise TypeError(f'load_bytes() got unexpected keyword arguments {sorted(unknown)}')
        data = bytes(data)
        raw = _n.RawFont()
        if data[:2] == _GZIP_MAGIC:
            from . import pcf

            try:
                is_pcf = pcf._head(data) == pcf.PCF_MAGIC
            except Exception:  # corrupt gzip: reported by the native loader
                is_pcf = False
            if is_pcf:  # gzip-wrapped PCF: malformed data raises PCFError, as bdfparser
                pcf._load_pcf(raw, data)
                return self._set_raw(raw)
        raw.load_bytes(data, ttfoptions.get('pixelsize'))
        return self._set_raw(raw)

    @classmethod
    def create(
        cls,
        pixelsize: int = 16,
        ascent: int | None = None,
        descent: int | None = None,
        fontname: str | None = None,
        familyname: str = 'Untitled',
        xres: int = 75,
        yres: int = 75,
    ) -> Font:
        """
        Create a new empty font.

        https://font.tomchen.org/bdfparser_py/font#fontcreate
        """

        raw = _n.RawFont.create(
            {
                'pixelsize': pixelsize,
                'ascent': ascent,
                'descent': descent,
                'fontname': fontname,
                'familyname': familyname,
                'xres': xres,
                'yres': yres,
            }
        )
        return cls._from_raw(raw)

    # -- charsets ------------------------------------------------------------

    def encoding(
        self,
        charset: str | None = None,
        char2cp: Callable[[str], int | None] | None = None,
        cp2char: Callable[[int], str | None] | None = None,
    ) -> Font:
        """
        Override the charset resolution (`charset`, any spelling; `''` clears
        the override) and/or the character <-> codepoint conversion functions.

        https://font.tomchen.org/bdfparser_py/font#encoding
        """

        if charset is not None:
            self._charset_override = canonical_charset(charset) if charset else None
            self._raw.encoding(self._charset_override or '')
        if char2cp is not None:
            self._char2cp_override = char2cp
        if cp2char is not None:
            self._cp2char_override = cp2char
        if char2cp is not None or cp2char is not None:
            self._apply_converters()
        self._changed()
        return self

    @property
    def charset(self) -> str:
        """
        Canonical charset key: the `encoding(charset=...)` override if set, else
        derived from `CHARSET_REGISTRY` / `CHARSET_ENCODING` (`'iso10646'` if absent).

        https://font.tomchen.org/bdfparser_py/font#charset
        """

        return self._raw.charset()

    def _is_unicode(self) -> bool:
        return is_unicode_charset(self.charset)

    def char2cp(self, char: str) -> int | None:
        """
        Convert a character to a codepoint of the font's charset (`None` if not
        representable).

        https://font.tomchen.org/bdfparser_py/font#char2cp
        """

        if not isinstance(char, str) or len(char) != 1:
            raise TypeError(f'char2cp() expected a single character, got {char!r}')
        try:
            return self._raw.char2cp(char)
        finally:
            self._drain()

    def cp2char(self, cp: int) -> str | None:
        """
        Convert a codepoint of the font's charset to a character (`None` if not
        representable).

        https://font.tomchen.org/bdfparser_py/font#cp2char
        """

        if self._cp2char_override is not None:
            return self._cp2char_override(cp)
        try:
            n = self._raw.cp2unicode(cp)
        finally:
            self._drain()
        return None if n is None else chr(n)

    # -- properties ----------------------------------------------------------

    def getprop(self, key: str) -> Any:
        """
        Get a property value (case-insensitive key), `None` if absent.

        https://font.tomchen.org/bdfparser_py/font#getprop
        """

        return self._raw.getprop(key)

    def setprop(self, key: str, value: int | str | None) -> Font:
        """
        Set a property (case-insensitive key); remembers the spelling of a new key.

        https://font.tomchen.org/bdfparser_py/font#setprop
        """

        self._raw.setprop(key, value)
        self._changed()
        return self

    def delprop(self, key: str) -> Font:
        """
        Remove a property (case-insensitive key) if present.

        https://font.tomchen.org/bdfparser_py/font#delprop
        """

        self._raw.delprop(key)
        self._changed()
        return self

    def swidth(self, dwx: int) -> int:
        """
        Compute SWIDTH (scalable width, 1/1000 em) from DWIDTH (pixels).

        https://font.tomchen.org/bdfparser_py/font#swidth
        """

        return self._raw.swidth(dwx)

    # -- querying ------------------------------------------------------------

    def length(self) -> int:
        """
        Number of encoded glyphs in the font.

        https://font.tomchen.org/bdfparser_py/font#length
        """

        return self._raw.length()

    def __len__(self) -> int:
        """Same as `.length()`."""
        return self.length()

    def itercps(self, order: int = 1, r: Range | None = None) -> Iterator[int]:
        """
        Iterator of the encoded codepoints: `order` 1 ascending (default), 0 file
        order, 2 descending, -1 reverse file order; `r` filters (an int `n` means
        `cp < n`, a tuple an inclusive range, a list of tuples several ranges).

        https://font.tomchen.org/bdfparser_py/font#itercps
        """

        if order not in (-1, 0, 1, 2):
            raise ValueError(f'Unknown order {order!r}')
        return iter(self._raw.itercps(order, _range(r)))

    def iterglyphs(self, order: int = 1, r: Range | None = None) -> Iterator[Glyph]:
        """
        Iterator of the glyphs (attached `Glyph` objects), in the same order and
        range semantics as `itercps`.

        https://font.tomchen.org/bdfparser_py/font#iterglyphs
        """

        for cp in self.itercps(order, r):
            raw = self._raw.lookup(cp)
            if raw is not None:
                yield Glyph._wrap(raw, self)

    def glyphbycp(self, codepoint: int) -> Glyph | None:
        """
        Get a glyph (attached `Glyph`) by codepoint; `None` (with a warning) if absent.

        https://font.tomchen.org/bdfparser_py/font#glyphbycp
        """

        try:
            raw = self._raw.glyphbycp(codepoint)
        finally:
            self._drain()
        return None if raw is None else Glyph._wrap(raw, self)

    def glyph(self, character: str) -> Glyph | None:
        """
        Get a glyph (attached `Glyph`) by character.

        https://font.tomchen.org/bdfparser_py/font#glyph
        """

        if not isinstance(character, str) or len(character) != 1:
            raise TypeError(f'glyph() expected a single character, got {character!r}')
        try:
            raw = self._raw.glyph(character)
        finally:
            self._drain()
        return None if raw is None else Glyph._wrap(raw, self)

    def lacksglyphs(self, string: str) -> list[str] | None:
        """
        Characters of `string` without a glyph (repeats included), or `None`.

        https://font.tomchen.org/bdfparser_py/font#lacksglyphs
        """

        try:
            return self._raw.lacksglyphs(string)
        finally:
            self._drain()

    @staticmethod
    def _draw_kw(
        linelimit: int,
        mode: int,
        direction: str,
        usecurrentglyphspacing: bool,
        missing: Glyph | Mapping[str, Any] | None,
    ) -> dict[str, Any]:
        kw: dict[str, Any] = {
            'linelimit': linelimit,
            'mode': mode,
            'direction': direction,
            'ucgs': bool(usecurrentglyphspacing),
        }
        if missing is not None:
            if isinstance(missing, Glyph):
                kw['missing'] = dict(zip(META_TITLES, missing._reader().meta(), strict=True))
            else:
                kw['missing'] = {k: v for k, v in dict(missing).items() if k in META_TITLES}
        return kw

    def drawcps(
        self,
        cps: Iterable[int | None],
        linelimit: int = 512,
        mode: int = 1,
        direction: str = 'lrtb',
        usecurrentglyphspacing: bool = False,
        missing: Glyph | dict | None = None,
    ) -> Bitmap:
        """
        Draw the glyphs of the given codepoints (`None` = not representable) to a
        `Bitmap`.

        https://font.tomchen.org/bdfparser_py/font#drawcps
        """

        codes = [-1 if cp is None else cp for cp in cps]
        kw = self._draw_kw(linelimit, mode, direction, usecurrentglyphspacing, missing)
        try:
            return self._raw.drawcps(codes, kw)
        finally:
            self._drain()

    def draw(
        self,
        string: str,
        linelimit: int = 512,
        mode: int = 1,
        direction: str = 'lrtb',
        usecurrentglyphspacing: bool = False,
        missing: Glyph | dict | None = None,
    ) -> Bitmap:
        """
        Draw (render) a text to a `Bitmap`.

        https://font.tomchen.org/bdfparser_py/font#draw
        """

        kw = self._draw_kw(linelimit, mode, direction, usecurrentglyphspacing, missing)
        try:
            return self._raw.draw(string, kw)
        finally:
            self._drain()

    def drawall(
        self,
        order: int = 1,
        r: Range | None = None,
        linelimit: int = 512,
        mode: int = 0,
        direction: str = 'lrtb',
        usecurrentglyphspacing: bool = False,
    ) -> Bitmap:
        """
        Draw all glyphs (or those in range `r`) in `order` to a `Bitmap`.

        https://font.tomchen.org/bdfparser_py/font#drawall
        """

        kw = self._draw_kw(linelimit, mode, direction, usecurrentglyphspacing, None)
        kw['order'] = order
        kw['r'] = _range(r)
        try:
            return self._raw.drawall(kw)
        finally:
            self._drain()

    # -- editing ---------------------------------------------------------------

    def addglyph(self, glyph: Glyph | dict[str, Any], replace: bool = True) -> Font:
        """
        Add a glyph (`Glyph` or meta `dict`). An existing codepoint is replaced
        when `replace` (default), else kept with a warning. Codepoint `-1` adds an
        unencoded glyph. A `Glyph` becomes attached to this font.

        https://font.tomchen.org/bdfparser_py/font#addglyph
        """

        meta = glyph.meta if isinstance(glyph, Glyph) else glyph
        for k in meta:
            if k not in META_TITLES:
                raise KeyError(f'Unknown glyph meta key {k!r}')
        if meta.get('codepoint') is None:
            raise ValueError('The glyph has no codepoint')
        try:
            if isinstance(glyph, Glyph):
                glyph._push()
                self._raw.add_handle(glyph._g, replace)
                if glyph._g.is_attached():
                    glyph.font = self
                    glyph._epoch = self._epoch
                glyph._pull()
            else:
                fields = dict(glyph)
                if 'hexdata' in fields and fields['hexdata'] is not None:
                    fields['hexdata'] = list(fields['hexdata'])
                self._raw.addglyph(fields, replace)
        finally:
            self._changed()
        return self

    def _check_slots(self) -> None:
        """bdfparser's writers need 17-slot meta lists (see `glyphs[cp] = ...`)."""
        if self._short & self._cpset():
            from .pcf import PCFError

            raise PCFError('glyph meta must be a 17-slot list')

    def removeglyph(self, cp_or_glyph: int | Glyph) -> Font:
        """
        Remove a glyph (by codepoint or `Glyph`); no-op if missing. An attached
        `Glyph` removes exactly its own slot.

        https://font.tomchen.org/bdfparser_py/font#removeglyph
        """

        if isinstance(cp_or_glyph, Glyph):
            if cp_or_glyph.font is self:
                self._raw.remove_handle(cp_or_glyph._g)
            elif isinstance(cp_or_glyph.cp(), int) and cp_or_glyph.cp() >= 0:
                self._raw.removeglyph(cp_or_glyph.cp())
        else:
            self._raw.removeglyph(cp_or_glyph)
        self._changed()
        return self

    def newglyph(
        self,
        cp: int,
        bitmap: Bitmap | None = None,
        mode: int = 0,
        name: str | None = None,
        dwidth: int | None = None,
    ) -> Glyph:
        """
        Create a glyph at codepoint `cp` (replacing any existing one), optionally
        with a bitmap placed as by `Glyph.setbitmap(bitmap, mode)`; returns it
        attached.

        https://font.tomchen.org/bdfparser_py/font#newglyph
        """

        try:
            raw = self._raw.newglyph(cp, bitmap, mode, name, dwidth)
        finally:
            self._changed()
        return Glyph._wrap(raw, self)

    def updatebbox(self) -> Font:
        """
        Set FONTBOUNDINGBOX to the union of all glyph boxes (empty boxes ignored).

        https://font.tomchen.org/bdfparser_py/font#updatebbox
        """

        self._raw.updatebbox()
        self._changed()
        return self

    def clone(self) -> Font:
        """
        Get a deep copy of the font.

        https://font.tomchen.org/bdfparser_py/font#clone
        """

        return self.__class__._from_raw(self._raw.clone(), self)

    def subset(self, r: Range) -> Font:
        """
        New font (deep copy) with only the encoded glyphs in range `r`.

        https://font.tomchen.org/bdfparser_py/font#subset
        """

        return self.__class__._from_raw(self._raw.subset(_range(r)), self)

    def merge(self, other: Font, replace: bool = False) -> Font:
        """
        Copy the encoded glyphs of `other` (no charset conversion); existing
        codepoints are kept unless `replace`.

        https://font.tomchen.org/bdfparser_py/font#merge
        """

        try:
            self._raw.merge(other._raw, replace)
        finally:
            self._changed()
        return self

    def transform(
        self, fn: Callable[[Bitmap], Bitmap | None], r: Range | None = None, mode: int = 0
    ) -> Font:
        """
        Apply `Glyph.transform(fn, mode)` to every glyph (or those in `r`;
        unencoded glyphs only when `r` is `None`).

        https://font.tomchen.org/bdfparser_py/font#transform
        """

        try:
            self._raw.transform(fn, _range(r), mode)
        finally:
            self._changed()
        return self

    def charsetconvert(self, newcharset: str) -> Font:
        """
        Re-encode every glyph to `newcharset` (unrepresentable glyphs are dropped
        with a warning), update CHARSET_REGISTRY / CHARSET_ENCODING and clear any
        `encoding()` override.

        https://font.tomchen.org/bdfparser_py/font#charsetconvert
        """

        try:
            self._raw.charsetconvert(newcharset)
        finally:
            self._charset_override = None
            self._char2cp_override = None
            self._cp2char_override = None
            self._changed()
        return self

    # -- output ----------------------------------------------------------------

    def tobdf(self) -> str:
        """
        Get the font as BDF text.

        https://font.tomchen.org/bdfparser_py/font#tobdf
        """

        return self._raw.tobdf()

    def topcf(self, **opts: Any) -> bytes:
        """
        Get the font as PCF `bytes` (options: see `bdfparser.pcf.write_pcf`).

        https://font.tomchen.org/bdfparser_py/font#topcf
        """

        from .pcf import write_pcf

        return write_pcf(self, **opts)

    def tottf(self, **opts: Any) -> bytes:
        """
        Get the font as TrueType `bytes` (options: see `bdfparser.ttf.write_ttf`).

        https://font.tomchen.org/bdfparser_py/font#tottf
        """

        from .ttf import write_ttf

        return write_ttf(self, **opts)

    def tosvgfont(
        self,
        xoff: float = 0,
        yoff: float = 0,
        controlcharnoskip: bool = False,
        unicodeformat: str = 'hexncr',
    ) -> str:
        """
        Get the font as an SVG font `str` (`unicodeformat`: `'hexncr'`, `'decncr'`
        or `'char'`).

        https://font.tomchen.org/bdfparser_py/font#tosvgfont
        """

        try:
            return self._raw.tosvgfont(float(xoff), float(yoff), controlcharnoskip, unicodeformat)
        finally:
            self._drain()

    def save(self, path: str | os.PathLike, format: str | None = None, **opts: Any) -> Font:
        """
        Save the font to a file; the format (`'bdf'`, `'pcf'`, `'ttf'`, `'svg'`)
        comes from `format` or the file extension, `opts` go to the writer.

        https://font.tomchen.org/bdfparser_py/font#save
        """

        fmt = (format or os.path.splitext(os.fspath(path))[1].lstrip('.')).lower()
        if fmt == 'pcf':
            from .pcf import _pcf_options

            options = _pcf_options(**opts)
        elif fmt == 'ttf':
            from .ttf import _ttf_options

            options = _ttf_options(**opts)
        elif fmt == 'svg' and opts:
            data = self.tosvgfont(**opts).encode('utf-8')
            with open(path, 'wb') as f:
                f.write(data)
            return self
        elif fmt in ('bdf', 'svg'):
            if opts:
                raise TypeError(f'{fmt} output takes no options, got {sorted(opts)}')
            options = None
        else:
            raise ValueError(f'Unknown output format {fmt!r} (use bdf, pcf, ttf or svg)')
        self._raw.save(os.fspath(path), fmt, options)
        return self
