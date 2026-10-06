"""Test shim: `bdfparser` is `fontpixel.compat.bdfparser`.

Put the directory containing this package first on `sys.path` (and on
`PYTHONPATH` for `python -m bdfparser` subprocesses) to run bdfparser's own
tests against the fontpixel compatibility API. The submodules are the
compatibility modules themselves (registered in `sys.modules`), except
`bdfparser.ttf`, which adds the private helpers the reference tests import
(see `ttf.py`).
"""

import importlib
import sys

import fontpixel.compat.bdfparser as _compat
from fontpixel.compat.bdfparser import *  # noqa: F401,F403
from fontpixel.compat.bdfparser import __all__, __version__  # noqa: F401

for _name in ('bitmap', 'cli', 'encoding', 'errors', 'font', 'glyph', 'outline', 'pcf'):
    _module = importlib.import_module(f'{_compat.__name__}.{_name}')
    sys.modules[f'{__name__}.{_name}'] = _module
    globals()[_name] = _module

from . import ttf  # noqa: E402,F401
