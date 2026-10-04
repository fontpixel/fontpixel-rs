#!/usr/bin/env python3
"""Independent frozen catalogue checks for KBITX (monobit has no handler)."""
import argparse
import base64
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT.parent / "reference/fontpixel/pipeline"))
from opf.ingest.kbitx import convert_kbitx
from opf.parsers.bdf import parse_bdf


def signature(g):
    return g.cp, g.bbw, g.bbh, g.bbx, g.bby, g.dwidth, g.rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--source", type=Path, action="append", default=[])
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    binary = str(args.binary.resolve())
    dest = args.output_dir.resolve()
    dest.mkdir(parents=True, exist_ok=False)
    checks = []

    def command(*argv):
        result = subprocess.run([binary, *map(str, argv)], capture_output=True)
        assert result.returncode == 0, (argv, result.stderr.decode())
        return result

    # Original fixtures explicitly exercise all four WIB kinds, long-run bit 5,
    # signed bearings, grayscale 127/128 and Unicode outside the BMP.
    wires = [
        bytes([34, 1, 0x21, 0x42]),
        bytes([34, 1, 0x61, 0x01, 0x41]),
        bytes([34, 1, 0xA1, 0x80, 0x01, 0x41]),
        bytes([34, 1, 0xA1, 0x7F, 0x42]),
        bytes([34, 1, 0xE1] + [0, 255] * 16 + [0x42]),
        bytes([1, 0x82, 1, 0x64, 0x42]),
        bytes([0x82, 1, 1, 0x64, 0x42]),
        bytes([2, 3, 0xC6, 0, 127, 128, 254, 255, 0]),
        bytes([12, 8, 0x63]),
    ]
    fixture = dest / "synthetic.kbitx"
    records = [f'<g u="{65 + i if i != 8 else 0x20000}" x="-2" y="12" w="9" d="{base64.b64encode(wire).decode()}"/>' for i, wire in enumerate(wires)]
    fixture.write_text('<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE kbits SYSTEM "https://invalid.test/never-fetched.dtd">\n<kbits><prop id="emAscent" value="10"/><prop id="emDescent" value="2"/><name id="1" value="Synthetic &amp; original"/>' + "".join(records) + "</kbits>\n")
    sources = [fixture, *[p.resolve() for p in args.source]]
    for index, source in enumerate(sources):
        expected = convert_kbitx(source, "oracle")
        bdf = dest / f"case-{index}.bdf"
        command("convert", source, "-o", bdf, "--from", "kbitx", "--to", "bdf", "--kbitx-read-threshold", "128")
        actual = parse_bdf(bdf, "oracle")
        expected_sig = sorted(map(signature, expected.glyphs))
        actual_sig = sorted(map(signature, actual.glyphs))
        assert actual_sig == expected_sig, (source, len(actual_sig), len(expected_sig), next(((a, b) for a, b in zip(actual_sig, expected_sig) if a != b), None))
        assert (actual.ascent, actual.descent, actual.pixel_size) == (expected.ascent, expected.descent, expected.pixel_size)
        native = dest / f"case-{index}.kbitx"
        command("convert", source, "-o", native, "--from", "kbitx", "--to", "kbitx", "--strict")
        # The independent catalogue consumes our emitted standard wire format.
        reimported = convert_kbitx(native, "oracle")
        assert sorted(map(signature, reimported.glyphs)) == expected_sig
        again = dest / f"case-{index}-again.kbitx"
        command("convert", native, "-o", again, "--strict")
        assert native.read_bytes() == again.read_bytes()
        checks.append({"oracle": "frozen MIT catalogue kbitx importer", "source": str(source), "sha256": hashlib.sha256(source.read_bytes()).hexdigest(), "glyphs": len(actual_sig), "source_bytes": source.stat().st_size, "output_bytes": native.stat().st_size, "catalogue_warnings": expected.warnings, "strict_native_roundtrip": True})
    oracle = ROOT.parent / "reference/fontpixel/pipeline/opf/ingest/kbitx.py"
    artifacts = {
        path.name: {"bytes": path.stat().st_size, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
        for path in sorted(dest.iterdir()) if path.is_file()
    }
    evidence = {
        "checks": checks,
        "python": sys.version,
        "binary_sha256": hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
        "oracle_sha256": hashlib.sha256(oracle.read_bytes()).hexdigest(),
        "artifacts": artifacts,
    }
    (dest / "results.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps(checks, indent=2))


if __name__ == "__main__":
    main()
