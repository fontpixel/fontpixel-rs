#!/usr/bin/env python3
"""Recreate eight original MIT KBITX seeds; leave libFuzzer mutations untouched."""

import argparse
import base64
import hashlib
import json
from pathlib import Path


def fixtures():
    """Return only the named project-authored fixtures, in stable name order."""
    wires = {
        "mono": bytes([12, 8, 0x63]),
        "gray": bytes([2, 3, 0xC6, 0, 127, 128, 254, 255, 0]),
        "long-zero": bytes([34, 1, 0x21, 0x42]),
        "long-repeat": bytes([34, 1, 0xA1, 0x7F, 0x42]),
        "long-literal": bytes([34, 1, 0xE1] + [0, 255] * 16 + [0x42]),
        "wide": bytes([1, 0x82, 1, 0x64, 0x42]),
    }
    seeds = {
        name + ".xml": (
            '<kbits><prop id="emAscent" value="10"/>'
            '<prop id="emDescent" value="2"/>'
            '<g u="65" x="-1" y="10" w="9" d="'
            + base64.b64encode(raw).decode("ascii")
            + '"/></kbits>\n'
        ).encode("utf-8")
        for name, raw in wires.items()
    }
    seeds["metadata.xml"] = (
        '<?xml version="1.0" encoding="UTF-8"?>'
        '<!DOCTYPE kbits SYSTEM "https://invalid.test/never">'
        '<kbits><name id="1" value="A &amp; B"/>'
        '<g n="empty" d="AAA"/><k ln="empty" rn="empty" o="-1"/>'
        '<feat name="liga">sub A B by C;</feat>'
        '<puaa><pt prop="Block"><pv range="0041..0042" value="Latin"/>'
        '</pt></puaa></kbits>\n'
    ).encode("utf-8")
    seeds["empty.xml"] = b"<kbits/>\n"
    return dict(sorted(seeds.items()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-dir", type=Path,
        default=Path(__file__).resolve().parent / "corpus/kbitx",
    )
    parser.add_argument("--check", action="store_true", help="Verify existing seeds without writing")
    args = parser.parse_args()
    if not args.check:
        args.output_dir.mkdir(parents=True, exist_ok=True)
    manifest = {}
    for name, data in fixtures().items():
        path = args.output_dir / name
        if args.check:
            if not path.is_file() or path.read_bytes() != data:
                raise SystemExit(f"Seed differs from generated fixture: {path}")
        else:
            path.write_bytes(data)
        manifest[name] = {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
    print(json.dumps(manifest, indent=2) + "\n", end="")


if __name__ == "__main__":
    main()
