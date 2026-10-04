#!/usr/bin/env python3
"""Build deterministic color reader/writer seeds without editing source fixtures."""
import argparse
import hashlib
import json
import struct
import subprocess
from pathlib import Path
from fontTools.ttLib import TTFont

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--output', type=Path, default=Path('fuzz/corpus/color_bitmap'))
    p.add_argument('--fixtures', type=Path, default=Path('reports/w5.6-color/fixtures'))
    a = p.parse_args()
    a.output.mkdir(parents=True, exist_ok=True)
    subprocess.run([str(a.binary), 'fixtures', str(a.fixtures)], check=True)
    seeds = []
    def save(name, data):
        (a.output / name).write_bytes(data)
        seeds.append(dict(name=name, bytes=len(data), sha256=hashlib.sha256(data).hexdigest()))
    for path in sorted(a.fixtures.glob('*.ttf')):
        if 'fonttools' in path.name or 'wide' in path.name:
            continue
        data = path.read_bytes()
        save(path.stem, data)
        f = TTFont(path)
        count = f['maxp'].numGlyphs
        if 'CBDT' in f:
            location, payload = f.getTableData('CBLC'), f.getTableData('CBDT')
            save(path.stem + '-tables', struct.pack('>HH', len(location), count - 1) + location + payload)
        else:
            save(path.stem + '-table', struct.pack('>HH', 0, count - 1) + f.getTableData('sbix'))
    for name, data in [('gray', bytes([12, 7, 255, 255, 0x43, 0, 1, 64, 128, 254, 255])),
                       ('zero-area', bytes([12, 7, 0, 0, 0x40])),
                       ('max-metrics', bytes([255, 255, 127, 128]) + bytes(range(16)))]:
        save('generated-' + name, data)
    (a.fixtures.parent / 'seeds.json').write_text(json.dumps(seeds, indent=2) + '\n')
    print(json.dumps({'seeds': len(seeds), 'bytes': sum(s['bytes'] for s in seeds)}))

if __name__ == '__main__':
    main()
