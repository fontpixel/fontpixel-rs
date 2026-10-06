"""Shim: `python -m bdfparser` runs the fontpixel compatibility CLI."""

import sys

from fontpixel.compat.bdfparser.cli import main

sys.exit(main())
