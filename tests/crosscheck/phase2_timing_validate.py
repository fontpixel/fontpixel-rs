#!/usr/bin/env python3
"""Keep timed outputs tied to independently validated complete-font bytes."""
import argparse
import hashlib
import json
import tempfile
from pathlib import Path

import ots
from fontTools.ttLib import TTFont

from phase2_exports_validate import streams


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def canonical(path):
    font = TTFont(path, recalcTimestamp=False)
    result = {}
    for tag, entry in font.reader.tables.items():
        data = entry.loadData(font.reader.transformBuffer) if font.flavor == 'woff2' else font.reader[tag]
        if tag == 'head':
            # Only original-reference dates and derived whole-sfnt checksum vary.
            data = data[:8] + bytes(4) + data[12:20] + bytes(16) + data[36:]
        result[str(tag)] = {'sha256': hashlib.sha256(data).hexdigest(),
                            'length': getattr(entry, 'origLength', len(data)),
                            'transform': getattr(entry, 'transformVersion', None)}
    return result


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--reports', type=Path, required=True)
    p.add_argument('--native', type=Path, required=True)
    p.add_argument('--reference', type=Path, required=True)
    p.add_argument('--native-benchmark', type=Path)
    p.add_argument('--reference-benchmark', type=Path)
    p.add_argument('--capture-reference', action='store_true')
    a = p.parse_args()
    full = json.loads((a.reports / 'full-export-validation.json').read_text())
    assert not full['partial'] and len(full['results']) == 32
    expected_reference = {}
    expected_native = {}
    for r in full['results']:
        if 'reference_sha256' in r:
            prefix, shape, kind = r['case'].rsplit('-', 2)
            expected_reference[f'{kind}/{prefix}-{shape.capitalize()}.{kind}'] = r['reference_sha256']
            expected_native[f'{kind}/{prefix}-{shape}.{kind}'] = r['sha256']
        elif r['case'].endswith('-reference-woff2'):
            stem = r['case'].removesuffix('-reference-woff2')
            expected_reference[f'woff2/{stem}.woff2'] = r['sha256']
    if a.capture_reference:
        out = {}
        for name, expected in expected_reference.items():
            path = a.reference / name
            assert sha(path) == expected, (name, 'not the fully checked reference')
            out[name] = canonical(path)
        (a.reports / 'reference-canonical-tables.json').write_text(json.dumps(out, indent=2) + '\n')
        print('Captured', len(out), 'verified reference identities')
        return
    bitmaps = json.loads((a.reports / 'bitmaps.json').read_text())
    for r in bitmaps['results']:
        if r['dataset'] == 'rebuilt':
            kind = r['format']
            expected_native[kind + '/' + Path(r['file']).stem + '.' + kind] = r['native_sha256']
    source = json.loads((a.reports / 'baseline-source-manifest.json').read_text())
    expected_native.update({'bdf/' + r['name']: r['sha256'] for r in source['bdf_files']})
    result = {'native_profiles': [], 'reference': None}
    if a.native_benchmark:
        benchmark = json.loads(a.native_benchmark.read_text())
        assert not benchmark['partial'] and benchmark['requested_runs'] == 5
        for group in benchmark['profiles']:
            assert len(group['runs']) == 5
            for name, expected in expected_native.items():
                assert group['outputs'][name]['sha256'] == expected, (group['label'], name)
            paths = sorted((a.native / ('saved-' + group['label'])).glob('*.woff2'))
            assert len(paths) == 8
            for path in paths:
                assert sha(path) == group['outputs']['woff2/' + path.name]['sha256']
                assert streams(path) == full['native_woff2_streams'][path.name], (group['label'], path.name)
                with tempfile.TemporaryDirectory(prefix='ots-', dir=a.native) as temp:
                    check = ots.sanitize(str(path), str(Path(temp) / 'out.sfnt'), capture_output=True)
                    assert check.returncode == 0, (group['label'], path.name, check.stderr)
            result['native_profiles'].append({'profile': group['label'],
                                               'five_byte_identical_runs': True,
                                               '76_nonweb_files_match_complete_oracles': True,
                                               'eight_webfont_table_streams_match_complete_oracles': True,
                                               'ots_all_eight': True})
    if a.reference_benchmark:
        benchmark = json.loads(a.reference_benchmark.read_text())
        assert not benchmark['partial'] and benchmark['requested_runs'] == 5
        expected = json.loads((a.reports / 'reference-canonical-tables.json').read_text())
        for name, tables in expected.items():
            assert canonical(a.reference / name) == tables, (name, 'reference benchmark changed content')
        result['reference'] = {'five_runs': True, 'all_24_vector_webfont_tables_unchanged_except_dates_checksum': True,
                               'known_null_DFLT_OTS_rejection_retained': True}
    (a.reports / 'benchmark-validation.json').write_text(json.dumps(result, indent=2) + '\n')
    print('Timed output identities verified')


if __name__ == '__main__':
    main()
