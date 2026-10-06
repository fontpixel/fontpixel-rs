"""fontpixel.render through Python, ported from crates/fontpixel/tests/render.rs."""
import struct
import zlib

import pytest

import fontpixel as fp
from conftest import render_font

I64_MIN, I64_MAX = -(2**63), 2**63 - 1


def width(font, text, **options):
    return font.draw(text, options).bitmap.width


def test_default_advance_cell_ink_and_gaps(rfont):
    f = rfont
    assert width(f, "AB") == 7
    assert width(f, "AB", mode="cell") == 8
    assert width(f, "AB", mode="ink") == 6
    assert width(f, "AB", mode="cell", letter_spacing=2) == 10
    r = f.draw("AB", {"mode": "cell", "line_limit": 4, "line_spacing": 2})
    assert (r.bitmap.width, r.bitmap.height) == (4, 8)
    assert str(r.bitmap) == ".#..\n###.\n#.#.\n....\n....\n##..\n###.\n##.."
    assert f.draw("AB", {"mode": "cell", "line_limit": 4, "line_spacing": -1}).bitmap.height == 5
    assert width(f, "AB", use_current_glyph_spacing=True) == 6
    assert r.report == {"entries": []}
    assert r.to_dict()["bitmap"]["width"] == 4
    assert "Rendered(bitmap=4x8" in repr(r)


def test_directions_and_missing_are_explicit(rfont):
    for a in ["lr", "rl", "tb", "bt"]:
        for b in ["lr", "rl", "tb", "bt"]:
            fp.parse_writing_direction(a + b)
    with pytest.raises(fp.InvalidInputError):
        fp.parse_writing_direction("garbage")
    assert fp.writing_direction_from_spec("garbage") == {"glyphs": "right", "lines": "down"}
    assert fp.writing_direction_from_spec("lrbt-suffix") == fp.parse_writing_direction("lrbt")
    assert fp.parse_writing_direction("tbrl") == {"glyphs": "down", "lines": "left"}
    f = rfont
    r = f.draw("A??B")
    assert fp.report_has_loss(r.report)
    assert len(r.report["entries"]) == 1  # repeated missing code is deduplicated
    with pytest.raises(fp.StrictLossError) as e:
        f.draw("A?", {"strict": True})
    assert e.value.kind == "StrictLoss" and e.value.exit_code == 3
    assert e.value.report["entries"][0]["impact"] == "loss"
    with pytest.raises(fp.FontpixelError):
        f.draw("A?", {"missing": "error"})
    replacement = f.glyph_for_codepoint(66)
    assert f.draw("A?", missing_glyph=replacement).bitmap == f.draw("AB").bitmap
    assert f.draw("", missing_glyph=replacement).bitmap == fp.Bitmap()
    with pytest.raises(ValueError):
        f.draw("A", {"missing": {"glyph": {}}})
    with pytest.raises(ValueError):
        f.draw("A", {"not_an_option": 1})
    rl = f.draw("AB", {"direction": {"glyphs": "left", "lines": "down"}})
    assert rl.bitmap.width == 7 and rl.bitmap != f.draw("AB").bitmap


def test_clipping_is_real_ink_loss(rfont):
    f = rfont

    def left(g):
        g.shift(-2, 0)

    f.edit_glyph(f.glyph_id("A"), left)
    r = f.draw("A")
    assert any(e["code"] == "render.cell_clipping" for e in r.report["entries"])
    assert str(r.bitmap) == "....\n#...\n#..."
    m = f.metrics
    m["bounds"] = {"width": 4, "height": 3, "x": I64_MAX, "y": I64_MIN}
    f.metrics = m
    assert not f.draw("A").bitmap.has_ink()


def test_limits_apply_before_rendering(rfont):
    f = rfont
    with pytest.raises(fp.ResourceLimitError):
        f.draw("AAA", {"limits": {"max_glyphs": 2}})
    with pytest.raises(fp.ResourceLimitError):
        f.draw("A", {"limits": {"max_layout_bytes": 1}})
    with pytest.raises(fp.ResourceLimitError):
        f.draw("A", {"limits": {"bitmap": {"max_pixels": 1}}})
    with pytest.raises(fp.FontpixelError):
        f.draw("A", {"letter_spacing": I64_MAX})
    with pytest.raises(fp.FontpixelError):
        f.draw("AA", {"letter_spacing": I64_MIN})
    empty = fp.Font()
    with pytest.raises(fp.FontpixelError):
        empty.draw("AAAA", {"limits": {"max_glyphs": 2}})
    assert empty.draw_all("file", [], {"limits": {"max_glyphs": 2}}).bitmap == fp.Bitmap()
    assert fp.default_render_limits() == {
        "max_glyphs": 1000000,
        "max_layout_bytes": 134217728,
        "max_report_bytes": 16777216,
        "bitmap": fp.default_bitmap_limits(),
    }
    assert fp.default_render_options()["line_limit"] == 512


def test_gray_conversion_is_explicit_and_reported(rfont):
    f = rfont

    def gray(g):
        g.bitmap = fp.Bitmap.from_raw(3, 3, bytes([0, 127, 128, 255, 0, 254, 42, 200, 100]), depth="gray8")

    f.edit_glyph(f.glyph_id("A"), gray)
    with pytest.raises(fp.FontpixelError):
        f.draw("A")
    r = f.draw("A", {"gray_threshold": 128})
    assert fp.report_has_loss(r.report)
    assert str(r.bitmap) == "..#.\n#.#.\n.#.."
    with pytest.raises(fp.StrictLossError):
        f.draw("A", {"gray_threshold": 128, "strict": True})


