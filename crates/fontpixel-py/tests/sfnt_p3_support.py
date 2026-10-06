"""Shared data for the OpenType/WOFF/variants/outline/merge binding tests.

``data/p3/expected.json`` holds the results of the same Rust calls the tests
make (a throwaway Rust program run on this branch with the extension's feature
set: all-formats, serde, compat-bdfparser, cjk-charsets) and, under ``cli.*``,
outputs of the frozen phase-6 CLI (``frozen-binaries-v5``) for variants, subset
and merge. Fonts (``font.*``) are the Rust BDF reader's serde dicts of
``tests/data/compat/mini.bdf`` and ``data/p3/{alt,latin}.bdf``, i.e. exactly
the models the CLI loads.
"""
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[3]
DATA = ROOT / "crates" / "fontpixel-py" / "tests" / "data" / "p3"
E = json.loads((DATA / "expected.json").read_text())
GOLDEN = (ROOT / "tests" / "data" / "compat" / "mini-square-1.0-0.ttf").read_bytes()
CIRCLE = (ROOT / "tests" / "data" / "compat" / "mini-circle-1.0-0.ttf").read_bytes()


def h(key):
    """Expected bytes stored as hex."""
    return bytes.fromhex(E[key])


def norm(value):
    """JSON-normalize a binding result (tuples -> lists, int keys -> str,
    bytes -> lists of ints) so it compares with the serde_json expectation."""
    return json.loads(json.dumps(value, default=lambda b: list(b)))


def font(name):
    """A serde font dict (`mini`, `alt` or `latin`)."""
    return E["font." + name]
