#!/usr/bin/env python3
"""Generate small original image/C fuzz seeds; never replace minimized findings."""
import json
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent / "corpus"


def put(target, name, data):
    path = ROOT / target / ("seed-" + name)
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        path.write_bytes(data)


def png(width, height):
    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))
    rows = b"".join(b"\0" + bytes(255 if (x + y) % 2 else 0 for x in range(width)) for y in range(height))
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 0, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


def image_seed(mode, primary, sidecar=b""):
    return bytes([mode]) + struct.pack("<I", len(primary)) + primary + sidecar


mapping = {"image": "a.png", "pixel_size": 2, "cell_size": [2, 2], "characters": "A", "foreground": [[255, 255, 255]]}
put("image", "json", image_seed(1, json.dumps(mapping).encode(), png(2, 2)))
put("image", "toml", image_seed(2, b'image="a.png"\npixel_size=2\ncell_size=[2,2]\ncharacters="A"\nforeground=[[255,255,255]]\n', png(2, 2)))
put("image", "grid", image_seed(0, png(2, 26)))
put("c", "generic", b'\x01const uint8_t font[2][4]={{128,128,64,0},{32,0,16,128}};')
put("c", "font8x8", b'\x02// unicode points U+0041 - U+0041\nchar font8x8_basic[1][8]={{1,2,4,8,16,32,64,128}};')
put("c", "gfx", b'\x04const uint8_t Bits[]={0xA8};\nconst GFXglyph Glyphs[]={{0,3,2,4,-1,-2}};\nconst GFXfont Font={(uint8_t*)Bits,(GFXglyph*)Glyphs,65,65,3};')
rows = [f'{{ // {cp} (0x{cp:02X})\n0b10000000,0,0,0,0,0,0,0\n}}' for cp in range(32, 128)]
put("c", "tmat", b'\x03static const unsigned char chars[][8]={' + ',\n'.join(rows).encode() + b'};')
print("Image/C synthetic seeds are available under fuzz/corpus/")
