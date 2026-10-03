#!/usr/bin/env python3
"""Tie all seven supplied PCF/OTB cases to their accepted external pixel oracles."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--baseline', type=Path, required=True)
    p.add_argument('--work', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    a.work.mkdir(parents=True, exist_ok=True)
    sources = Path(__file__).resolve().parents[3] / 'samples/tong-pixel-sans'
    baseline = json.loads(a.baseline.read_text())
    assert not baseline['partial']
    result = {'binary_sha256': sha(a.binary), 'results': []}
    for row in baseline['results']:
        if row['dataset'] != 'supplied':
            continue
        source = sources / row['file']
        assert sha(source) == row['source_sha256']
        with tempfile.TemporaryDirectory(prefix='bitmap-', dir=a.work) as temp:
            output = Path(temp) / ('font.' + row['format'])
            subprocess.run([str(a.binary), 'convert', str(source), '-o', str(output)],
                           check=True, capture_output=True)
            assert sha(output) == row['native_sha256'], (source.name, row['format'])
        result['results'].append({'source': source.name, 'format': row['format'],
                                  'sha256': row['native_sha256'], 'glyphs': row['glyphs'],
                                  'byte_identical_to_full_external_pixel_oracle': True})
        print(source.name, row['format'], 'exact bytes PASS', flush=True)
    assert len(result['results']) == 14
    a.output.write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
