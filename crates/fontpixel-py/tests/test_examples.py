"""Run every example of crates/fontpixel-py/examples on repository fixtures.

Each example is called in-process as ``main([INPUT, tmp_path])`` and must
return 0 and write only below ``tmp_path``; its output files are then checked
against independent reads of the same data.
"""
from __future__ import annotations

import importlib
import json
import sys
from pathlib import Path

import pytest

import fontpixel as fp

EXAMPLES = Path(__file__).resolve().parents[1] / "examples"
REPO = Path(__file__).resolve().parents[3]
SPLEEN = REPO / "tests" / "data" / "bdf" / "spleen-5x8.bdf"
ALL = sorted(p.stem for p in EXAMPLES.glob("*.py") if not p.name.startswith("_"))


def run(name: str, out: Path, monkeypatch: pytest.MonkeyPatch, source: Path = SPLEEN,
        cwd: Path | None = None) -> None:
    monkeypatch.syspath_prepend(str(EXAMPLES))
    monkeypatch.chdir(cwd or out)   # anything written relative to cwd stays in tmp
    module = importlib.import_module(name)
    try:
        assert module.main([str(source), str(out)]) == 0
    finally:
        sys.modules.pop(name, None)


def files(root: Path) -> set[str]:
    return {p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file()}


def font_of(data: bytes, **kw: object) -> fp.Font:
    fonts = fp.read(data, **kw).fonts  # type: ignore[arg-type]
    assert isinstance(fonts, fp.FontSet)
    return fonts[0]


def test_every_example_is_tested():
    tested = {name[len("test_"):] for name in globals() if name.startswith("test_")}
    assert set(ALL) <= tested, set(ALL) - tested
    assert len(ALL) >= 9


def test_read_inspect_convert(tmp_path, monkeypatch, capsys):
    run("read_inspect_convert", tmp_path, monkeypatch)
    assert files(tmp_path) == {"spleen-5x8.font.pcf", "spleen-5x8.font.psf", "spleen-5x8.font.yaff"}
    source = font_of(SPLEEN.read_bytes())
    for name, fmt in (("pcf", "pcf"), ("psf", "psf"), ("yaff", "yaff")):
        back = font_of((tmp_path / f"spleen-5x8.font.{name}").read_bytes(), format=fmt)
        glyph = back.glyph_for_codepoint("A")
        assert glyph is not None and glyph.bitmap == source.glyph_for_codepoint("A").bitmap
    assert "PCF round trip" in capsys.readouterr().out


def test_edit_glyphs(tmp_path, monkeypatch):
    run("edit_glyphs", tmp_path, monkeypatch)
    assert files(tmp_path) == {"edited.bdf"}
    font = font_of((tmp_path / "edited.bdf").read_bytes(), format="bdf")
    star = font.glyph_for_codepoint(0xE000)
    assert star is not None and star.name == "star"
    assert star.bitmap.to_text() == "..#..\n.###.\n#####\n.###.\n.#.#."
    a = font.glyph_for_codepoint("A")
    assert a is not None and a.bitmap.get_pixel(0, 0) == 1 and a.bitmap.get_pixel(4, 0) == 1
    assert font.names["family"] == "Spleen Edited"
    assert font.properties.get("COMMENT_EXAMPLE") is not None
    assert len(font) == len(font_of(SPLEEN.read_bytes()))   # one added, one removed


def test_render_text(tmp_path, monkeypatch):
    run("render_text", tmp_path, monkeypatch)
    assert files(tmp_path) == {"hello.png", "hello.svg", "vertical.png", "all.png"}
    font = font_of(SPLEEN.read_bytes())
    expected = font.draw("Hello, fontpixel!", {"letter_spacing": 1, "line_limit": 200}).bitmap
    png = fp.to_png(expected, {"scale": 4, "foreground": fp.RGBA_WHITE,
                               "background": fp.parse_rgba("#1b2a49")})
    assert (tmp_path / "hello.png").read_bytes() == png
    assert (tmp_path / "hello.svg").read_text().startswith("<svg")
    for name in ("vertical.png", "all.png"):
        assert (tmp_path / name).read_bytes()[:8] == b"\x89PNG\r\n\x1a\n"


def test_typed_options(tmp_path, monkeypatch, capsys):
    run("typed_options", tmp_path, monkeypatch)
    assert files(tmp_path) == {"font.psf", "font-msb.pcf", "font.psf2"}
    assert (tmp_path / "font.psf").read_bytes()[:2] == b"\x36\x04"          # PSF1 magic
    assert (tmp_path / "font.psf2").read_bytes()[:4] == b"\x72\xb5\x4a\x86"  # PSF2 magic
    pcf = font_of((tmp_path / "font-msb.pcf").read_bytes(), format="pcf")
    assert len(pcf) == len(font_of(SPLEEN.read_bytes()))
    out = capsys.readouterr().out
    assert "unknown variant `3`" in out and "unknown field `no_such_key`" in out


