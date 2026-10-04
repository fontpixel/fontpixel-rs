#!/usr/bin/env python3
"""Original CC0 synthetic W5.2 fixtures; no third-party font artwork."""
from pathlib import Path
import argparse
import binascii
import struct


def hanfont():
    out = bytearray(360 * 32)
    for slot in range(360):
        if slot < 160:
            filler = slot % 20 == 0
            row = 0
        elif slot < 248:
            filler = (slot - 160) % 22 == 0
            row = 5
        else:
            filler = (slot - 248) % 28 == 0
            row = 10
        if not filler:
            out[slot * 32 + row * 2:slot * 32 + row * 2 + 2] = (slot + 1).to_bytes(2, 'big')
            out[slot * 32 + row * 2 + 2:slot * 32 + row * 2 + 4] = ((slot * 17) & 65535).to_bytes(2, 'big')
    return bytes(out)


def iigs():
    return b''.join((b'\x04Test', struct.pack('<6H', 6, 42, 3, 9, 0x101, 4),
                    struct.pack('<13H', 0xffff, 65, 66, 3, 0xffff, 0xffff, 3, 3, 12, 2, 1, 1, 1),
                    bytes((0xb8, 0, 0xc8, 0, 0xb8, 0)), struct.pack('<8H', 0, 2, 3, 5, 3, 0x102, 3, 0xffff)))


def extended_iigs():
    # Two 128 x 5000 rasters force a nonzero high word in the 1.5 header.
    strike = b'\xa5' * (32 * 5000)
    offset = (26 + len(strike) + 6 - 16) // 2
    return b''.join((b'\0', struct.pack('<7H', 7, 0, 0, 5000, 0x105, 128, offset >> 16),
                    struct.pack('<13H', 0, 65, 65, 128, 0, 0, 128, 5000, offset & 65535, 5000, 0, 0, 16),
                    strike, struct.pack('<6H', 0, 128, 256, 128, 128, 65535)))


def shifted_iigs(ascent, descent, leading):
    out = bytearray(iigs())
    for offset, value in [(25, 2), (27, -descent), (35, ascent), (37, descent), (39, leading)]:
        out[offset:offset + 2] = struct.pack('<h', value)
    return bytes(out)


def binscii(data):
    alphabet = b'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789()'

    def encode(raw):
        return bytes(alphabet[(int.from_bytes(raw[k:k + 3], 'big') >> shift) & 63]
                     for k in range(0, len(raw), 3) for shift in (0, 6, 12, 18))

    lines = []
    for start in range(0, len(data), 12288):
        part = data[start:start + 12288]
        header = bytearray(27)
        header[0:3] = len(data).to_bytes(3, 'little')
        header[3:6] = start.to_bytes(3, 'little')
        header[6:8] = b'\xe3\xc8'
        header[10] = 1 if len(data) <= 512 else 2 if len(data) <= 131072 else 3
        header[11:13] = ((len(data) + 511) // 512).to_bytes(2, 'little')
        header[21:24] = len(part).to_bytes(3, 'little')
        header[24:26] = binascii.crc_hqx(header[:24], 0).to_bytes(2, 'little')
        lines.extend((b'FiLeStArTfIlEsTaRt', alphabet, b'DFONT           ' + encode(header)))
        padded = part + b'\0' * (-len(part) % 48)
        lines.extend(encode(padded[k:k + 48]) for k in range(0, len(padded), 48))
        lines.append(encode(binascii.crc_hqx(padded, 0).to_bytes(2, 'little') + b'\0'))
    return b'\n'.join(lines) + b'\n'


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--directory', type=Path, default=Path('fuzz/corpus'))
    args = p.parse_args()
    for name, data in [('hanfont', hanfont()), ('iigs', iigs())]:
        folder = args.directory / name
        folder.mkdir(parents=True, exist_ok=True)
        (folder / 'original').write_bytes(data)
    h = bytearray(iigs())
    h[5:7] = struct.pack('<H', 7)
    h[13:15] = struct.pack('<H', 0x105)
    h[17:17] = b'\0\0'
    (args.directory / 'iigs' / 'version105').write_bytes(h)
    (args.directory / 'iigs' / 'extended-offset105').write_bytes(extended_iigs())
    for name, metrics in [('above-baseline', (8, -5, -2)), ('below-baseline', (-2, 5, 1))]:
        (args.directory / 'iigs' / name).write_bytes(shifted_iigs(*metrics))
    gap = bytearray(iigs())
    gap[49:49] = b'\xde\xad\xbe\xef'
    gap[33:35] = struct.pack('<H', 14)
    (args.directory / 'iigs' / 'table-gap').write_bytes(gap)
    for padding in [0, 30000]:
        (args.directory / 'iigs' / f'binscii-{padding}').write_bytes(binscii(iigs() + b'\0' * padding))


if __name__ == '__main__':
    main()
