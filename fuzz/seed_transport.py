#!/usr/bin/env python3
"""Deterministic original transport inputs (MIT), independent of Rust writers.

The 16-byte control prefix belongs to the harness, not a font format. gzip
members use RFC 1952 header/trailer fields and stdlib raw DEFLATE; no upstream
font artwork or executable implementation is copied. --check is read-only.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import zlib

BDF = b'''STARTFONT 2.1
FONT OriginalTransport
SIZE 2 75 75
FONTBOUNDINGBOX 2 2 0 0
STARTPROPERTIES 2
FONT_ASCENT 2
FONT_DESCENT 0
ENDPROPERTIES
CHARS 1
STARTCHAR A
ENCODING 65
SWIDTH 1000 0
DWIDTH 2 0
BBX 2 2 0 0
BITMAP
80
40
ENDCHAR
ENDFONT
'''
SSFN_ASCII = b'''# Scalable Screen Font #
$glyphdim 2 2
$type 3
$style regular
$baseline 2
$underline 2
$name "Original Transport"
$family "Original"
$subfamily "Regular"
$license "MIT"
===U+000041===w2=h2=x2=y0=o0="A"===
X.
.X
# End #
'''

def ssfn_binary():
    # Independently assembled 2x2 diagonal raster, one Unicode U+0041 glyph.
    strings = b'\0'.join([b'Original Transport', b'Original', b'Regular', b'1', b'Fontpixel', b'MIT']) + b'\0'
    fragment_offset = 32 + len(strings)
    fragment = bytes([0x80, 1, 1, 2])
    chars = bytearray([0xc0, 64, 0, 1, 2, 2, 2, 0, 0, 0])
    chars.extend(fragment_offset.to_bytes(3, 'little'))
    remaining = 0x110000 - 66
    while remaining >= 65536:
        chars.append(255); remaining -= 65536
    while remaining > 64:
        n = min(remaining, 16128) - 1
        chars.extend([192 + (n >> 8), n & 255]); remaining -= n + 1
    if remaining:
        chars.append(128 + remaining - 1)
    h = bytearray(32); h[:4] = b'SFN2'; h[8:14] = bytes([3, 0, 2, 2, 2, 2])
    h[14:16] = fragment_offset.to_bytes(2, 'little')
    h[16:20] = (fragment_offset + len(fragment)).to_bytes(4, 'little')
    out = h + strings + fragment + chars + b'2NFS'
    out[4:8] = len(out).to_bytes(4, 'little')
    return bytes(out)

def gzip(data):
    c = zlib.compressobj(9, zlib.DEFLATED, -15)
    body = c.compress(data) + c.flush()
    return bytes.fromhex('1f8b08000000000000ff') + body + struct.pack('<II', zlib.crc32(data), len(data))

def header(fmt=0, wire=False, flags=0, profile=0, dims=(0, 0, 0, 0)):
    h = bytearray(16)
    h[0] = fmt
    h[1] = int(wire) | flags
    h[2] = profile
    h[3:7] = b'\xff' * 4
    h[8:12] = b'\xff' * 4
    h[12:16] = bytes(dims)
    return h

def seeds():
    out = {}
    for fmt, name in enumerate(['raw', 'bdf', 'ssfn']):
        for profile in ([0, 1, 2, 4, 8, 16, 32, 46, 63] if fmt == 0 else [0, 128]):
            h = header(fmt, profile=profile, dims=(8, 4, 2, 33))
            out[f'{name}-model-profile-{profile}'] = h + bytes.fromhex('81a542183ce799')
        for field in range(4):
            for value in [0, 1, 3, 4, 5, 8, 10, 255]:
                for common in [False, True]:
                    h = header(fmt, flags=32 if common else 0)
                    h[(8 if common else 3) + field] = value
                    out[f'{name}-{"common" if common else "typed"}-{field}-{value}'] = h + b'\x80'
        for flags in [2, 16, 18, 34, 48]:
            out[f'{name}-strict-case-{flags}'] = header(fmt, flags=flags) + b'\x80'
    for fmt, name, plain, profile in [(0, 'raw', b'\x80', 0), (1, 'bdf', BDF, 0), (2, 'ssfn-ascii', SSFN_ASCII, 128), (2, 'ssfn-binary', ssfn_binary(), 0)]:
        h = header(fmt, wire=True, profile=profile)
        encoded = gzip(plain)
        out[name+'-wire-plain'] = h + plain
        out[name+'-wire-gzip'] = h + encoded
        cut = len(plain) // 2
        out[name+'-wire-members'] = h + gzip(plain[:cut]) + gzip(plain[cut:])
        out[name+'-wire-nested'] = h + gzip(encoded)
        out[name+'-wire-trailing'] = h + encoded + b'junk'
        for at in [-8, -4, 2, 3]:
            bad = bytearray(encoded); bad[at] ^= 0x40
            out[f'{name}-wire-corrupt-{at}'] = h + bad
        for cut in [0, 1, 2, 9, 10, len(encoded)-8, len(encoded)-1]:
            out[f'{name}-wire-truncated-{cut}'] = h + encoded[:cut]
        for field in range(4):
            limited = h.copy(); limited[8+field] = 4
            out[f'{name}-read-budget-{field}'] = limited + encoded
    # Original repeated bytes exceed the harness's decompressed cap while wire
    # stays small. The model parser need not accept these to exercise inflation.
    out['read-expansion-over-cap'] = header(0, wire=True) + gzip(b'\0' * 262145)
    out['read-chunk-growth'] = header(0, wire=True) + gzip(b'\0' * 14000)
    return {name: bytes(data) for name, data in sorted(out.items())}

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--root', type=Path, default=Path(__file__).parent/'corpus'/'transport')
    p.add_argument('--check', action='store_true')
    p.add_argument('--manifest', type=Path)
    a = p.parse_args()
    rows = []
    for name, data in seeds().items():
        dest = a.root/name
        if a.check:
            assert dest.read_bytes() == data, str(dest)
        else:
            a.root.mkdir(parents=True, exist_ok=True); dest.write_bytes(data)
        rows.append({'name': name, 'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()})
    result = {'license': 'MIT original synthetic artwork and control inputs', 'zlib_runtime': zlib.ZLIB_RUNTIME_VERSION, 'seeds': rows}
    if a.manifest and not a.check:
        a.manifest.write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps({'seeds': len(rows), 'bytes': sum(r['bytes'] for r in rows), 'check': a.check}))
if __name__ == '__main__':
    main()
