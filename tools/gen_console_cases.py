#!/usr/bin/env python3
"""Generate small, original MIT HEX/PSF seeds; never read or modify references."""
import argparse
from pathlib import Path
import struct


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--directory", type=Path, default=Path(__file__).resolve().parents[1] / "fuzz/corpus")
    args = parser.parse_args()
    hex_dir, psf_dir = args.directory / "hex", args.directory / "psf"
    hex_dir.mkdir(parents=True, exist_ok=True)
    psf_dir.mkdir(parents=True, exist_ok=True)
    (hex_dir / "standard").write_text("0041:" + "A5"*16 + "\n0042:" + "55"*32 + "\n")
    (hex_dir / "wide").write_text("# generated\n1F600:" + "81"*48 + "\n10FFFF:" + "55"*64 + "\n")
    (hex_dir / "height24").write_text("0041:" + "A5"*24 + "\n")
    for width in (1,8,9,16,32):
        size = ((width + 7)//8)*2
        header = struct.pack("<8I",0x864AB572,0,32,1,3,size,2,width)
        table = b"A\xc3\x85\xfeA\xcc\x8a\xff\xfe\xf0\x9f\x98\x80\xff\xff"
        (psf_dir / f"psf2-{width}").write_bytes(header + bytes([0xAA])*(size*3) + table)
    for mode in range(8):
        count = 512 if mode & 1 else 256
        table = b"A\0\xfe\xffA\0\x0a\3\xff\xff" + b"\xff\xff"*(count-1) if mode & 6 else b""
        (psf_dir / f"psf1-{mode}").write_bytes(bytes([0x36,4,mode,2]) + bytes([0xAA])*(count*2) + table)
    print(f"Generated 3 HEX and 13 PSF seeds in {args.directory}")


if __name__ == "__main__":
    main()
