"""Format helpers outside the uniform detect/read/write namespaces."""
import importlib
import sys

import pytest

import fontpixel
import fontpixel.formats as F
from formats_support import REPO, SOURCES, assets_of, load_golden, sha, single

GOLDEN = load_golden()
UNI3 = fontpixel.read(open(SOURCES["uni3"], "rb").read())
UNI5 = fontpixel.read(open(SOURCES["uni5"], "rb").read())


def yaff(model, report=None):
    return single(fontpixel.write(model, "yaff", report=report))


def test_namespaces_are_importable_modules():
    assert len(F.MODULES) == 93
    for name in F.MODULES:
        mod = importlib.import_module(f"fontpixel.formats.{name}")
        assert mod is getattr(F, name) is sys.modules[f"fontpixel.formats.{name}"]
        assert mod.FORMAT_IDS and mod.__doc__
        for fn in ("read", "write"):
            assert hasattr(mod, fn) or name == "x11", name
    served = {fid for name in F.MODULES for fid in getattr(F, name).FORMAT_IDS if name != "x11"}
    assert served == {s["format"]["id"] for s in fontpixel.list_formats()}
    from fontpixel.formats import bdf

    assert bdf.read.__doc__ and bdf.read.__text_signature__


def test_catalogue_constant():
    assert F.CATALOGUE == [s["format"] for s in fontpixel.list_formats()]
    assert F.CATALOGUE[0] == {"id": "bdf", "name": "Glyph Bitmap Distribution Format",
                              "extensions": ["bdf", "bdf.gz"], "feature_enabled": True}


def test_direct_detectors_reject_other_content():
    assert F.bdf.detect(b"not a font") == "no_match"
    assert F.pcf.detect(single(fontpixel.write(UNI3.fonts, "pcf"))) == "certain"
    assert F.hzk.detect(b"\0" * 64) == "no_match"  # HZK has no magic
    assert F.gem.detect(b"\0" * 8, name="font.gft") == "no_match"
    with pytest.raises(fontpixel.ResourceLimitError):
        F.gem.detect(b"\0" * 64, limits={"max_input_bytes": 8})
    assert not hasattr(F.raw, "detect")  # raw has no detector at all


def test_direct_reader_shapes_and_limits():
    data = single(fontpixel.write(UNI3.fonts, "bdf"))
    font, report = F.bdf.read(data)
    assert isinstance(font, fontpixel.Font) and report == {"entries": []}
    with pytest.raises(fontpixel.ResourceLimitError):
        F.bdf.read(data, limits={"max_glyphs": 3})
    set_, _ = F.yaff.read(yaff(UNI3.fonts))
    assert isinstance(set_, fontpixel.FontSet)
    # Font-only writers take a one-strike set or a Font, never several strikes.
    two = UNI3.fonts.copy()
    two.append(UNI3.fonts[0].copy())
    with pytest.raises(fontpixel.InvalidInputError):
        F.bdf.write(two)
    out, rep = F.bdf.write(UNI3.fonts[0])
    assert out == data and rep == {"entries": []}
    with pytest.raises(TypeError):
        F.bdf.write(b"bytes")


def test_x11_source_rows_and_validation():
    font, _ = F.bdf.read(single(fontpixel.write(UNI3.fonts, "bdf")))
    glyph = font[1]
    assert F.bdf.source_rows(glyph) == F.x11.source_rows(glyph) == ["80", "80", "80", "00", "80"]
    assert F.x11.validate_bitmap(glyph) is None
    fresh = fontpixel.Glyph(glyph.bitmap)
    assert F.x11.source_rows(fresh) is None


def test_embedded_codecs():
    otb = GOLDEN["formats"]["otb"]["write"]["assets"]["font.otb"]
    data, report = F.embedded.write(UNI3.fonts, "open_type")
    assert sha(data) == otb and isinstance(report, dict)
    assert F.embedded.detect(data, "open_type") == "certain"
    assert F.embedded.detect(b"junk", "open_type") == "no_match"
    model, _ = F.embedded.read(data, "open_type")
    assert yaff(model) == yaff(fontpixel.read(data, "otb").fonts)
    with pytest.raises(ValueError):
        F.embedded.read(data, "png")
    color, _ = F.embedded.write_color(UNI3.fonts)
    assert sha(color) == GOLDEN["formats"]["color"]["write"]["assets"]["font.ttf"]
    codec = F.embedded.codec("apple")
    assert codec["kind"] == "apple" and set(codec) == {"kind", "read_options", "write_options", "color_write_options"}
    md = F.normalize("formats::embedded::metadata::EmbeddedMetadataOptions")
    assert F.embedded.validate_metadata(md, "open_type") is None
    assert F.embedded.metadata_retained_bytes(md) == 0
    with pytest.raises(fontpixel.InvalidInputError):
        F.embedded.validate_metadata({**md, "glyph_order": "codepoint"}, "color")


