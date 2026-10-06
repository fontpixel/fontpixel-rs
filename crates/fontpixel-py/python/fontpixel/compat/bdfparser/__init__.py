"""bdfparser-compatible API (bdfparser 3.0 / SPEC-v3) on the fontpixel Rust core.

`Font`, `Glyph`, `Bitmap`, the module functions (`encoding`, `outline`, `pcf`,
`ttf`, `cli`), `BDFParserWarning` and `BDFParseError` behave as in bdfparser.
Font data stays in Rust: `Font`/`Glyph` hold handles and `Bitmap` wraps a
native raster; pixels are converted to row strings only when requested.
"""

__version__ = '3.0.0'

from .bitmap import Bitmap  # noqa: E402
from .errors import BDFParseError, BDFParserWarning  # noqa: E402
from .font import Font  # noqa: E402
from .glyph import Glyph, defaultglyphname  # noqa: E402

__all__ = ['Font', 'Glyph', 'Bitmap', 'BDFParserWarning', 'BDFParseError', 'defaultglyphname']
