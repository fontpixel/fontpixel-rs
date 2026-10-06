"""fontpixel: bitmap font toolkit (Python bindings).

The complete native API lives in this package; ``fontpixel.compat.bdfparser``
provides a bdfparser-compatible layer on the same Rust core.
"""
from ._native import *  # noqa: F401,F403
from ._native import __version__  # noqa: F401
