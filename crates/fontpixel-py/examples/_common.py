"""Shared command-line handling for the fontpixel examples.

Every example is a script with ``main(argv) -> int``: ``argv`` is
``[INPUT, OUTPUT_DIR]``; both default to a fixture of this repository and a
``fontpixel-examples`` directory under the current directory.
"""
from __future__ import annotations

import argparse
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
SPLEEN = REPO / "tests" / "data" / "bdf" / "spleen-5x8.bdf"
ALT = REPO / "crates" / "fontpixel-py" / "tests" / "data" / "p3" / "alt.bdf"
MINI = REPO / "tests" / "data" / "compat" / "mini.bdf"


def arguments(description: str, argv: list[str] | None, default_input: Path = SPLEEN) -> tuple[Path, Path]:
    """Parse ``[INPUT] [OUTPUT_DIR]``; create the output directory."""
    parser = argparse.ArgumentParser(description=description)
    parser.add_argument("input", nargs="?", type=Path, default=default_input, help="input font")
    parser.add_argument("out", nargs="?", type=Path, default=Path("fontpixel-examples"),
                        help="directory for the files the example writes")
    args = parser.parse_args(argv)
    out: Path = args.out
    out.mkdir(parents=True, exist_ok=True)
    return args.input, out