def test_opentype(tmp_path, monkeypatch):
    run("opentype", tmp_path, monkeypatch)
    assert files(tmp_path) == {"font.ttf", "font.otb", "font.woff2", "latin-subset.ttf", "variants.ttf"}
    ttf = (tmp_path / "font.ttf").read_bytes()
    assert fp.sfnt.Sfnt(ttf).flavor == "true_type"
    assert fp.woff.woff2.decode((tmp_path / "font.woff2").read_bytes())["sfnt"] == \
        fp.sfnt.Sfnt(fp.woff.woff2.decode(fp.woff.woff2.encode(ttf)["bytes"])["sfnt"]).data
    otb = font_of((tmp_path / "font.otb").read_bytes(), format="otb")
    assert otb.glyph_for_codepoint("A") is not None
    subset = fp.sfnt.Sfnt((tmp_path / "latin-subset.ttf").read_bytes())
    assert len(subset.table("glyf") or b"") < len(fp.sfnt.Sfnt(ttf).table("glyf") or b"")
    assert fp.sfnt.Sfnt((tmp_path / "variants.ttf").read_bytes()).table("GSUB") is not None


def test_merge_fonts(tmp_path, monkeypatch):
    run("merge_fonts", tmp_path, monkeypatch)
    assert files(tmp_path) == {"merged.bdf", "merged-ops.bdf", "merged-ops.pcf"}
    lib = font_of((tmp_path / "merged.bdf").read_bytes(), format="bdf")
    via_ops = font_of((tmp_path / "merged-ops.bdf").read_bytes(), format="bdf")
    assert len(lib) == len(via_ops) == 95
    alt = font_of((REPO / "crates/fontpixel-py/tests/data/p3/alt.bdf").read_bytes())
    for font in (lib, via_ops):
        a = font.glyph_for_codepoint("A")
        assert a is not None and a.bitmap == alt.glyph_for_codepoint("A").bitmap


def test_batch_jobs(tmp_path, monkeypatch):
    run("batch_jobs", tmp_path, monkeypatch)
    assert files(tmp_path / "memory") == {"out/font.pcf", "out/font.psf"}
    assert files(tmp_path / "disk") == {"font.bdf", "jobs.toml", "out/font.pcf", "out/font.ttf"}
    assert (tmp_path / "memory/out/font.pcf").read_bytes() == (tmp_path / "disk/out/font.pcf").read_bytes()


def test_resources_sidecars(tmp_path, monkeypatch):
    run("resources_sidecars", tmp_path, monkeypatch)
    assert files(tmp_path) == {"bmfont/font.fnt", "bmfont/font_0000.png", "ops/font.fnt", "ops/font_0000.png"}
    # The filesystem operation and the native writer produce the same files.
    for name in ("font.fnt", "font_0000.png"):
        assert (tmp_path / "ops" / name).read_bytes() == (tmp_path / "bmfont" / name).read_bytes()


def test_compat_bdfparser(tmp_path, monkeypatch):
    run("compat_bdfparser", tmp_path, monkeypatch)
    assert files(tmp_path) == {"hello.txt", "hello.bdf", "hello.pcf"}
    from fontpixel.compat.bdfparser import Font
    small = Font(str(tmp_path / "hello.bdf"))
    assert sorted(small.glyphs) == list(range(0x48, 0x50))
    assert len(font_of((tmp_path / "hello.pcf").read_bytes(), format="pcf")) == 8
    assert "#" in (tmp_path / "hello.txt").read_text()


def test_errors_limits(tmp_path, monkeypatch, capsys):
    run("errors_limits", tmp_path, monkeypatch)
    assert files(tmp_path) == {"strict-report.json"}
    report = json.loads((tmp_path / "strict-report.json").read_text())
    expected = fp.write(font_of(SPLEEN.read_bytes()), "pcf").report
    assert report == expected                     # strict raises with the full report
    out = capsys.readouterr().out
    for text in ("kind=StrictLoss exit_code=3", "glyph count > 10", "unknown format: 'no-such-format'",
                 "ops rejected request, exit code 2", "render strict: ['render.missing_glyph']",
                 "BdfParseError at line 2"):
        assert text in out


def test_examples_write_only_to_their_directory(tmp_path, monkeypatch):
    """The common argument parser creates the output directory, nothing else."""
    out = tmp_path / "nested" / "out"
    run("edit_glyphs", out, monkeypatch, cwd=tmp_path)
    assert files(tmp_path) == {"nested/out/edited.bdf"}
