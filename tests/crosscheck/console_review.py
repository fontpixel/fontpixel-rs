#!/usr/bin/env python3
"""Independent W3.1 review probes using primary-spec wire bytes and the real CLI.

All fixtures are generated in temporary storage. No reference/sample is modified.
Run with --binary PATH --output report.json; only the Python standard library is used.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    results = []
    with tempfile.TemporaryDirectory(prefix="console-independent-review-") as tmp:
        root = Path(tmp)
        def roundtrip(name, wire, *options):
            source = root / name
            output = root / ("out-" + name)
            source.write_bytes(wire)
            run = subprocess.run([args.binary, "convert", str(source), "-o", str(output), "--strict", "--report", "json", *options], capture_output=True)
            assert run.returncode == 0, run.stderr.decode()
            assert output.read_bytes() == wire, name
            report = json.loads(run.stdout)
            results.append({"case": name, "bytes": len(wire), "sha256": hashlib.sha256(wire).hexdigest(), "result": "strict exact byte roundtrip", "report": report})

        # Exhaust every representable scalar, without using either codec's helpers.
        aliases = "".join(chr(cp) for cp in range(0x110000) if not 0xD800 <= cp <= 0xDFFF)
        table = aliases.encode("utf-8") + b"\xff"
        wire = struct.pack("<8I", 0x864AB572, 0, 32, 1, 1, 1, 1, 8) + b"\xa5" + table
        roundtrip("all-1112064-scalars.psf", wire)
        table = b"".join(struct.pack("<H", cp) for cp in range(0xFFFE) if not 0xD800 <= cp <= 0xDFFF)
        wire = bytes([0x36, 4, 2, 1]) + b"\xa5" * 256 + table + b"\xff\xff" * 256
        roundtrip("all-63486-psf1-scalars.psf", wire, "--psf-version", "1")

        # Multiple single-scalar sequences, aliases and marker-like UTF-8 values.
        edges = [0, 0x7F, 0x80, 0x7FF, 0x800, 0xD7FF, 0xE000, 0xFFFD, 0xFFFE, 0xFFFF, 0x10000, 0x10FFFF]
        table = b"AB" + b"".join(b"\xfe" + chr(cp).encode("utf-8") for cp in edges) + b"\xff"
        wire = struct.pack("<8I", 0x864AB572, 0, 32, 1, 1, 2, 1, 9) + b"\xa5\x80" + table
        roundtrip("unicode-sequence-edges.psf", wire)
        for width in (8, 16, 24, 32):
            wire = "".join(f"{cp:04X}:" + "A5" * (width // 8 * 16) + "\n" for cp in edges).encode()
            roundtrip(f"width-{width}.hex", wire)

        # Every malformed byte stream must fail without replacing the old output.
        bad_tables = [b"\x80\xff", b"\xc0\x80\xff", b"\xc1\xbf\xff", b"\xe0\x80\x80\xff", b"\xed\xa0\x80\xff", b"\xf0\x80\x80\x80\xff", b"\xf4\x90\x80\x80\xff", b"\xf5\x80\x80\x80\xff", b"\xfe\xff", b"A\xff\xff"]
        for table in bad_tables:
            source = root / "bad.psf"
            output = root / "protected.psf"
            source.write_bytes(struct.pack("<8I", 0x864AB572, 0, 32, 1, 1, 1, 1, 8) + b"\x80" + table)
            output.write_bytes(b"existing destination")
            run = subprocess.run([args.binary, "convert", str(source), "-o", str(output)], capture_output=True)
            assert run.returncode != 0
            assert output.read_bytes() == b"existing destination"
        results.append({"case": "malformed UTF-8, empty sequence, trailing entry", "count": len(bad_tables), "result": "all rejected, existing destination preserved"})
    args.output.write_text(json.dumps({"result": "pass", "checks": results}, indent=2) + "\n")
    print(f"{len(results)} independent review groups passed")


if __name__ == "__main__":
    main()
