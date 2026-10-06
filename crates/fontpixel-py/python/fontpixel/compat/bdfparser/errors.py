"""Warning and exception classes of the bdfparser-compatible API.

bdfparser raises built-in exceptions (`ValueError`, `KeyError`, `OSError`, ...)
plus `BDFParseError` / `PCFError`. Errors coming from the Rust core are
translated by `_translate`: the raised class derives from both the bdfparser
class and the matching `fontpixel` exception (so `except ValueError` and
`except fontpixel.InvalidInputError` both work) and keeps the structured
attributes (`kind`, `resource`, `limit`, `line`, `report`, ...).
"""

from __future__ import annotations

import fontpixel as _fp

__all__ = ['BDFParserWarning', 'BDFParseError']


class BDFParserWarning(UserWarning):
    """
    Category of every warning emitted by the compatibility API (non-fatal
    problems in a font, missing glyphs, unknown charsets, ...).

    Filter them with the standard `warnings` machinery, e.g.
    `warnings.simplefilter('ignore', BDFParserWarning)`.
    """


class BDFParseError(ValueError):
    """
    Raised on fatal BDF parse errors (unexpected end of file inside the headers,
    malformed numbers in required fields, glyph without `BBX`, ...).

    `lineno` is the 1-based line number where the problem was detected (or `None`).
    """

    def __init__(self, message: str, lineno: int | None = None) -> None:
        self.lineno = lineno
        self.msg = message
        super().__init__(f'line {lineno}: {message}' if lineno is not None else message)


_CLASSES: dict[tuple[type, type], type] = {}


def _class(native: type, builtin: type) -> type:
    key = (native, builtin)
    cls = _CLASSES.get(key)
    if cls is None:
        name = native.__name__.replace('Error', '') + builtin.__name__
        cls = type(name, (builtin, native), {'__module__': __name__})
        _CLASSES[key] = cls
    return cls


def _copy_attrs(src: BaseException, dst: BaseException) -> BaseException:
    for k, v in vars(src).items():
        if k not in ('lineno', 'msg'):
            try:
                setattr(dst, k, v)
            except AttributeError:
                pass
    dst.__cause__ = src
    dst.__suppress_context__ = True
    return dst


def _translate(exc: BaseException, ctx: str = '') -> BaseException:
    """Map a native `fontpixel` exception to the bdfparser-compatible one."""
    if not isinstance(exc, _fp.FontpixelError):
        return exc
    kind = getattr(exc, 'kind', '')
    msg = str(exc)
    if kind == 'BdfParse':
        line = getattr(exc, 'line', None)
        prefix = f'line {line}: '
        text = msg[len(prefix) :] if msg.startswith(prefix) else msg
        if ctx == 'pcf':
            from .pcf import PCFError

            return _copy_attrs(exc, PCFError(text, line))
        return _copy_attrs(exc, _class(type(exc), BDFParseError)(text, line))
    if ctx == 'pcf' and kind in ('InvalidInput', 'ResourceLimit', 'UnrecognizedFormat'):
        from .pcf import PCFError

        return _copy_attrs(exc, PCFError(msg))
    if kind == 'Io':
        return _copy_attrs(exc, _class(type(exc), OSError)(msg))
    if msg.startswith('Unknown glyph meta key'):
        return _copy_attrs(exc, _class(type(exc), KeyError)(msg))
    return _copy_attrs(exc, _class(type(exc), ValueError)(msg))
