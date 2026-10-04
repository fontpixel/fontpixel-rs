#!/usr/bin/env python3
"""Generate original HBF header/sidecar seeds; no external fonts are copied."""
import argparse
from pathlib import Path


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", type=Path, default=Path(__file__).parent / "corpus/hbf")
    args = p.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    cases = [
        ("unicode-ascii", "Unicode 1.1", [], [], [(65, 66, 0)], 65),
        ("unicode-cjk", "Unicode 1.1", [(0, 255)], [], [(0x4E00, 0x4E03, 3)], 0x4E00),
        ("vendor-sparse", "Vendor 2 bytes", [(0x40, 0x41), (0xFE, 0xFF)], [], [(0xA141, 0xA240, 2), (0xA241, 0xA241, 18)], 0xA141),
        ("vendor-three", "Vendor 3 bytes", [(2, 3)], [(4, 4), (7, 7)], [(0x10207, 0x10307, 3)], 0x10207),
        ("unicode-three", "Unicode", [(0, 255)], [(0, 255)], [(0x10000, 0x10003, 0)], 0x10000),
        ("gb-gl", "GB2312-1980", [(0x21, 0x7E)], [], [(0x2121, 0x2123, 0)], 0x2121),
        ("jis-gr", "JISX0208-1990", [(0xA1, 0xFE)], [], [(0xA1A1, 0xA1A3, 0)], 0xA1A1),
        ("cns-plane", "CNS11643-92p3", [(0x21, 0x7E)], [], [(0x2121, 0x2123, 0)], 0x2121),
    ]
    for name, scheme, second, third, ranges, default in cases:
        h = ["COMMENT original synthetic seed", "HBF_START_FONT 1.1", f"HBF_CODE_SCHEME {scheme}", "FONT Seed",
             "SIZE 12 72 72", "HBF_BITMAP_BOUNDING_BOX 9 2 -1 -1", "FONTBOUNDINGBOX 11 4 -2 -2",
             "STARTPROPERTIES 5", f"DEFAULT_CHAR {default}", 'FAMILY_NAME "Seed ""quoted"""', "CUSTOM 077", 'SPACING "c"', "UNSIGNED 18446744073709551615", "ENDPROPERTIES"]
        for n, byte_ranges in [(2, second), (3, third)]:
            if byte_ranges:
                h.append(f"HBF_START_BYTE_{n}_RANGES {len(byte_ranges)}")
                h.extend(f"HBF_BYTE_{n}_RANGE 0x{a:X}-0x{b:X}" for a, b in byte_ranges)
                h.append(f"HBF_END_BYTE_{n}_RANGES")
        h.append(f"HBF_START_CODE_RANGES {len(ranges)}")
        h.extend(f"HBF_CODE_RANGE 0x{a:X}-0x{b:X} bitmap.bin {offset}" for a, b, offset in ranges)
        h.extend(["HBF_END_CODE_RANGES", "HBF_END_FONT", ""])
        header = "\n".join(h).encode("ascii")
        # Plenty of bounded data; every row is monochrome padding-clean.
        data = len(header).to_bytes(4, "little") + header + bytes(128)
        (args.output / name).write_bytes(data)
    print(f"wrote {len(cases)} original seeds to {args.output}")


if __name__ == "__main__":
    main()
