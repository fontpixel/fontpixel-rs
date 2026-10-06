"""Shared fixtures for the fontpixel native API tests."""
from pathlib import Path

import pytest

import fontpixel as fp

REPO = Path(__file__).resolve().parents[3]
CORE_TEST_DATA = REPO / "crates" / "fontpixel" / "tests" / "data"


def render_font():
    """The two-glyph font of crates/fontpixel/tests/render.rs::font()."""
    font = fp.Font()
    metrics = font.metrics
    metrics["bounds"] = {"width": 4, "height": 3, "x": -1, "y": -1}
    font.metrics = metrics
    for cp, rows, advance in [(65, ".#.\n###\n#.#", 3), (66, "##.\n###\n##.", 2)]:
        glyph = fp.Glyph(fp.Bitmap.from_text(rows), codepoints=[cp])
        m = glyph.metrics
        m["bounds"]["x"] = -1
        m["bounds"]["y"] = -1
        m["advance"] = {"x": advance, "y": 0}
        m["vertical_advance"] = {"x": 0, "y": 3}
        glyph.metrics = m
        font.add_glyph(glyph)
    return font


@pytest.fixture
def rfont():
    return render_font()
