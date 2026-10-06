"""Typed tour of the fontpixel API, checked with ``mypy --strict`` and executed.

tests/test_typing.py runs mypy --strict over this file (and the examples) and
calls ``main()``: every annotation below is checked statically and every
call runs against the built extension, so the stubs and the runtime agree on
the shapes used here.
"""
from __future__ import annotations

from pathlib import Path

import fontpixel as fp
import fontpixel.formats.bdf
import fontpixel.sfnt.cmap
import fontpixel.sfnt.name
from fontpixel import charset_table, ops, outline, sfnt, types, variants, woff
from fontpixel.compat import bdfparser
from fontpixel.formats import pcf
from fontpixel.types import (
    Bounds,
    CodepointDict,
    FontMetrics,
    GlyphMetrics,
    Report,
    ResourceLimits,
)

REPO = Path(__file__).resolve().parents[3]
SPLEEN = REPO / "tests" / "data" / "bdf" / "spleen-5x8.bdf"


def model() -> fp.Font:
    loaded: fp.Loaded = fp.read(SPLEEN.read_bytes(), name=SPLEEN.name)
    assert loaded.format == "bdf"
    single = loaded.into_single()
    font = single.fonts
    assert isinstance(font, fp.Font)
    metrics: FontMetrics = font.metrics
    assert metrics.get("pixel_size") == 8
    codepoint: fp.Codepoint = fp.Codepoint.unicode(0x41)
    plain: CodepointDict = codepoint.to_dict()
    assert plain == {"Unicode": 0x41} and codepoint.kind == "unicode" and codepoint.scalar() == "A"
    gid = font.glyph_id(codepoint)
    assert gid is not None
    glyph = font[gid]
    gm: GlyphMetrics = glyph.metrics
    bounds: Bounds = gm["bounds"]
    assert (bounds["width"], bounds["height"]) == (glyph.bitmap.width, glyph.bitmap.height)
    ids: list[int] = font.glyph_ids()
    assert gid in ids and len(font) == len(ids)
    pairs = font.codepoints()
    assert all(isinstance(cp, fp.Codepoint) and isinstance(i, int) for cp, i in pairs)
    usage = font.retained_usage()
    assert usage["glyphs"] >= len(font)
    return font


def bitmaps() -> None:
    b = fp.Bitmap(3, 2)
    b[1, 0] = 1
    assert b.depth == "mono" and b.to_rows() == [[0, 1, 0], [0, 0, 0]]
    gray = b.to_gray8()
    assert gray.depth == "gray8" and gray.max_value == 255
    rows: list[list[int]] = gray.to_mono(128).to_rows()
    assert rows[0] == [0, 1, 0]
    raw = fp.Bitmap.from_raw(3, 2, b.to_bytes(), "mono")
    assert raw == b
    wide = fp.Bitmap.concat_all([b, b], "right", "end", [1])
    assert wide.width == 7
    as_dict = b.to_dict()
    assert fp.Bitmap.from_dict(as_dict) == b
    bbox: Bounds = b.bbox()
    assert bbox["width"] == 1
    layout = fp.default_byte_layout()
    assert fp.Bitmap.from_bytes(3, 2, b.to_bytes(layout), layout) == b


def rendering(font: fp.Font) -> None:
    rendered = font.draw("Hi", {"mode": "advance"})
    report: Report = rendered.report
    assert report["entries"] == []
    png: bytes = fp.to_png(rendered.bitmap, {"scale": 2})
    svg: str = fp.to_svg(rendered.bitmap)
    assert png.startswith(b"\x89PNG") and svg.startswith("<svg")
    codes = font.draw_codepoints([0x48, None, fp.Codepoint.unicode(0x69)])
    assert codes.bitmap.width >= rendered.bitmap.width


def formats(font: fp.Font) -> None:
    fonts = fp.FontSet([font])
    written = fp.write(fonts, "pcf", {"glyph_pad": "4"})
    assert isinstance(written, fp.Written) and written.output.primary == "font.pcf"
    data, rep = pcf.write(fonts, {"byte_order": "lsb"})
    assert data[:4] == b"\x01fcp" and isinstance(rep["entries"], list)
    model, _ = fontpixel.formats.bdf.read(SPLEEN.read_bytes())
    assert isinstance(model, (fp.Font, fp.FontSet))
    status = fp.registry().status("bdf")
    assert status["read"] and status["write"]
    assert fp.formats.pcf.detect(data) == "certain"
    defaults = fp.formats.normalize("formats::pcf::PcfWriteOptions")
    assert defaults["glyph_pad"] == "4" and defaults["byte_order"] == "msb"
    limits: ResourceLimits = fp.default_resource_limits()
    assert limits.get("max_glyphs", 0) > 0
    entries = charset_table()
    assert any(e["key"] == "iso88591" and e["available"] for e in entries)
    assert types.SERDE_TYPES["formats::pcf::PcfWriteOptions"] is types.PcfWriteOptions


def opentype(font: fp.Font) -> None:
    fonts = fp.FontSet([font])
    ttf, _ = fp.formats.vector_raster.write(fonts, "ttf")
    parsed = sfnt.Sfnt(ttf)
    cmap = parsed.table("cmap")
    assert cmap is not None
    mapping: dict[int, int] = sfnt.cmap.decode(cmap, 600)
    assert 0x41 in mapping
    encoded = sfnt.cmap.encode(mapping, 600)
    assert isinstance(encoded["data"], bytes)
    name = parsed.table("name")
    assert name is not None
    table = sfnt.name.decode(name)
    assert any(r["name_id"] == 1 for r in table["records"])
    packed = woff.woff2.encode(ttf)
    assert woff.woff2.decode(packed["bytes"])["auxiliary"]["metadata"] is None
    v = variants.Variants(font)
    assert v.languages == [] and isinstance(v.encode()["bytes"], bytes)
    contours = outline.contours(font.glyph_in_cell(font.glyph_ids()[33]))
    assert contours and all(len(point) == 3 for c in contours for point in c)


def operations() -> None:
    files = {"s.bdf": SPLEEN.read_bytes()}
    result: ops.OperationResult = ops.convert(inputs=["s.bdf"], output="s.pcf", files=files)
    assert result.exit_code == 0 and set(result.outputs) == {"s.pcf"}
    info = ops.info({"input": "s.bdf"}, files=files)
    assert info.document["format"] == "bdf"
    parsed = ops.parse_args(["convert", "a.bdf", "-o", "b.pcf"])
    assert parsed["command"] == "convert"
    vec = ops.vectorize(inputs=["s.bdf"], output="s.ttf", files=files)
    assert set(vec.outputs) == {"s.ttf"}


def errors() -> None:
    try:
        fp.read(b"not a font")
    except fp.UnrecognizedFormatError as error:
        assert error.kind == "UnrecognizedFormat" and error.exit_code == 1
    try:
        fp.read(SPLEEN.read_bytes(), limits={"max_glyphs": 1})
    except fp.ResourceLimitError as error:
        assert error.limit == 1 and isinstance(error.resource, str)


def compat() -> None:
    font = bdfparser.Font(str(SPLEEN))
    glyph = font.glyph("A")
    assert glyph is not None
    bitmap: bdfparser.Bitmap = glyph.draw()
    assert bitmap.width() == 5 and "#" in str(bitmap)


def main() -> None:
    font = model()
    bitmaps()
    rendering(font)
    formats(font)
    opentype(font)
    operations()
    errors()
    compat()


if __name__ == "__main__":
    main()
