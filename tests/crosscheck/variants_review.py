#!/usr/bin/env python3
"""Independent inherited-metric, vertical-sbit and table-drop review regressions."""
import argparse
import json
import logging
import subprocess
from pathlib import Path

import freetype
import ots
import uharfbuzz as hb
from fontTools.ttLib import TTFont

from variants import invoke, sfnt_bytes


def fixture(path, advance, vertical_advance, origin_y, explicit=False):
    metrics = (
        f"SWIDTH 500 0\nDWIDTH {advance} 0\nSWIDTH1 0 -1000\n"
        f"DWIDTH1 0 -{vertical_advance}\nVVECTOR {advance // 2} {origin_y}\n"
    )
    path.write_text(
        "STARTFONT 2.2\nFONT Test\nSIZE 14 75 75\nFONTBOUNDINGBOX 2 2 0 0\n"
        "METRICSSET 2\n" + ("" if explicit else metrics)
        + "STARTPROPERTIES 5\nPIXEL_SIZE 14\nFONT_ASCENT 14\nFONT_DESCENT 0\n"
        'CHARSET_REGISTRY "ISO10646"\nCHARSET_ENCODING "1"\nENDPROPERTIES\n'
        "CHARS 1\nSTARTCHAR A\nENCODING 65\n" + (metrics if explicit else "")
        + "BBX 2 2 0 0\nBITMAP\nC0\n40\nENDCHAR\nENDFONT\n"
    )


def shape(data, language, script, direction):
    font = hb.Font(hb.Face(data))
    font.scale = (1400, 1400)
    buffer = hb.Buffer()
    buffer.add_str("A")
    buffer.language, buffer.script, buffer.direction = language, script, direction
    hb.shape(font, buffer)
    info, pos = buffer.glyph_infos[0], buffer.glyph_positions[0]
    return info.codepoint, pos.x_advance, pos.y_advance, pos.x_offset, pos.y_offset


def validate(path):
    font = TTFont(path)
    font.ensureDecompiled()
    raw = sfnt_bytes(path)
    assert font["EBLC"].strikes[0].bitmapSizeTable.flags == 3
    face = freetype.Face(str(path))
    face.select_size(0)
    results = {}
    for lang, script, advance, height, origin in [
        ("zh-Hans", "hani", 4, 14, 12),
        ("ja", "hani", 6, 16, 13),
        ("ko", "hani", 6, 16, 13),
        ("en", "latn", 2, 12, 11),
    ]:
        horizontal = shape(raw, lang, script, "ltr")
        vertical = shape(raw, lang, script, "ttb")
        gid = horizontal[0]
        assert horizontal[1:3] == (advance * 100, 0)
        assert vertical[1:3] == (0, -height * 100)
        assert vertical[0] == gid
        name = font.getGlyphName(gid)
        assert font["hmtx"][name] == (advance * 100, 0)
        assert font["vmtx"][name] == (height * 100, (origin - 2) * 100)
        metrics = font["EBDT"].strikeData[0][name].metrics
        expected = (-advance // 2, 2 - origin, height)
        assert (metrics.vertBearingX, metrics.vertBearingY, metrics.vertAdvance) == expected
        face.load_glyph(
            gid, freetype.FT_LOAD_SBITS_ONLY | freetype.FT_LOAD_RENDER
            | freetype.FT_LOAD_VERTICAL_LAYOUT
        )
        m = face.glyph.metrics
        assert (m.vertBearingX, m.vertBearingY, m.vertAdvance) == tuple(x * 64 for x in expected)
        assert m.horiAdvance == advance * 64
        results[(lang, script)] = (horizontal, vertical, metrics.__dict__)
    assert results[("ja", "hani")][0][0] == results[("ko", "hani")][0][0]
    result = ots.sanitize(str(path), str(path) + ".ots", capture_output=True)
    assert result.returncode == 0, result.stderr
    Path(str(path) + ".ots").unlink()
    return results, font.reader["name"]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.work.mkdir(parents=True, exist_ok=True)
    logging.getLogger("fontTools.ttLib.tables._h_e_a_d").setLevel(logging.ERROR)
    for name, advance, height, origin, explicit in [
        ("base", 4, 14, 12, False), ("ja", 6, 16, 13, False),
        ("ko", 6, 16, 13, True), ("latin", 2, 12, 11, False),
    ]:
        fixture(args.work / (name + ".bdf"), advance, height, origin, explicit)
    common = [
        "variants", "--base", args.work / "base.bdf",
        "--lang", f"ja={args.work / 'ja.bdf'}", "--lang", f"ko={args.work / 'ko.bdf'}",
        "--script", f"latn={args.work / 'latin.bdf'}", "--embed-bitmaps",
    ]
    cases = []
    for shape_name in ["square", "dot"]:
        for flavor in ["ttf", "otf"]:
            source = args.work / f"metrics-{shape_name}.{flavor}"
            invoke(args.binary, *common, "--shape", shape_name, "-o", source)
            expected, names = validate(source)
            for container in [flavor, "woff", "woff2"]:
                output = args.work / f"subset-{shape_name}-{flavor}.{container}"
                invoke(args.binary, "subset", source, "--text", "A", "-o", output,
                       "--woff2-quality", "5")
                actual, subset_names = validate(output)
                assert actual == expected
                assert subset_names == names
            cases.append({"shape": shape_name, "flavor": flavor,
                          "inherited_and_explicit_metrics_deduplicate": True,
                          "vertical_sbit_fontTools_FreeType_HarfBuzz_OTS": "pass",
                          "subset_containers": [flavor, "woff", "woff2"]})
    destination = args.work / "sentinel.ttf"
    destination.write_bytes(b"unchanged")
    source = args.work / "metrics-square.ttf"
    for tag in ["vhea", "vmtx"]:
        result = subprocess.run(
            [str(args.binary), "subset", str(source), "--text", "A", "--drop-table", tag,
             "-o", str(destination)], capture_output=True, text=True
        )
        assert result.returncode == 1
        assert "dropped together" in result.stderr
        assert destination.read_bytes() == b"unchanged"
    invoke(args.binary, "subset", source, "--text", "A", "--drop-table", "vhea",
           "--drop-table", "vmtx", "-o", destination)
    font = TTFont(destination)
    assert "vhea" not in font and "vmtx" not in font
    result = ots.sanitize(str(destination), str(destination) + ".ots", capture_output=True)
    assert result.returncode == 0, result.stderr
    Path(str(destination) + ".ots").unlink()
    args.output.write_text(json.dumps({"partial": False, "cases": cases,
                                     "paired_metric_drops_and_atomic_failure": "pass"}, indent=2) + "\n")
    print("REVIEW PASS: four outline cases, twelve subsets, paired-drop validation")


if __name__ == "__main__":
    main()
