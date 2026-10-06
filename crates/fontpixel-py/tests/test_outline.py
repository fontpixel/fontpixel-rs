"""fontpixel.outline compared with the same Rust calls."""
import pytest

import fontpixel
from fontpixel import outline
from fontpixel.outline import path
from sfnt_p3_support import E, norm

DOT = {"shape": "dot", "dot_size": 0.9}


def glyph_a():
    """The Mono bitmap of glyph A, rasterized from its TrueType contours."""
    return outline.rasterize(E["glyf.contours.2"], 64.0, (0, 0), 5, 7)


def test_outline_contours():
    bm = glyph_a()
    assert isinstance(bm, fontpixel.Bitmap)
    assert norm(outline.contours(bm)) == E["outline.contours"]
    assert norm(outline.contours(bm, outline.defaults()["Options"], outline.defaults()["Limits"])) == E[
        "outline.contours"
    ]
    assert norm(outline.pixel_origins(bm, DOT)) == E["outline.pixel_origins.dot"]
    assert norm(outline.pixel_contour(DOT)) == E["outline.pixel_contour.dot"]
    assert norm(outline.segments(outline.pixel_contour(DOT))) == E["outline.segments"]
    assert norm(outline.spec_bitmap_contours(["0110", "1001"], "circle", 0.8, 10, 1, -2)) == E["outline.spec"]
    # rasterizing the traced contours reproduces the same raster
    again = outline.rasterize(outline.contours(bm), 100.0, (0, 0), 5, 7)
    assert norm(outline.contours(again)) == E["outline.contours"]


def test_outline_path():
    edges = E["path.edges"]
    bm = path.rasterize(edges, 0.01, (0.0, 0.0), 4, 4)
    assert norm(outline.contours(bm)) == E["path.rasterize.contours"]
    assert [[norm(path.edge_points(e)), path.edge_max_y(e)] for e in edges] == E["path.points_max_y"]
    with pytest.raises(fontpixel.InvalidInputError):
        path.rasterize(edges, 0.0, (0.0, 0.0), 4, 4)
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        path.rasterize(edges, 0.01, (0.0, 0.0), 4, 4, {"max_edges": 1})
    assert (e.value.resource, e.value.limit) == ("path edges", 1)


def test_outline_errors_and_limits():
    bm = glyph_a()
    with pytest.raises(fontpixel.InvalidInputError):
        outline.contours(bm, {"dot_size": 0.5})
    with pytest.raises(fontpixel.InvalidInputError):
        outline.contours(bm, {"units_per_pixel": 0})
    with pytest.raises(fontpixel.ResourceLimitError) as e:
        outline.contours(bm, None, {"max_pixels": 4})
    assert (e.value.resource, e.value.limit) == ("outline pixels", 4)
    with pytest.raises(fontpixel.ResourceLimitError):
        outline.rasterize(E["glyf.contours.2"], 64.0, (0, 0), 5, 7, {"max_points": 3})
    with pytest.raises(fontpixel.InvalidInputError):
        outline.rasterize(E["glyf.contours.2"], -1.0, (0, 0), 5, 7)
    with pytest.raises(fontpixel.InvalidInputError):
        outline.spec_bitmap_contours(["01"], "hexagon", 1.0, 10, 0, 0)
    with pytest.raises(ValueError):
        outline.contours(bm, {"shape": "triangle"})
    d = outline.defaults()
    assert d["Options"] == {"shape": "square", "units_per_pixel": 100, "dot_size": 1.0, "origin": (0, 0)}
    assert d["Limits"]["max_points"] == 1024 * 1024
