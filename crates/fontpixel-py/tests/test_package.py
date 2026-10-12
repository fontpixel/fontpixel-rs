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


def test_distribution_metadata_and_typing_marker():
    from importlib import metadata, resources

    assert metadata.version("fontpixel") == fontpixel.__version__
    assert resources.files("fontpixel").joinpath("py.typed").is_file()
    # The bdfparser-compatible API is a stable import path (bdfparser 4.x
    # re-exports it); its version is the bdfparser API it follows.
    import fontpixel.compat.bdfparser as bdfparser
    from fontpixel.compat.bdfparser import cli, encoding, outline, pcf, ttf

    assert bdfparser.__version__.startswith("3.")
    for name in bdfparser.__all__:
        assert hasattr(bdfparser, name), name
    for module, name in [(pcf, "write_pcf"), (pcf, "read_pcf"), (ttf, "write_ttf"), (ttf, "read_ttf"),
                         (encoding, "char2cp"), (outline, "bitmap_contours"), (cli, "main")]:
        assert callable(getattr(module, name)), name
