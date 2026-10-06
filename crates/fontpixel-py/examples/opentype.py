"""OpenType output: outline TTF, bitmap-only OTB, WOFF2, subsets and variants.

    python opentype.py [INPUT] [OUTPUT_DIR]

Writes to OUTPUT_DIR:

* `font.ttf` -- pixels vectorized into TrueType outlines (square pixels);
* `font.otb` -- embedded bitmaps only (EBDT/EBLC);
* `font.woff2` -- the TTF in a WOFF2 container (decoded again to check);
* `latin-subset.ttf` -- the TTF reduced to the characters of a text;
* `variants.ttf` -- a locl-enabled font whose Japanese glyphs for "A"/"B"
  come from a second font.
"""
from __future__ import annotations

import sys

import fontpixel as fp
from _common import ALT, arguments
from fontpixel import sfnt, variants
from fontpixel.formats import embedded, vector_raster
from fontpixel.types import ExportOptions, VectorRasterWriteOptions
from fontpixel.woff import woff2


def main(argv: list[str] | None = None) -> int:
    source, out = arguments(__doc__ or "", argv)
    fonts = fp.read(source.read_bytes(), name=source.name).fonts
    assert isinstance(fonts, fp.FontSet)

    # Outline TrueType: every ink pixel becomes a square (merged contours).
    outline_options: VectorRasterWriteOptions = {"outline": {"shape": "square", "family": "Pixel Example"}}
    ttf, report = vector_raster.write(fonts, "ttf", outline_options)
    (out / "font.ttf").write_bytes(ttf)
    print(f"ttf: {len(ttf)} bytes, {len(report['entries'])} report entries")

    # Bitmap-only OpenType (OTB): the strikes are stored as EBDT/EBLC.
    otb, _ = embedded.write(fonts, "open_type")
    (out / "font.otb").write_bytes(otb)

    # Inspect the container: table directory, flavor, checksums.
    parsed = sfnt.Sfnt(ttf)
    parsed.validate_checksums()
    print("flavor:", parsed.flavor, "| tables:", [t["tag"] for t in parsed.tables()])

    # WOFF2 round trip.
    packed = woff2.encode(ttf)
    (out / "font.woff2").write_bytes(packed["bytes"])
    unpacked = woff2.decode(packed["bytes"])
    assert sfnt.Sfnt(unpacked["sfnt"]).data == unpacked["sfnt"]
    print(f"woff2: {len(packed['bytes'])} bytes (sfnt {len(ttf)} bytes)")

    # Subset to the characters of a text (cmap/GSUB closure included).
    subset = sfnt.subset.subset(ttf, sfnt.subset.options_text("Hello"))
    (out / "latin-subset.ttf").write_bytes(subset["bytes"])
    print("subset glyph mapping:", dict(sorted(subset["glyph_mapping"].items())[:6]))

    # Regional variants: a Japanese (`JAN`) locl alternative from another font.
    regional = fp.read(ALT.read_bytes(), name=ALT.name).fonts
    assert isinstance(regional, fp.FontSet)
    v = variants.Variants(fonts[0])
    v.add_language("ja", regional[0])
    export: ExportOptions = {"family": "Pixel Variants", "embed_bitmaps": True}
    encoded = v.encode(export)
    (out / "variants.ttf").write_bytes(encoded["bytes"])
    gsub = sfnt.Sfnt(encoded["bytes"]).table("GSUB")
    print("variants: GSUB table present:", gsub is not None,
          "| languages:", [item["language"] for item in v.languages])
    return 0


if __name__ == "__main__":
    sys.exit(main())
