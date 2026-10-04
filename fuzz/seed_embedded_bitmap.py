#!/usr/bin/env python3
"""Seed full SFNTs and raw bitmap table pairs from project-authored fixtures."""
import argparse
import hashlib
import json
from pathlib import Path
import struct


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("fuzz/corpus/embedded_bitmap"))
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    rows = []
    fixtures = Path(__file__).resolve().parents[1] / "tests/data/embedded-bitmap"
    for path in sorted(fixtures.glob("*.ttf")):
        data = path.read_bytes()
        tables = {}
        for index in range(struct.unpack_from(">H", data, 4)[0]):
            tag, _, offset, length = struct.unpack_from(">4sIII", data, 12 + index * 16)
            tables[tag] = data[offset:offset + length]
        if not any(tag in tables for tag in (b"EBLC", b"bloc")):
            continue
        seeds = {path.stem + ".sfnt": data}
        count = struct.unpack_from(">H", tables[b"maxp"], 4)[0]
        for location_tag, data_tag in [(b"EBLC", b"EBDT"), (b"bloc", b"bdat")]:
            if location_tag in tables and data_tag in tables:
                location = tables[location_tag]
                assert 0 < count <= 256 and len(location) <= 65535
                seeds[path.stem + ".tables"] = struct.pack(">HH", len(location), count - 1) + location + tables[data_tag]
        for name, seed in seeds.items():
            (args.output / name).write_bytes(seed)
            rows.append({"name": name, "bytes": len(seed), "sha256": hashlib.sha256(seed).hexdigest()})
    print(json.dumps({"source": "tests/data/embedded-bitmap/SOURCES.json", "seeds": rows}, indent=2))


if __name__ == "__main__":
    main()
