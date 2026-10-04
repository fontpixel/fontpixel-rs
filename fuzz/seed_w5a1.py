#!/usr/bin/env python3
"""Original MIT synthetic rasters and independently authored native wire seeds.
No historical Dr. Halo protection stub or external font artwork is included.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct


def fixtures():
    data = {
        'dosstart': b'DosStartFont\nOriginal\n1, 2\n' + b'003002HGHGHH, 4\n' * 96,
        'edwin': b'65 66\n65: 2 3 5 6\nbee: 1 3 3\n',
        'mkwinfont': b'facename Original\nheight 2\nascent 1\npointsize 9\ncharset 0\n# comment\nchar 65\nwidth 3\n101\n011\n',
        'psf2txt': b'%PSF2\nVersion: 0\nFlags: 1\nLength: 2\nWidth: 3\nHeight: 2\n%\nBitmap: #-# \\\n        -##\nUnicode: [0041];[0391];[0061+0301];\n%\nBitmap: ##- \\\n        #--\n',
        'pcr': b'KPG\x01\x02\x20\x01\x02\0\0\x02' + bytes([0xa5, 0x63])*256,
    }
    b = bytearray(896); b[:9] = b'PLUS3DOS\x1a'; b[9] = 1
    struct.pack_into('<IBHH', b, 11, 896, 3, 768, 0x3d00)
    b[22] = ord('f'); b[127] = sum(b[:127]) & 255; b[128:] = bytes([0xa5])*768
    data['plus3dos'] = bytes(b)
    b = bytearray(20); b[:2] = b'AH'
    struct.pack_into('<HBBHHHHHHH', b, 2, 0xdf, 0x0a, 2, 0x300, 0, 127, 4, 2, 9, 2)
    data['drhalo'] = bytes(b) + bytes([0xa5, 0x80, 0x63, 0, 0x80, 0, 0, 0x80])
    data['writeon'] = b'ES\x01\0' + struct.pack('<9H', 22, 8, 14, 14, 65, 66, 0, 8, 0) + bytes([0xa5])*28
    return data


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--root', type=Path, default=Path(__file__).parent/'corpus')
    parser.add_argument('--manifest', type=Path)
    args = parser.parse_args()
    manifest = {}
    for ident, original in fixtures().items():
        directory = args.root/ident; directory.mkdir(parents=True, exist_ok=True)
        seeds = {'original': original}
        for cut in (0, 1, 7, 15, len(original)//2, len(original)-1):
            seeds[f'truncated-{cut}'] = original[:cut]
        seeds['integer-boundary'] = original.replace(b'ascent 1', b'ascent -9223372036854775808').replace(b'1, 2', b'1, -9223372036854775808').replace(b'Width: 3', b'Width: 4294967295')
        if ident == 'mkwinfont': seeds['utf8'] = original.replace(b'Original', '字型 Café'.encode())
        if ident == 'psf2txt': seeds['utf8-bom-comment'] = b'\xef\xbb\xbf' + original.replace(b'Version:', '// 注释\nVersion:'.encode())
        if ident == 'edwin': seeds['utf8-label'] = original.replace(b'bee:', '字形:'.encode())
        if ident == 'edwin': seeds['wide129'] = f'65 65\n65: 1 129 {(1<<128)+1}\n'.encode()
        if ident == 'psf2txt': seeds['invalid-scalar'] = original.replace(b'0041', b'd800')
        if ident == 'drhalo': seeds['zero-origin'] = original[:2] + b'\xc8' + original[3:]
        if ident == 'pcr': seeds['v1'] = original[:4] + b'\x01' + original[5:]
        for name, value in seeds.items():
            (directory/name).write_bytes(value)
            manifest[f'{ident}/{name}'] = dict(bytes=len(value), sha256=hashlib.sha256(value).hexdigest())
    if args.manifest:
        args.manifest.write_text(json.dumps(dict(license='Original project-authored fixtures: MIT', seeds=manifest), indent=2)+'\n')
    print(f'W5.A1: {len(manifest)} original seeds in {args.root}')


if __name__ == '__main__':
    main()
