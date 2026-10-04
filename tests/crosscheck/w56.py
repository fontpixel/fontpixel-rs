#!/usr/bin/env python3
"""W5.6 independent fontTools/Pillow/system-FreeType acceptance; MIT original tests."""
import argparse
import ctypes
import ctypes.util
import hashlib
import importlib.metadata
import io
import json
import logging
import subprocess
from pathlib import Path
from fontTools.ttLib import TTFont
from PIL import Image

# The PyPI FreeType wheel may omit PNG. Use the installed system library, without
# modifying site-packages or copying third-party implementation code.
_system = ctypes.util.find_library("freetype")
if not _system:
    raise RuntimeError("system FreeType is required for PNG interoperability")
_cdll = ctypes.CDLL
ctypes.CDLL = lambda path, *a, **kw: _cdll(
    _system if str(path).endswith("libfreetype.so") else path, *a, **kw)
try:
    import freetype
finally:
    ctypes.CDLL = _cdll
logging.getLogger("fontTools.ttLib.tables._h_e_a_d").setLevel(logging.ERROR)


def check_png(data, expected):
    image = Image.open(io.BytesIO(data)).convert("RGBA")
    assert image.size == (expected["width"], expected["height"])
    values = list(image.get_flattened_data())
    assert all(p[:3] == (0, 0, 0) for p in values)
    assert [p[3] for p in values] == expected["pixels"]


def rendered(face, gid):
    face.load_glyph(gid, freetype.FT_LOAD_RENDER | freetype.FT_LOAD_COLOR)
    slot, out = face.glyph, []
    bitmap = slot.bitmap
    data = bytes(bitmap.buffer)
    for y in range(bitmap.rows):
        for x in range(bitmap.width):
            start = y * abs(bitmap.pitch)
            if bitmap.pixel_mode == freetype.FT_PIXEL_MODE_BGRA:
                p = data[start + 4 * x:start + 4 * x + 4]
                assert p[:3] == b"\0\0\0"
                out.append(p[3])
            elif bitmap.pixel_mode == freetype.FT_PIXEL_MODE_GRAY:
                out.append(data[start + x])
            else:
                raise AssertionError(("unexpected pixel format", bitmap.pixel_mode))
    return dict(width=bitmap.width, height=bitmap.rows, x=slot.bitmap_left,
                y=slot.bitmap_top - bitmap.rows, advance=slot.advance.x // 64,
                pixels=out)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, default=Path("reports/w5.6-color"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    directory = args.output / "fixtures"
    subprocess.run([str(args.binary), "fixtures", str(directory)], check=True)
    results = []
    total_pixels = total_glyphs = 0
    for path in sorted(directory.glob("*.ttf")):
        if "fonttools" in path.name:
            continue
        expected = json.loads(path.with_name(path.stem + "-expected.json").read_text())
        font = TTFont(path, recalcBBoxes=False, recalcTimestamp=False)
        font.ensureDecompiled()
        order = font.getGlyphOrder()
        face = freetype.Face(str(path))
        cmap = font.getBestCmap()
        wanted_cmap = {cp: g["name"] for s in expected["source"] for g in s["glyphs"] for cp in g["codes"]}
        assert cmap == wanted_cmap, (path, cmap, wanted_cmap)
        assert face.num_fixed_sizes == len(expected["source"])
        assert [(cp, gid) for cp, gid in face.get_chars() if gid] == [(cp, order.index(name)) for cp, name in sorted(cmap.items())]
        native = {s["ppem"]: {g["name"]: g for g in s["glyphs"]} for s in expected["read"]}
        glyph_checks = pixel_checks = 0
        for source in expected["source"]:
            ppem = source["ppem"]
            indices = [i for i, s in enumerate(face.available_sizes) if s.y_ppem == ppem * 64]
            assert len(indices) == 1
            face.select_size(indices[0])
            if "CBDT" in font:
                si = next(i for i, s in enumerate(font["CBLC"].strikes) if s.bitmapSizeTable.ppemY == ppem)
                strike = font["CBLC"].strikes[si]
                assert strike.bitmapSizeTable.bitDepth == 32
                assert strike.bitmapSizeTable.hori.ascender == source["ascent"]
                assert strike.bitmapSizeTable.hori.descender == -source["descent"]
                assert all(s.indexFormat == 1 for s in strike.indexSubTables)
                images = font["CBDT"].strikeData[si]
            else:
                images = font["sbix"].strikes[ppem].glyphs
                assert font["sbix"].flags == 1
            for g in source["glyphs"]:
                name, area = g["name"], g["width"] * g["height"]
                gid = order.index(name)
                image = images[name]
                if "sbix" in font:
                    if not area:
                        assert image.graphicType is None
                        assert name not in native[ppem]
                        continue
                    assert (image.originOffsetX, image.originOffsetY) == (g["x"], g["y"])
                    assert image.graphicType == "png "
                else:
                    m = image.metrics
                    assert (m.width, m.height) == (g["width"], g["height"])
                    bx, by, adv = ((m.horiBearingX, m.horiBearingY, m.horiAdvance)
                                  if hasattr(m, "horiAdvance") else (m.BearingX, m.BearingY, m.Advance))
                    assert (bx, by - m.height, adv) == (g["x"], g["y"], g["advance"])
                    if "vertical" in path.stem and name != ".notdef":
                        assert (m.vertBearingX, m.vertBearingY, m.vertAdvance) == (g["x"] - 1, g["y"] + g["height"] - 3, 20)
                if area:
                    check_png(image.imageData, g)
                for field in ("width", "height", "x", "y", "advance", "pixels", "codes"):
                    assert native[ppem][name][field] == g[field], (path, ppem, name, field)
                actual = rendered(face, gid)
                # FreeType intentionally normalizes zero-area bitmap placement.
                fields = ("width", "height", "advance", "pixels") if not area else ("width", "height", "x", "y", "advance", "pixels")
                for field in fields:
                    assert actual[field] == g[field], (path, ppem, name, field, actual[field], g[field])
                glyph_checks += 1
                pixel_checks += area
        # Force fontTools to serialize every parsed native table, then decode its
        # independently rebuilt output with Rust and compare the entire model.
        repacked = path.with_name(path.stem + "-fonttools.ttf")
        font.save(repacked)
        actual = json.loads(subprocess.check_output([str(args.binary), "read", str(repacked)], text=True))
        assert actual == expected["read"], (path, "fontTools recompile/native read mismatch")
        results.append(dict(file=path.name, bytes=path.stat().st_size,
                            sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                            fonttools_recompiled_sha256=hashlib.sha256(repacked.read_bytes()).hexdigest(),
                            strikes=len(expected["source"]), glyphs=glyph_checks, pixels=pixel_checks,
                            fonttools=True, freetype=True, pillow=True, native_recompiled_read=True))
        total_pixels += pixel_checks
        total_glyphs += glyph_checks
    report = dict(versions={k: importlib.metadata.version(k) for k in ("fonttools", "freetype-py", "pillow")},
                  freetype_library=_system, freetype_version=freetype.version(), cases=results,
                  glyph_checks=total_glyphs, pixel_checks=total_pixels)
    (args.output / "crosscheck.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
