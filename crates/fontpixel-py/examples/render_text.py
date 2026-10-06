"""Render text with a bitmap font and export it as PNG and SVG.

    python render_text.py [INPUT] [OUTPUT_DIR]

Writes `hello.png` (4x scale, white on dark blue), `hello.svg`, `vertical.png`
(top-to-bottom columns) and `all.png` (every glyph, 16 per line).
"""
from __future__ import annotations

import sys

import fontpixel as fp
from _common import arguments
from fontpixel.types import ImageOptions, RenderOptions


def main(argv: list[str] | None = None) -> int:
    source, out = arguments(__doc__ or "", argv)
    fonts = fp.read(source.read_bytes(), name=source.name).fonts
    assert isinstance(fonts, fp.FontSet)
    font = fonts[0]

    # draw() lays out Unicode text through the font's charset map. Options
    # are a RenderOptions dict; omitted keys take the Rust defaults.
    options: RenderOptions = {"letter_spacing": 1, "line_limit": 200}
    rendered = font.draw("Hello, fontpixel!", options)
    bitmap = rendered.bitmap                       # a Mono Bitmap
    print(f"{bitmap.width}x{bitmap.height}, report entries: {len(rendered.report['entries'])}")

    image: ImageOptions = {
        "scale": 4,
        "foreground": fp.RGBA_WHITE,
        "background": fp.parse_rgba("#1b2a49"),
    }
    (out / "hello.png").write_bytes(fp.to_png(bitmap, image))
    (out / "hello.svg").write_text(fp.to_svg(bitmap, {"foreground": fp.rgba(200, 40, 40)}))

    # Vertical layout: glyphs go down, lines (columns) go left.
    vertical = font.draw("ABC", {"direction": {"glyphs": "down", "lines": "left"}})
    (out / "vertical.png").write_bytes(fp.to_png(vertical.bitmap, {"scale": 2}))

    # Every glyph in codepoint order, 16 per line.
    bounds = font.metrics.get("bounds")           # FontMetrics: a TypedDict copy
    cell_width = bounds["width"] if bounds else 8
    everything = font.draw_all("ascending", options={"line_limit": 16 * cell_width})
    (out / "all.png").write_bytes(fp.to_png(everything.bitmap))

    # Missing characters render as empty cells by default; "error" raises
    # in strict mode instead (see errors_limits.py).
    missing = font.draw("中", {"missing": "blank"})
    print("missing glyph report:", [e["code"] for e in missing.report["entries"]])

    # Pixels can also be read directly: rows of 0/1 values, top row first.
    first_row = rendered.bitmap.to_rows()[0]
    print("first row has", sum(first_row), "ink pixels")
    return 0


if __name__ == "__main__":
    sys.exit(main())
