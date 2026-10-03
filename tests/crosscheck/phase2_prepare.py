#!/usr/bin/env python3
"""Recover immutable full Tong inputs from the user's existing source archive.

The original task's reference/ and samples/ trees remain read-only. This helper
does not download or alter glyphs; it rebuilds a separate, identified snapshot.
"""
import argparse
import hashlib
import importlib.util
import json
import subprocess
import sys
import zipfile
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[2]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    args = parser.parse_args()
    args.work.mkdir(parents=True, exist_ok=True)
    source = args.work / 'tong-baseline'
    source.mkdir(exist_ok=False)
    members = []
    with zipfile.ZipFile(args.archive) as archive:
        for entry in archive.infolist():
            name = entry.filename
            if entry.is_dir() or not (name.startswith(('glyphs/', 'build-data/'))
                                      or name in ['tools/build.py', 'OFL.txt']):
                continue
            path = PurePosixPath(name)
            assert not path.is_absolute() and '..' not in path.parts, name
            data = archive.read(entry)
            target = source / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            members.append({'path': name, 'bytes': len(data), 'sha256': sha(data)})
    builder = source / 'tools/build.py'
    original_builder = ROOT.parent / 'reference/tong-pixel-sans/tools/build.py'
    assert builder.read_bytes() == original_builder.read_bytes(), 'Snapshot builder differs from task reference'
    output = args.work / 'bdf'
    process = subprocess.run([sys.executable, '-B', str(builder), str(output)],
                             check=True, capture_output=True, text=True)
    (args.work / 'build.log').write_text(process.stdout + process.stderr)
    assert len(list(output.glob('*.bdf'))) == 20
    manifest = {'archive': args.archive.name, 'archive_sha256': sha(args.archive.read_bytes()),
                'builder_identical_to_task_reference': True, 'files': members,
                'bdf_files': [{'name': p.name, 'bytes': p.stat().st_size, 'sha256': sha(p.read_bytes())}
                              for p in sorted(output.glob('*.bdf'))]}
    (args.work / 'baseline-source-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print('Recovered and rebuilt', len(members), 'source files into 20 BDFs')


if __name__ == '__main__':
    main()
