#!/usr/bin/env python3
"""Full-family WOFF1 validation by independent, exact sfnt table identities.

Every underlying sfnt has an every-glyph external oracle. Exact independent
table bytes plus exact native sfnt reconstruction preserve those established
pixels and shaping without reloading multiple expanded outline models.
"""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

import ots
from fontTools.ttLib import TTFont


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def tables_equal(left, right, same_checksum=True):
    a, b = TTFont(left, recalcTimestamp=False), TTFont(right, recalcTimestamp=False)
    assert set(a.reader.tables) == set(b.reader.tables)
    for tag in a.reader.tables:
        x, y = a.reader[tag], b.reader[tag]
        if tag == 'head' and not same_checksum:
            x, y = x[:8] + bytes(4) + x[12:], y[:8] + bytes(4) + y[12:]
        assert x == y, (left.name, right.name, tag)
    return len(a.reader.tables)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--native-exports', type=Path, required=True)
    p.add_argument('--woff-driver', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    paths = sorted([*(a.native_exports / 'ttf').glob('*.ttf'),
                    *(a.native_exports / 'otf').glob('*.otf')])
    assert len(paths) == 16
    result = {'partial': True, 'driver_sha256': sha(a.woff_driver), 'results': []}
    for source in paths:
        with tempfile.TemporaryDirectory(prefix='woff1-', dir=a.native_exports) as temp:
            temp = Path(temp)
            web, decoded = temp / 'native.woff', temp / 'native.sfnt'
            subprocess.run([str(a.woff_driver), 'encode', 'woff', str(source), str(web), '9', '1'],
                           check=True, capture_output=True)
            table_count = tables_equal(source, web)
            subprocess.run([str(a.woff_driver), 'decode', 'woff', str(web), str(decoded)],
                           check=True, capture_output=True)
            assert source.read_bytes() == decoded.read_bytes()
            sanitized = temp / 'native.ots'
            check = ots.sanitize(str(web), str(sanitized), capture_output=True)
            assert check.returncode == 0, (source.name, check.stderr)
            foreign, foreign_decoded = temp / 'fonttools.woff', temp / 'foreign.sfnt'
            font = TTFont(source, recalcTimestamp=False, recalcBBoxes=False)
            font.flavor = 'woff'
            font.save(foreign)
            tables_equal(source, foreign, same_checksum=False)
            subprocess.run([str(a.woff_driver), 'decode', 'woff', str(foreign), str(foreign_decoded)],
                           check=True, capture_output=True)
            tables_equal(source, foreign_decoded, same_checksum=False)
            check = ots.sanitize(str(foreign_decoded), str(sanitized), capture_output=True)
            assert check.returncode == 0, (source.name, 'foreign native decode', check.stderr)
            result['results'].append({'source': source.name, 'source_sha256': sha(source),
                                      'native_woff_sha256': sha(web), 'fonttools_woff_sha256': sha(foreign),
                                      'tables': table_count, 'native_sfnt_bytes_exact': True,
                                      'fonttools_table_bytes_exact': True,
                                      'foreign_read_table_bytes_exact_except_checksum_adjustment': True,
                                      'ots_native_and_foreign_reconstruction': True,
                                      'pixels_shaping': 'inherited from identical validated sfnt tables'})
            a.output.write_text(json.dumps(result, indent=2) + '\n')
            print(source.name, 'PASS', flush=True)
    result['partial'] = False
    a.output.write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
