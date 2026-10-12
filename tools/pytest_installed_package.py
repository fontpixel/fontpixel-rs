"""pytest plugin: abort unless `fontpixel` is imported from site-packages.

Used by the installed-package tests (CI, scripts/build_wheels.sh): the suite
runs from the repository's crates/fontpixel-py/tests (its data paths are
relative to it) but must test the installed wheel or sdist, never the source
tree. Load it with `PYTHONPATH=tools python -m pytest -p pytest_installed_package`.
"""

import sys
import sysconfig
from pathlib import Path


def pytest_sessionstart(session):
    import fontpixel
    import fontpixel._native as native

    roots = {Path(sysconfig.get_paths()[key]).resolve() for key in ("purelib", "platlib")}
    for mod in (fontpixel, native):
        path = Path(mod.__file__).resolve()
        if not any(root in path.parents for root in roots):
            raise SystemExit(f"{mod.__name__} imported from {path}, not from {sorted(map(str, roots))}")
    print(f"\ninstalled package: {Path(native.__file__).resolve()} (Python {sys.version.split()[0]})")
