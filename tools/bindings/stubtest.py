#!/usr/bin/env python3
"""Run mypy.stubtest over the `fontpixel` package with the generated allowlist.

The private native submodules `fontpixel._native._ops` and
`fontpixel._native._bdfparser` are attributes of the extension module, not
importable by their dotted names (`fontpixel._native` is not a package). This
wrapper registers them in `sys.modules` under those names before stubtest
imports anything, so their stubs (`_native/_ops.pyi`, `_native/_bdfparser.pyi`)
are checked like every other module instead of being allowlisted.

Usage (with the extension installed, e.g. after `maturin develop`):
    tools/bindings/stubtest.py [extra stubtest arguments]
"""
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent


def main(argv):
    import fontpixel._native as native

    sys.modules["fontpixel._native._ops"] = native._ops
    sys.modules["fontpixel._native._bdfparser"] = native._bdfparser
    from mypy import stubtest

    args = ["fontpixel", "--allowlist", str(HERE / "stubtest-allowlist.txt"), *argv]
    return stubtest.test_stubs(stubtest.parse_options(args))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
