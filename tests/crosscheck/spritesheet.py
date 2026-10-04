#!/usr/bin/env python3
"""Compare image imports with the independent MIT catalogue spritesheet importer.

Fixture semantics are adapted from reference/fontpixel/pipeline/tests/
test_spritesheet.py; the oracle is opf/ingest/spritesheet.py, imported unchanged.
The irregular-cell fixture and this CLI/evidence runner are original additions.

MIT License — Copyright (c) 2026 Tom Chen

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
"""

import argparse
import hashlib
import json
import platform
import subprocess
import sys
import traceback
from pathlib import Path

import PIL
from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
REFERENCE = ROOT.parent / "reference/fontpixel"
sys.dont_write_bytecode = True
sys.path.insert(0, str(REFERENCE / "pipeline"))
from opf.ingest.spritesheet import convert_spritesheet
from opf.parsers.bdf import parse_bdf


def digest(path):
    with path.open("rb") as source:
        return {"bytes": path.stat().st_size, "sha256": hashlib.file_digest(source, "sha256").hexdigest()}


def save_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def signature(font):
    def glyph(g):
        return {"code": g.cp, "name": g.name, "advance": g.dwidth,
                "bbx": [g.bbw, g.bbh, g.bbx, g.bby], "rows_hex": g.rows.hex()}

    return {"metrics": {"pixel_size": font.pixel_size, "ascent": font.ascent,
                        "descent": font.descent, "bounds": list(font.bbox)},
            "glyphs": [glyph(g) for g in font.glyphs],
            "unencoded_glyphs": [glyph(g) for g in font.unencoded_glyphs]}


