"""Read a font, inspect its model and convert it to other formats.

    python read_inspect_convert.py [INPUT] [OUTPUT_DIR]

Reads INPUT (format detected from its content, the name is a hint), prints
the strike's names, metrics and a few glyphs, then writes PCF, PSF2 and YAFF
copies to OUTPUT_DIR and reads the PCF back to check the round trip.
"""
from __future__ import annotations

import sys

import fontpixel as fp
from _common import arguments


def main(argv: list[str] | None = None) -> int:
    source, out = arguments(__doc__ or "", argv)
    data = source.read_bytes()

    # Detection only: what would read() pick, and how sure is it?
    detected = fp.detect(data, name=source.name)
    print(f"detected {detected['format']['id']} ({detected['confidence']})")

    # read() returns a Loaded: a FontSet handle, the format used and the report.
    loaded = fp.read(data, name=source.name)
    fonts = loaded.fonts
    assert isinstance(fonts, fp.FontSet)
    font = fonts[0]                      # a view into the set, not a copy
    print(f"{loaded.format}: {len(fonts)} strike(s), {len(font)} glyphs")
    print("family:", font.names.get("family"), "| pixel size:", font.metrics.get("pixel_size"))

    # Glyph lookup by Unicode scalar ("A", 0x41 or Codepoint.unicode(0x41)).
    glyph = font.glyph_for_codepoint("A")
    assert glyph is not None
    print(glyph.bitmap.to_text())
    print("codepoints of the first glyphs:",
          [f"U+{int(cp):04X}" for cp, _gid in font.codepoints()[:5]])

    # Write: each writer returns a Written (OutputBundle + full report).
    for fmt in ("pcf", "psf", "yaff"):
        written = fp.write(fonts, fmt)
        losses = [e for e in written.report["entries"] if e["impact"] != "none"]
        for asset in written.output.assets:
            (out / f"{source.stem}.{asset.path}").write_bytes(asset.data)
        print(f"{fmt}: {written.output.primary} with {len(losses)} reported losses")

    # Round trip: read the PCF back and compare glyph counts and pixels.
    pcf = (out / f"{source.stem}.font.pcf").read_bytes()
    back = fp.read(pcf, format="pcf").fonts
    assert isinstance(back, fp.FontSet)
    again = back[0].glyph_for_codepoint("A")
    assert again is not None and again.bitmap == glyph.bitmap
    assert len(back[0]) == len(font)
    print("PCF round trip: identical glyph count and pixels")
    return 0


if __name__ == "__main__":
    sys.exit(main())
