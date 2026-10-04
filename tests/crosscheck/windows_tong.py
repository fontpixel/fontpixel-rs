#!/usr/bin/env python3
"""Check every surviving Tong FNT pixel and every omitted mapping independently."""
import argparse
from collections import Counter
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import sys

import monobit

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT.parent / "reference/fontpixel/pipeline"))
from opf.parsers.bdf import parse_bdf


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--source", type=Path, default=ROOT.parent / "samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf")
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=False)
    source = parse_bdf(args.source, "tong")
    assert not source.unencoded_glyphs, "This probe expects the supplied fully encoded Tong sample"
    expected, unmapped, zero_width = {}, set(), set()
    for glyph in source.glyphs:
        if glyph.dwidth == 0:
            zero_width.add(glyph.cp)
            continue
        try:
            code = chr(glyph.cp).encode("cp1252")
        except UnicodeEncodeError:
            unmapped.add(glyph.cp)
            continue
        assert len(code) == 1 and code[0] not in expected
        expected[code[0]] = glyph

    target = args.output_dir / "tong.fnt"
    command = [str(args.binary.resolve()), "convert", str(args.source), "-o", str(target), "--to", "fnt", "--report", "json"]
    result = subprocess.run(command, capture_output=True)
    assert result.returncode == 0, result.stderr.decode()
    (args.output_dir / "tong-report.json.gz").write_bytes(gzip.compress(result.stdout, mtime=0))
    report = json.loads(result.stdout)["report"]["entries"]
    for code, expected_codes in (("fnt.unmapped_code", unmapped), ("fnt.zero_width_code", zero_width)):
        found = [e["location"]["codepoint"] for e in report if e["code"] == code]
        assert len(found) == len(expected_codes) and set(found) == expected_codes
    assert sum(e["code"] == "fnt.omitted_glyph" for e in report) == len(unmapped | zero_width)

    output = monobit.load(target, format="win")[0]
    assert len(output.glyphs) == len(expected)
    top = max([source.ascent, 0] + [g.bby + g.bbh for g in expected.values()])
    bottom = min([-source.descent, 0] + [g.bby for g in expected.values()])
    for glyph in output.glyphs:
        original = expected[int.from_bytes(bytes(glyph.codepoint), "big")]
        width, height = original.dwidth, top - bottom
        assert (glyph.width, glyph.height, glyph.advance_width, glyph.left_bearing, glyph.shift_up) == (width, height, width, 0, bottom)
        stride = (width + 7) // 8
        rows = bytearray(stride * height)
        source_stride = (original.bbw + 7) // 8
        for sy in range(original.bbh):
            y = top - (original.bby + original.bbh) + sy
            for sx in range(original.bbw):
                x = original.bbx + sx
                if 0 <= x < width and 0 <= y < height and original.rows[sy * source_stride + sx // 8] & (0x80 >> (sx % 8)):
                    rows[y * stride + x // 8] |= 0x80 >> (x % 8)
        assert glyph.as_bytes() == bytes(rows), f"Pixel mismatch for U+{original.cp:04X}"

    # Strict conversion fails before persistence, both for absent and existing output.
    protected = target.read_bytes()
    strict = subprocess.run(command + ["--strict"], capture_output=True)
    assert strict.returncode == 3 and target.read_bytes() == protected
    strict_report = json.loads(strict.stderr)["report"]
    assert strict_report["entries"] == report
    absent = args.output_dir / "must-not-exist.fnt"
    absent_command = command.copy()
    absent_command[4] = str(absent)
    strict_new = subprocess.run(absent_command + ["--strict"], capture_output=True)
    assert strict_new.returncode == 3 and not absent.exists()
    summary = {
        "result": "pass", "command": command, "source_sha256": digest(args.source),
        "binary_sha256": digest(args.binary), "input_glyphs": len(source.glyphs),
        "output_glyphs": len(output.glyphs), "output_bytes": target.stat().st_size,
        "output_sha256": digest(target), "report_entries": len(report),
        "counts_by_code": dict(Counter(e["code"] for e in report)),
        "verification": "All CP1252 glyph pixels/metrics checked against the independent catalogue BDF parser and monobit; every unmapped/zero-width code checked against Python cp1252; strict preserves existing and absent outputs",
    }
    (args.output_dir / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
