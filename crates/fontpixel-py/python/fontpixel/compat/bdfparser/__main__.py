"""`python -m fontpixel.compat.bdfparser` runs the bdfparser-compatible CLI."""

import sys

from .cli import main

sys.exit(main())
