"""`Glyph` of the bdfparser-compatible API (handle over the Rust compat glyph).

A glyph obtained from a font (`glyphbycp`, `glyph`, `iterglyphs`, `newglyph`,
or passed to `addglyph`) is *attached*: every editing method writes the updated
meta back into the font's native model immediately. A glyph constructed
directly, or a `clone()`, is detached.
"""

from __future__ import annotations

from collections.abc import Callable, Mapping
from typing import TYPE_CHECKING, Any

from ..._native import _bdfparser as _n
from .bitmap import Bitmap

if TYPE_CHECKING:
    from .font import Font

__all__ = ['Glyph', 'defaultglyphname', 'META_TITLES']

#: The 17 glyph meta keys, in the order of `Font.glyphs` lists.
META_TITLES: tuple[str, ...] = tuple(_n.META_TITLES)

defaultglyphname = _n.defaultglyphname


def _font_of(font: Font | None) -> Any:
    if font is None:
        return _n.RawFont()
    return font._raw


class Glyph:
    """
    `Glyph` object: the 17 meta values (`meta`, a `dict`) plus the font it
    belongs to (`font`).

    https://font.tomchen.org/bdfparser_py/glyph
    """

    def __init__(self, meta_dict: Mapping[str, Any], font: Font) -> None:
        """
        Initialize a detached `Glyph` from a `dict` of meta information and the
        font it belongs to.

        https://font.tomchen.org/bdfparser_py/glyph#glyph
        """

        self.meta: dict[str, Any] = dict(meta_dict)
        self.font = font
        known = {k: v for k, v in self.meta.items() if k in META_TITLES}
        self._g = _n.RawGlyph(known, _font_of(font))

    @classmethod
    def _wrap(cls, raw: Any, font: Font | None, extra: Mapping[str, Any] | None = None) -> Glyph:
        g = cls.__new__(cls)
        g.font = font
        g._g = raw
        g.meta = dict(extra) if extra else {}
        g._pull()
        return g

    # -- synchronisation with the Rust handle ------------------------------

    def _pull(self) -> None:
        for k, v in zip(META_TITLES, self._g.meta(), strict=True):
            self.meta[k] = v

    def _push(self) -> None:
        """Write direct edits of `self.meta` into the handle (and its font)."""
        current = self._g.meta()
        changed = {}
        for k, v in zip(META_TITLES, current, strict=True):
            mine = self.meta.get(k)
            if mine != v or type(mine) is not type(v):
                changed[k] = mine
        if changed:
            self._g.setmeta(changed)

    def _edit(self, op: Callable[[], Any]) -> Glyph:
        font = self.font
        try:
            self._push()
            op()
        finally:
            self._pull()
            if font is not None:
                font._changed()
        return self

    @property
    def _attached(self) -> bool:
        """Whether this handle still writes through to its font's slot."""
        return self._g.is_attached()

    @_attached.setter
    def _attached(self, value: bool) -> None:
        # attachment is tracked by the Rust handle: it cannot be forced on, and
        # a removed or replaced slot detaches it automatically
        return None

    # -- bdfparser API -----------------------------------------------------

    def __str__(self) -> str:
        """
        Human-readable picture of the glyph drawn in the font's bounding box.

        https://font.tomchen.org/bdfparser_py/glyph#str-and-print
        """

        return str(self.draw())

    def __repr__(self) -> str:
        """
        Programmer-readable representation.

        https://font.tomchen.org/bdfparser_py/glyph#repr
        """

        return 'Glyph(' + str(self.meta) + ', ' + str(self.font) + ')'

    def cp(self) -> int:
        """
        Get the codepoint of the glyph.

        https://font.tomchen.org/bdfparser_py/glyph#cp
        """

        return self.meta['codepoint']

    def chr(self) -> str | None:
        """
        Get the character of the glyph (converted with the font's charset);
        `None` if the codepoint is not representable.

        https://font.tomchen.org/bdfparser_py/glyph#chr
        """

        if self.font is not None:
            return self.font.cp2char(self.cp())
        return self._g.chr()

    def draw(self, mode: int = 0, bb: tuple[int, int, int, int] | None = None) -> Bitmap:
        """
        Draw the glyph to a `Bitmap`: `mode=0` in the font's bounding box, `1` in
        the glyph's BBX, `2` the original hex data with padding bits, `-1` in the
        box `bb` (`(w, h, xoff, yoff)`).

        https://font.tomchen.org/bdfparser_py/glyph#draw
        """

        self._push()
        try:
            return self._g.draw(mode, None if bb is None else tuple(bb))
        finally:
            if self.font is not None:
                self.font._drain()

    def origin(
        self,
        mode: int = 0,
        fromorigin: bool = False,
        xoff: int | None = None,
        yoff: int | None = None,
    ) -> tuple[int, int]:
        """
        Displacement of the origin from the bottom-left corner of the bitmap drawn
        by `draw(mode)`, or vice versa with `fromorigin=True`.

        https://font.tomchen.org/bdfparser_py/glyph#origin
        """

        if mode == -1 and (xoff is None or yoff is None):
            raise ValueError(
                'Parameter xoff and yoff in origin() method must be all set when mode=-1'
            )
        self._push()
        return self._g.origin(mode, fromorigin, xoff, yoff)

    def setbitmap(
        self,
        bitmap: Bitmap,
        mode: int = 0,
        bb: tuple[int, int, int, int] | None = None,
        trim: bool = True,
    ) -> Glyph:
        """
        Set the glyph's bitmap (inverse of `draw()`); any non-`'0'` pixel is ink.
        DWIDTH / SWIDTH are not changed.

        https://font.tomchen.org/bdfparser_py/glyph#setbitmap
        """

        if not isinstance(bitmap, Bitmap):
            bitmap = Bitmap(list(bitmap))
        box = None if bb is None else tuple(bb)
        return self._edit(lambda: self._g.setbitmap(bitmap, mode, box, trim))

    def setmeta(self, obj: Mapping[str, Any] | None = None, /, **kwargs: Any) -> Glyph:
        """
        Set any of the 17 meta keys, e.g. `glyph.setmeta(dwx0=8, glyphname='A')`.
        Unknown keys raise `KeyError`. Changing the codepoint of an attached glyph
        moves it in the font (overwriting a glyph already at the destination).

        https://font.tomchen.org/bdfparser_py/glyph#setmeta
        """

        items = dict(obj or {})
        items.update(kwargs)
        for k in items:
            if k not in META_TITLES:
                raise KeyError(f'Unknown glyph meta key {k!r}')
        items = {k: (list(v) if k == 'hexdata' and v is not None else v) for k, v in items.items()}
        return self._edit(lambda: self._g.setmeta(items))

    def shift(self, dx: int = 0, dy: int = 0) -> Glyph:
        """
        Move the glyph's bitmap relative to the origin (`bbxoff += dx`,
        `bbyoff += dy`, y up).

        https://font.tomchen.org/bdfparser_py/glyph#shift
        """

        return self._edit(lambda: self._g.shift(dx, dy))

    def transform(self, fn: Callable[[Bitmap], Bitmap | None], mode: int = 0) -> Glyph:
        """
        Draw the glyph (`draw(mode)`), pass the bitmap to `fn` and set the result
        (or the mutated argument if `fn` returns `None`) with `setbitmap`.

        https://font.tomchen.org/bdfparser_py/glyph#transform
        """

        return self._edit(lambda: self._g.transform(fn, mode))

    def clone(self) -> Glyph:
        """
        Get a deep copy of the glyph (same font, detached).

        https://font.tomchen.org/bdfparser_py/glyph#clone
        """

        self._push()
        g = Glyph._wrap(self._g.clone(), self.font, self.meta)
        return g
