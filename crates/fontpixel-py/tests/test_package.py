"""The top-level package exposes the native API, version and exception hierarchy."""
import fontpixel


def test_package_exports():
    assert fontpixel.__version__
    for name in ("FontSet", "Font", "Glyph", "Bitmap", "read", "write", "Registry"):
        assert hasattr(fontpixel, name), name
    for name in ("formats", "sfnt", "woff", "variants", "merge", "outline", "ops"):
        __import__(f"fontpixel.{name}")
    assert issubclass(fontpixel.ResourceLimitError, fontpixel.FontpixelError)
    assert issubclass(fontpixel.StrictLossError, fontpixel.FontpixelError)