def fixtures():
    wide = Image.new("RGBA", (18, 3), (10, 20, 30, 255))
    for point in [(0, 0), (8, 2), (9, 1)]:
        wide.putpixel(point, (255, 255, 255, 255))
    yield "wide-palette-baseline", wide, {
        "pixel_size": 3, "baseline": 2, "cell_size": [9, 3],
        "characters": "AЖ", "foreground": [[255, 255, 255]],
    }, None

    blank = Image.new("RGBA", (3, 1))
    blank.putpixel((2, 0), (255, 255, 255, 255))
    yield "omit-blank-space-unencoded", blank, {
        "pixel_size": 1, "omit_blank": True,
        "cells": [{"rect": [0, 0, 1, 1], "char": " "},
                  {"rect": [1, 0, 1, 1], "char": "A"},
                  {"rect": [2, 0, 1, 1], "name": "ornament"}],
    }, None

    irregular = Image.new("RGBA", (12, 4), (20, 40, 60, 0))
    for point in [(0, 0), (8, 2), (9, 1), (11, 2)]:
        irregular.putpixel(point, (255, 255, 255, 255))
    irregular.putpixel((7, 1), (255, 255, 255, 0))
    yield "irregular-bearing-advance", irregular, {
        "pixel_size": 4, "foreground": [[255, 255, 255]],
        "cells": [{"rect": [0, 0, 9, 3], "char": "Ж", "name": "Cyrillic",
                   "bearing_x": -2, "baseline": 1, "advance": 12},
                  {"rect": [9, 1, 3, 2], "char": "A", "name": "Alpha",
                   "bearing_x": 2, "baseline": 3, "advance": 5}],
    }, None

    for name, characters, reason in [
        ("visible-unmapped", "A", "no character mapping"),
        ("duplicate-unicode", "AA", "duplicate"),
    ]:
        yield name, Image.new("RGBA", (2, 1), "white"), {
            "pixel_size": 1, "cell_size": [1, 1], "characters": characters,
        }, reason


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    binary = args.binary.resolve()
    original_binary = digest(binary)
    destination = args.output_dir.resolve()
    destination.mkdir(parents=True, exist_ok=False)
    sources = [REFERENCE / "LICENSE", REFERENCE / "pipeline/tests/test_spritesheet.py",
               REFERENCE / "pipeline/opf/ingest/spritesheet.py",
               REFERENCE / "pipeline/opf/model.py", REFERENCE / "pipeline/opf/parsers/bdf.py",
               REFERENCE / "pipeline/opf/parsers/charset_map.py", Path(__file__).resolve()]
    results = {
        "oracle": "Unmodified MIT catalogue convert_spritesheet; catalogue BDF parser reads CLI output",
        "attribution": "Copyright (c) 2026 Tom Chen; fixture semantics adapted from test_spritesheet.py",
        "environment": {"python": sys.version, "pillow": PIL.__version__, "platform": platform.platform()},
        "binary": {"path": str(binary), **original_binary},
        "sources": {str(p): digest(p) for p in sources}, "checks": [], "commands": [],
        "metric_scope": "Compare all ParsedFont metrics (pixel size, ascent, descent, bounds). "
                        "Native point size equals pixel size at 72 dpi, checked separately; "
                        "ParsedFont carries no point-size/resolution fields.",
    }

    def command(folder, label, *argv):
        process = subprocess.run([str(binary), *map(str, argv)], capture_output=True, timeout=60)
        (folder / f"{label}.stdout").write_bytes(process.stdout)
        (folder / f"{label}.stderr").write_bytes(process.stderr)
        results["commands"].append({"case": folder.name, "label": label,
                                    "args": list(map(str, argv)), "exit_code": process.returncode})
        return process

    def successful(process):
        assert process.returncode == 0, process.stderr.decode(errors="replace")
        return json.loads(process.stdout)

    for name, image, spec, rejection in fixtures():
        folder = destination / name
        folder.mkdir()
        path = folder / "sheet.png"
        image.save(path)
        mapping = {**spec, "image": path.name}
        map_path = path.with_suffix(".json")
        save_json(map_path, mapping)
        check = {"case": name, "status": "failed"}
        try:
            if rejection is not None:
                try:
                    convert_spritesheet(path, "sample", mapping)
                except ValueError as error:
                    assert rejection in str(error), str(error)
                    check["oracle_rejection"] = str(error)
                else:
                    raise AssertionError("catalogue unexpectedly accepted invalid fixture")
                for entrypoint in [path, map_path]:
                    label = entrypoint.suffix[1:]
                    output = folder / f"rejected-{label}.bdf"
                    process = command(folder, label, "convert", entrypoint, "-o", output,
                                      "--from", "image", "--to", "bdf", "--report", "json")
                    assert process.returncode == 1, (process.returncode, process.stderr.decode())
                    error = json.loads(process.stderr)["error"]["message"]
                    assert rejection in error, error
                    assert not output.exists(), "rejected conversion unexpectedly produced a BDF"
                check["entrypoints"] = ["png", "json"]
            else:
                expected = convert_spritesheet(path, "sample", mapping)
                oracle = signature(expected)
                save_json(folder / "catalogue.json", oracle)
                omissions = 1 if spec.get("omit_blank") else 0
                for entrypoint in [path, map_path]:
                    label = entrypoint.suffix[1:]
                    info = successful(command(folder, f"{label}-info", "info", entrypoint,
                                              "--from", "image", "--json"))
                    assert info["font_count"] == 1, info
                    font = info["fonts"][0]
                    actual_metrics = {key: font[key] for key in oracle["metrics"]}
                    assert actual_metrics == oracle["metrics"], (actual_metrics, oracle["metrics"])
                    assert font["point_size"] == {"numerator": expected.pixel_size, "denominator": 1}
                    assert font["resolution"] == [72, 72]
                    assert font["charset"] == "unicode"
                    assert font["glyph_count"] == len(expected.glyphs) + len(expected.unencoded_glyphs)
                    assert font["encoded_count"] == len(expected.glyphs)
                    assert font["unencoded_count"] == len(expected.unencoded_glyphs)
                    losses = [e for e in info["report"]["entries"] if e["impact"] == "loss"]
                    assert len(losses) == omissions, losses
                    assert all(e["code"] == "image.read.omit_blank" for e in losses), losses
                    output = folder / f"rust-{label}.bdf"
                    converted = successful(command(folder, f"{label}-convert", "convert", entrypoint,
                                                   "-o", output, "--from", "image", "--to", "bdf",
                                                   "--report", "json"))
                    parsed = parse_bdf(output, "sample")
                    assert not parsed.warnings, parsed.warnings
                    actual = signature(parsed)
                    save_json(folder / f"rust-{label}.json", actual)
                    assert actual == oracle, {"expected": oracle, "actual": actual}
                    assert sum(e["code"] == "image.read.omit_blank"
                               for e in converted["report"]["entries"]) == omissions
                if omissions:
                    strict_output = folder / "strict-rejected.bdf"
                    process = command(folder, "strict-omission", "convert", path, "-o", strict_output,
                                      "--strict", "--report", "json")
                    assert process.returncode == 3, process.stderr.decode()
                    assert not strict_output.exists()
                    assert any(e["code"] == "image.read.omit_blank"
                               for e in json.loads(process.stderr)["report"]["entries"])
                check.update({"glyphs": len(expected.glyphs), "unencoded": len(expected.unencoded_glyphs),
                              "omissions": omissions, "entrypoints": ["png", "json"],
                              "compared": ["all packed pixel rows", "code", "name", "advance", "BBX",
                                           "pixel_size", "ascent", "descent", "font bounds"]})
            check["status"] = "passed"
        except Exception:
            check["failure"] = traceback.format_exc()
        results["checks"].append(check)

    final_binary = digest(binary)
    results["binary_unchanged"] = final_binary == original_binary
    results["artifacts"] = {str(p.relative_to(destination)): digest(p)
                            for p in sorted(destination.rglob("*")) if p.is_file()}
    results["status"] = ("passed" if results["binary_unchanged"]
                         and all(c["status"] == "passed" for c in results["checks"]) else "failed")
    report_path = destination / "results.json"
    save_json(report_path, results)
    (destination / "results.sha256").write_text(digest(report_path)["sha256"] + "  results.json\n")
    print(json.dumps({"status": results["status"], "checks": results["checks"],
                      "report": str(report_path)}, ensure_ascii=False, indent=2))
    return 0 if results["status"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