def test_vector_raster_helpers():
    ttf, report = F.vector_raster.write(UNI3.fonts, "ttf")
    assert sha(ttf) == GOLDEN["formats"]["ttf"]["write"]["assets"]["font.ttf"]
    assert F.vector_raster.detect_native_ppem(ttf) == 6
    with pytest.raises(fontpixel.ResourceLimitError):
        F.vector_raster.detect_native_ppem(ttf, {"sfnt": {"resources": {"max_input_bytes": 10}}})
    assert F.vector_raster.detect(ttf, "ttf") == "certain"
    model, _ = F.vector_raster.read(ttf)
    assert yaff(model) == yaff(fontpixel.read(ttf, "ttf").fonts)
    moved = UNI3.fonts.copy()
    owned, _ = F.vector_raster.write_owned(moved, "ttf")
    assert owned == ttf and len(moved) == 0
    with pytest.raises(fontpixel.InvalidInputError):
        F.vector_raster.write(UNI3.fonts, "bdf")
    codec = F.vector_raster.codec("woff2")
    assert codec["id"] == "woff2" and codec["write_options"]["woff2"]["quality"] == 11
    ttf_fixture = open(f"{REPO}/tests/data/compat/mini-circle-1.0-0.ttf", "rb").read()
    assert F.vector_raster.detect_native_ppem(ttf_fixture) is None
    with pytest.raises(fontpixel.InvalidInputError):
        F.vector_raster.read(ttf_fixture)
    model, _ = F.vector_raster.read(ttf_fixture, {"ppem": 16})
    assert sha(yaff(model)) == GOLDEN["fixtures"]["ttf-mini-circle"]["yaff"]


def test_svg_image_helpers():
    font = UNI3.fonts[0]
    exclam = font.glyph_for_codepoint(fontpixel.Codepoint.unicode(0x21))
    svg = F.svg.image(exclam.bitmap)
    assert svg.startswith("<svg") and 'viewBox="0 0 1 5"' in svg
    assert F.svg.image(exclam.bitmap) == svg  # deterministic
    dots = F.svg.image(exclam.bitmap, {"shape": "dot", "dot_size": 0.5})
    assert dots != svg and "a0.25 0.25 0 1 0" in dots  # true circular dots use arcs
    assert F.svg.glyph_image(exclam).startswith("<svg")
    assert F.svg.path_data(exclam.bitmap, (0, 0), 10) == "M0 0V10H10V0ZM0 20V50H10V20Z"
    with pytest.raises(fontpixel.InvalidInputError):
        F.svg.path_data(exclam.bitmap, (0, 0), 0)
    with pytest.raises(fontpixel.InvalidInputError):
        F.svg.image(exclam.bitmap, {"dot_size": 2.0})
    gray = fontpixel.Bitmap(2, 2, depth="gray8")
    with pytest.raises(fontpixel.InvalidInputError):
        F.svg.image(gray)


def test_fnt_fbit_nfnt_hanfont_hzk_raw_helpers():
    assert F.fnt.charset_name(0) == "cp1252" and F.fnt.charset_name(77) == "macroman"
    assert F.fnt.charset_name(255) is None
    fnt_bytes = single(fontpixel.write(UNI3.fonts, "fnt"))
    font, _ = F.fnt.read_default(fnt_bytes)
    assert yaff(font) == yaff(F.fnt.read(fnt_bytes)[0])
    # Hangul component slots (crates/fontpixel/tests/hanfont.rs).
    assert F.hanfont.component_slots(0xAC00) == (1, 161, None)
    assert F.hanfont.component_slots(0xAC00 + 1) == (101, 205, 249)
    with pytest.raises(fontpixel.InvalidInputError):
        F.hanfont.component_slots(0xABFF)
    presets = F.hzk.presets()
    assert len(presets) == 25 and presets[0] == "hzk12" and "asc48" in presets
    for p in presets:
        assert F.hzk.preset_name(p) == p
        g = F.hzk.preset_geometry(p)
        assert F.normalize("formats::hzk::Geometry", g) == g
    assert F.hzk.preset_geometry("asc16") == {"width": 8, "height": 16, "first_code": 0, "last_code": 255,
                                              "code_space": "ascii", "storage": "rows"}
    assert F.raw.raw_layout(3, 6)["width"] == 3 and F.raw.raw_layout(3, 6)["first_codepoint"] == 0
    assert F.raw.preset_layout("vga16")["height"] == 16
    assert F.raw.preset_layout("spectrum")["first_codepoint"] == 32
    # fbit / nfnt bundles from the direct writers.
    fbit_src = GOLDEN["formats"]["fbit"]
    from formats_support import NATIVE

    model, _ = F.fbit.read_context(NATIVE["fbit"]())
    assert sha(yaff(model)) == fbit_src["source"]["yaff"]
    w = F.fbit.write_bundle(model)
    assert {k: sha(v) for k, v in assets_of(w).items()} == fbit_src["write"]["assets"]
    leg3 = fontpixel.read(open(SOURCES["leg3"], "rb").read())
    nfnt = F.nfnt.write_bundle(leg3.fonts, primary_name="font.dfont")
    assert {k: sha(v) for k, v in assets_of(nfnt).items()} == GOLDEN["formats"]["nfnt"]["write"]["assets"]