def test_unicode_to_legacy_uses_exact_tables(rfont):
    f = rfont
    f.glyph(f.glyph_id("A")).codepoints = [fp.Codepoint.legacy(0x80)]
    cs = f.charset
    cs["name"] = "cp1252"
    f.charset = cs
    assert f.draw("€").bitmap == f.draw_codepoints([fp.Codepoint.legacy(0x80)]).bitmap
    assert fp.report_has_loss(f.draw("人").report)
    cs["name"] = "unknown-test"
    f.charset = cs
    assert any(e["code"] == "render.unknown_charset" for e in f.draw("B").report["entries"])


def test_draw_codepoints_and_draw_all(rfont):
    f = rfont
    assert f.draw_codepoints([65, "B"]).bitmap == f.draw("AB").bitmap
    r = f.draw_codepoints([65, None])
    assert len(r.report["entries"]) == 1
    asc = f.draw_all("ascending", options={"mode": "cell"})
    assert asc.bitmap == f.draw("AB", {"mode": "cell"}).bitmap
    desc = f.draw_all("descending", options={"mode": "cell"})
    assert desc.bitmap == f.draw("BA", {"mode": "cell"}).bitmap
    assert f.draw_all("file", options={"mode": "cell"}).bitmap == asc.bitmap
    assert f.draw_all("reverse_file", options={"mode": "cell"}).bitmap == desc.bitmap
    only_b = f.draw_all(ranges=[(66, 66)], options={"mode": "cell"})
    assert only_b.bitmap == f.draw("B", {"mode": "cell"}).bitmap
    with pytest.raises(ValueError):
        f.draw_all("sideways")


def test_diagnostic_deduplication_and_report_budget(rfont):
    empty = fp.Font()
    r = empty.draw_codepoints(range(12000))
    assert len(r.report["entries"]) == 12000
    assert r.bitmap == fp.Bitmap()
    with pytest.raises(fp.ResourceLimitError) as e:
        empty.draw("?", {"limits": {"max_report_bytes": 1}})
    assert e.value.resource == "render diagnostic bytes"
    assert rfont.draw("A", {"limits": {"max_report_bytes": 0}}).bitmap.has_ink()


def test_svg_and_color_validation():
    assert fp.parse_rgba("#f08") == fp.rgba(255, 0, 136)
    assert fp.parse_rgba("#f08c") == fp.rgba(255, 0, 136, 204)
    assert fp.parse_rgba("#aabbccdd") == {"red": 170, "green": 187, "blue": 204, "alpha": 221}
    assert fp.parse_rgba("transparent") == fp.RGBA_TRANSPARENT == fp.rgba(0, 0, 0, 0)
    assert fp.RGBA_BLACK == fp.rgba(0, 0, 0) and fp.RGBA_WHITE == fp.rgba(255, 255, 255)
    for bad in ["red", "#ff", "#zz0000", "#１２３", '"><script/>']:
        with pytest.raises(fp.InvalidInputError):
            fp.parse_rgba(bad)
    b = fp.Bitmap.from_text("#.#\n###")
    svg = fp.to_svg(b, {"scale": 4})
    assert 'width="12" height="8"' in svg
    assert "M0 1h3v1h-3z" in svg
    with pytest.raises(fp.FontpixelError):
        fp.to_svg(b, {"scale": 0})
    with pytest.raises(fp.ResourceLimitError):
        fp.to_svg(b, {"limits": {"max_bytes": 10}})
    assert fp.to_svg(fp.Bitmap()).startswith("<svg")
    with pytest.raises(fp.InvalidInputError):
        fp.to_svg(b.to_gray8())
    assert fp.default_image_options() == {
        "scale": 1,
        "foreground": fp.RGBA_BLACK,
        "background": fp.RGBA_WHITE,
        "limits": fp.default_bitmap_limits(),
    }


def test_png_roundtrips_bits_dimensions_alpha_and_limits():
    b = fp.Bitmap.from_text("#.#\n###")
    o = {"scale": 3, "foreground": fp.rgba(2, 4, 6, 128), "background": fp.RGBA_TRANSPARENT}
    data = fp.to_png(b, o)
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    offset, compressed = 8, b""
    while offset < len(data):
        (n,) = struct.unpack(">I", data[offset : offset + 4])
        kind = data[offset + 4 : offset + 8]
        body = data[offset + 8 : offset + 8 + n]
        (crc,) = struct.unpack(">I", data[offset + 8 + n : offset + 12 + n])
        assert crc == zlib.crc32(data[offset + 4 : offset + 8 + n])
        if kind == b"IHDR":
            assert body[:8] == struct.pack(">II", 9, 6) and body[8] == 1 and body[9] == 3
        elif kind == b"PLTE":
            assert body == bytes([0, 0, 0, 2, 4, 6])
        elif kind == b"tRNS":
            assert body == bytes([0, 128])
        elif kind == b"IDAT":
            compressed += body
        offset += n + 12
    decoded = zlib.decompress(compressed)
    expected = b.enlarge(3, 3).as_bytes()
    rows = [decoded[i : i + 3] for i in range(0, len(decoded), 3)]
    assert [r[1:] for r in rows] == [expected[i : i + 2] for i in range(0, len(expected), 2)]
    assert all(r[0] == 0 for r in rows)
    with pytest.raises(fp.FontpixelError):
        fp.to_png(fp.Bitmap(), o)
    with pytest.raises(fp.ResourceLimitError):
        fp.to_png(b, {**o, "limits": {"max_bytes": 10}})


def test_render_font_fixture_is_independent():
    assert render_font() == render_font()
