#!/usr/bin/env python3
"""Tie every timed optimized artifact to complete independent correctness gates."""
import argparse
import hashlib
import json
import tempfile
from pathlib import Path

import ots
from phase2_exports_validate import streams


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--baseline-reports', type=Path, required=True)
    p.add_argument('--full-report', type=Path, required=True)
    p.add_argument('--native', type=Path, required=True)
    p.add_argument('--benchmark', type=Path, action='append', required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    full = json.loads(a.full_report.read_text())
    assert not full['partial'] and len(full['results']) == 32
    expected = {}
    for r in full['results']:
        if 'reference_sha256' in r:
            prefix, shape, kind = r['case'].rsplit('-', 2)
            expected[f'{kind}/{prefix}-{shape}.{kind}'] = r['sha256']
    for r in json.loads((a.baseline_reports / 'bitmaps.json').read_text())['results']:
        if r['dataset'] == 'rebuilt':
            expected[r['format'] + '/' + Path(r['file']).stem + '.' + r['format']] = r['native_sha256']
    for r in json.loads((a.baseline_reports / 'baseline-source-manifest.json').read_text())['bdf_files']:
        expected['bdf/' + r['name']] = r['sha256']
    assert len(expected) == 76
    results = []
    for path in a.benchmark:
        bench = json.loads(path.read_text())
        assert not bench['partial'] and bench['requested_runs'] == 5 and len(bench['runs']) == 5
        assert bench['all_outputs_byte_identical'] and bench['binary_unchanged']
        for name, digest in expected.items():
            assert bench['outputs'][name]['sha256'] == digest, (path.name, name)
        saved = a.native / ('saved-' + path.stem)
        webfonts = sorted(saved.glob('*.woff2'))
        assert len(webfonts) == 8, saved
        for web in webfonts:
            assert sha(web) == bench['outputs']['woff2/' + web.name]['sha256']
            assert streams(web) == full['native_woff2_streams'][web.name], (path.name, web.name)
            with tempfile.TemporaryDirectory(prefix='ots-', dir=a.native) as temp:
                check = ots.sanitize(str(web), str(Path(temp) / 'font.sfnt'), capture_output=True)
                assert check.returncode == 0, (path.name, web.name, check.stderr)
        results.append({'benchmark': path.name, 'five_identical_runs': True,
                        '76_nonweb_files_match_complete_oracles': True,
                        '8_independently_decoded_webfont_streams_match_complete_oracles': True,
                        'ots_all_8': True, 'quality': bench['quality'],
                        'jobs': bench['jobs'], 'compression_threads': bench['compression_threads'],
                        'dot_reuse': bench['dot_reuse']})
    a.output.write_text(json.dumps({'results': results}, indent=2) + '\n')
    print('PASS: every timed artifact is tied to the complete independent oracle')


if __name__ == '__main__':
    main()
