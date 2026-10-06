"""Edit glyphs and bitmaps in place, then save the font.

    python edit_glyphs.py [INPUT] [OUTPUT_DIR]

Shows the handle model: `Font`/`Glyph` objects taken from a `FontSet` are
views (edits change the set), `Bitmap` values are always copies (assign them
back), and plain data such as metrics are dicts (read, edit, assign back).
Writes `edited.bdf` to OUTPUT_DIR.
"""
from __future__ import annotations

import sys

import fontpixel as fp
from _common import arguments


def main(argv: list[str] | None = None) -> int:
    source, out = arguments(__doc__ or "", argv)
    fonts = fp.read(source.read_bytes(), name=source.name).fonts
    assert isinstance(fonts, fp.FontSet)
    font = fonts[0]

    # 1. Pixel edits on a copy, assigned back through the glyph view.
    glyph = font.glyph_for_codepoint("A")
    assert glyph is not None and glyph.is_view
    bitmap = glyph.bitmap                     # a copy of the pixels
    bitmap.set_pixel(0, 0, 1)                 # (x, y) from the top-left pixel
    bitmap[4, 0] = 1                          # item syntax: bitmap[x, y]
    glyph.bitmap = bitmap                     # write back (bounds follow the size)
    print(glyph.bitmap.to_text())

    # 2. Geometric operations return new bitmaps.
    bold = glyph.bitmap.bold(1)
    mirrored = glyph.bitmap.flip_horizontal()
    print("bold width:", bold.width, "| mirrored ink:", mirrored.has_ink())

    # 3. Build a new glyph from text rows and add it at U+E000.
    star = fp.Bitmap.from_text("""
        ..#..
        .###.
        #####
        .###.
        .#.#.
    """.replace(" ", ""))
    new = fp.Glyph(star, codepoints=[0xE000], name="star")
    metrics = new.metrics                     # a dict copy (GlyphMetrics)
    metrics["advance"] = {"x": 6, "y": 0}
    new.metrics = metrics
    gid = font.add_glyph(new)                 # the font copies the glyph
    print(f"added glyph {gid}:", font.glyph(gid))

    # 4. Font-level plain data: edit the dict and assign it back.
    names = font.names
    names["family"] = "Spleen Edited"
    font.names = names
    font.properties.set("COMMENT_EXAMPLE", "edited by fontpixel")

    # 5. Remove a glyph: its ID stays reserved (other IDs do not move).
    removed = font.remove_glyph(font.glyph_ids()[-2])
    print("removed:", removed.name if removed else None, "| slots:", font.slot_count)

    font.update_bounds()
    font.validate()
    written = fp.write(fonts, "bdf")
    (out / "edited.bdf").write_bytes(written.output.into_single())

    again = fp.read((out / "edited.bdf").read_bytes(), format="bdf").fonts
    assert isinstance(again, fp.FontSet)
    assert again[0].glyph_for_codepoint(0xE000) is not None
    assert again[0].names.get("family") == "Spleen Edited"
    print("saved", out / "edited.bdf")
    return 0


if __name__ == "__main__":
    sys.exit(main())
