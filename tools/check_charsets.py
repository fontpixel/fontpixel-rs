#!/usr/bin/env python3
"""Byte-for-byte Rust/Python parity for every SPEC charset and valid GL mode.

Unlike golden digests this compares the complete ordered binary mapping, with
no hashing. Rust scans the entire Unicode reverse domain (surrogates included),
all 65,536 short codes, and every 1,587,600 four-byte GB18030 candidate. Python
uses the supplied read-only bdfparser reference, not our generated Rust data.
The forward image is the complete possible reverse domain for SBCS/DBCS because
SPEC requires encode/decode round-trip. GB18030 checks all Unicode code points.
No bytecode, fixtures, or outputs are written in the reference checkout.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]


def pairs_bytes(pairs):
    output = bytearray()
    for a, b in pairs:
        if b is not None:
            output.extend(struct.pack('<II', a, b))
    return output


def compare(binary, key, gl, operation, expected):
    actual = subprocess.check_output([str(binary), key, str(gl).lower(), operation])
    if actual != expected:
        length = min(len(actual), len(expected))
        i = next((i for i in range(0, length, 8) if actual[i:i+8] != expected[i:i+8]), length)
        raise AssertionError(f'{key} gl={gl} {operation} mismatch at byte {i}; '
                             f'Rust={actual[i:i+8].hex()} Python={expected[i:i+8].hex()}; '
                             f'sizes {len(actual)}/{len(expected)}')
    return len(actual) // 8


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', type=Path, required=True, help='read-only bdfparser repository root')
    parser.add_argument('--no-cjk', action='store_true', help='verify core tables and missing-feature errors')
    args = parser.parse_args()
    sys.path.insert(0, str(args.reference.resolve() / 'src'))
    from bdfparser import encoding as ref
    manifest = json.loads((ROOT / 'tools/charsets/registry.json').read_text())
    actual_table = json.loads(json.dumps(ref.charset_table()))
    assert manifest['charsets'] == actual_table
    assert manifest['unicode_prefixes'] == list(ref.UNICODE_PREFIXES)
    cmd = ['cargo', 'build', '--locked', '--release', '-p', 'fontpixel', '--example', 'charset_probe', '--no-default-features']
    if not args.no_cjk:
        cmd += ['--features', 'cjk-charsets']
    subprocess.run(cmd, cwd=ROOT, check=True)
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / 'target'))
    if not target.is_absolute(): target = ROOT / target
    binary = target / 'release/examples/charset_probe'
    start = time.monotonic()
    variants, total, disabled = 0, 0, 0
    for key, info in actual_table.items():
        if args.no_cjk and info['kind'] in ('dbcs', 'gb18030'):
            result = subprocess.run([str(binary), key, 'false', 'decode'], capture_output=True)
            assert result.returncode != 0 and b'FeatureDisabled' in result.stderr, (key, result)
            disabled += 1
            continue
        if info['kind'] == 'unicode':
            base_forward = None
            candidates = range(0x110000)
        else:
            base_forward = ref.charset_map(key)
            candidates = sorted({ord(ch) for ch in base_forward.values()})
            if info['kind'] == 'gb18030': candidates = range(0x110000)
        modes = [False, True] if info['gl'] or info['forcegl'] else [False]
        for gl in modes:
            codec = ref.charset_codec(key, gl)
            expected = pairs_bytes((code, None if (ch := codec.cp2char(code)) is None else ord(ch)) for code in range(0x10000))
            total += compare(binary, key, gl, 'decode', expected)
            expected = pairs_bytes((value, codec.char2cp(chr(value))) for value in candidates)
            total += compare(binary, key, gl, 'encode', expected)
            variants += 1
        print(f'{key}: exact ({len(modes)} mode(s))', flush=True)
    if not args.no_cjk:
        codec = ref.charset_codec('gb18030')
        def gb4():
            for a in range(0x81, 0xff):
                for b in range(0x30, 0x3a):
                    for c in range(0x81, 0xff):
                        for d in range(0x30, 0x3a):
                            code = (a << 24) | (b << 16) | (c << 8) | d
                            ch = codec.cp2char(code)
                            yield code, ord(ch) if ch is not None else None
        total += compare(binary, 'gb18030', False, 'gb4', pairs_bytes(gb4()))
    print(f'PASS: {variants} charset/GL variants; {total:,} exact mapping pairs; '
          f'{disabled} explicit disabled-feature errors; {time.monotonic()-start:.2f}s')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
