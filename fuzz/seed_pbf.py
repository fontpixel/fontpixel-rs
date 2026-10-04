#!/usr/bin/env python3
"""Regenerate exactly eight original PBF fuzz seeds using only the stdlib.

These authored bitmaps and aliases are the fixture family from
``tests/crosscheck/pbf.py::fixture``. They are neither upstream font extracts nor
saved libFuzzer mutations. Wire layouts follow ``docs/formats/pbf.md``. The two
explicit malformed/extended variants reproduce the decoder branches examined
before fuzz acceptance. Existing seed files must already match; unrelated corpus
files are never read, copied, rewritten or deleted.
"""
import argparse
import hashlib
import itertools
import json
import struct
from pathlib import Path


def fixture(version, rle=False, cp_bytes=2, off_bytes=2):
    """Two original raster records, the first with two Unicode aliases."""
    entries = [(65, 0), (66, 0), (0x1F600 if cp_bytes == 4 else 67, 1)]
    images = [
        (-2, 3, 10, [[int(v) for v in row] for row in
                     ["101001011", "001110010", "111000100"]]),
        (1, -2, -3, [[0] * 7 for _ in range(2)]),
    ]
    data = bytearray(4)
    offsets = []
    for left, top, advance, rows in images:
        offsets.append(len(data))
        width, height = len(rows[0]), len(rows)
        pixels = list(itertools.chain.from_iterable(rows))
        packed = bytearray()
        if rle:
            units = []
            for colour, group in itertools.groupby(pixels):
                remaining = len(list(group))
                while remaining:
                    run = min(8, remaining)
                    units.append((colour << 3) | (run - 1))
                    remaining -= run
            height = len(units)
            for i in range(0, len(units), 2):
                high = units[i + 1] if i + 1 < len(units) else 0
                packed.append(units[i] | (high << 4))
        else:
            for i in range(0, len(pixels), 8):
                packed.append(sum(v << j for j, v in enumerate(pixels[i:i + 8])))
        if version == 1:
            data.extend(struct.pack("<BBbb3xb", width, height, left, top, advance))
        else:
            data.extend(struct.pack("<BBbbb", width, height, left, top, advance))
        data.extend(packed)
        while version == 1 and len(data) % 4:
            data.append(0)
    header = struct.pack("<BBHH", version, 8, len(entries), 65)
    if version > 1:
        header += bytes([1, cp_bytes])
    if version == 3:
        header += bytes([10, int(off_bytes == 2) | (int(rle) << 1)])
    if version > 1:
        header += struct.pack("<BBH", 0, len(entries), 0)
    table = b"".join(
        cp.to_bytes(cp_bytes, "little")
        + (offsets[index] // (4 if version == 1 else 1)).to_bytes(off_bytes, "little")
        for cp, index in entries
    )
    return header + table + data


def seeds():
    """Stable names and bytes used by the initial eight-seed PBF corpus."""
    result = {}
    for version, rle, cp_bytes, off_bytes in [
        (1, False, 2, 2), (2, False, 2, 4), (2, False, 4, 4),
        (3, False, 2, 2), (3, True, 2, 2), (3, True, 4, 4),
    ]:
        name = f"v{version}-{rle}-{cp_bytes}-{off_bytes}"
        result[name] = fixture(version, rle, cp_bytes, off_bytes)
    partial = bytearray(fixture(3, True, 2, 2))
    # 10-byte header + 4-byte hash + three 4-byte mappings + 4-byte
    # null prefix + 5-byte glyph header = first RLE pair at offset 35.
    partial[35] ^= 1
    result["rle-partial-tail"] = bytes(partial)
    extended = bytearray(fixture(3, False, 2, 2))
    extended[8] = 12
    extended[10:10] = b"AB"
    result["v3-extended-header"] = bytes(extended)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-dir", type=Path,
        default=Path(__file__).resolve().parent / "corpus/pbf",
    )
    args = parser.parse_args()
    fixtures = seeds()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for name, data in fixtures.items():
        path = args.output_dir / name
        if path.exists() and path.read_bytes() != data:
            raise SystemExit(f"Refusing to overwrite different seed: {path}")
    for name, data in fixtures.items():
        path = args.output_dir / name
        if not path.exists():
            with path.open("xb") as output:
                output.write(data)
    print(json.dumps({
        name: {"bytes": len(data), "sha256": hashlib.sha256(data).hexdigest()}
        for name, data in fixtures.items()
    }, indent=2))


if __name__ == "__main__":
    main()
