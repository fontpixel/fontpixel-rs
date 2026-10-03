#!/usr/bin/env python3
"""Exercise Tong exporter path safety using disposable 20-face fixtures only."""
import argparse
import hashlib
import json
import os
import subprocess
import tempfile
from pathlib import Path

BDF = b'''STARTFONT 2.1
FONT review
SIZE 14 75 75
FONTBOUNDINGBOX 1 1 0 0
STARTPROPERTIES 5
FAMILY_NAME "Review"
PIXEL_SIZE 14
FONT_ASCENT 14
FONT_DESCENT 0
CHARSET_REGISTRY "ISO10646"
ENDPROPERTIES
CHARS 1
STARTCHAR A
ENCODING 65
SWIDTH 71 0
DWIDTH 1 0
BBX 1 1 0 0
BITMAP
80
ENDCHAR
ENDFONT
'''


def files(root):
    return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(root.rglob('*')) if p.is_file()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    results = []
    cases = ['identical', 'cross-input-hardlink', 'cross-format-hardlink',
             'output-hardlinks', 'ordinary-rerun']
    if os.name == 'posix':
        cases += ['file-symlink', 'directory-symlink', 'dangling-symlink']
    for case in cases:
        with tempfile.TemporaryDirectory(prefix='fontpixel-export-paths-') as temp:
            root = Path(temp)
            source, out = root / 'bdf', root / 'out'
            source.mkdir()
            for size in ['Large', 'Small']:
                for spacing in ['', 'Mono']:
                    for region in ['SC', 'TC', 'JP', 'KR', 'Latin']:
                        (source / f'TongPixelSans{spacing}14{size}-{region}.bdf').write_bytes(BDF)
            (out / 'bdf').mkdir(parents=True)
            (out / 'pcf').mkdir()
            (out / 'otf').mkdir()
            sc, jp = [source / f'TongPixelSans14Large-{r}.bdf' for r in ['SC', 'JP']]
            if case == 'identical':
                out = root
            elif case == 'cross-input-hardlink':
                os.link(jp, out / 'bdf' / sc.name)
            elif case == 'cross-format-hardlink':
                os.link(jp, out / 'otf/TongPixelSans14Large-dot.otf')
            elif case == 'output-hardlinks':
                first = out / 'bdf' / sc.name
                first.write_bytes(b'old output')
                os.link(first, out / 'pcf' / sc.with_suffix('.pcf').name)
            elif case == 'file-symlink':
                (out / 'pcf' / sc.with_suffix('.pcf').name).symlink_to(jp)
            elif case == 'directory-symlink':
                (out / 'bdf').rmdir()
                (out / 'bdf').symlink_to(source, target_is_directory=True)
            elif case == 'dangling-symlink':
                (out / 'pcf' / sc.with_suffix('.pcf').name).symlink_to(out / 'bdf' / sc.name)
            before = files(root)
            command = [str(args.binary.resolve()), str(source), str(out), '1', '0', '1']
            run = subprocess.run(command, capture_output=True, text=True)
            assert all(p.read_bytes() == BDF for p in source.glob('*.bdf')), (case, 'source modified')
            if case == 'ordinary-rerun':
                assert run.returncode == 0, (case, run.stderr)
                first = files(out)
                assert len(first) == 84, len(first)
                again = subprocess.run(command, capture_output=True, text=True)
                assert again.returncode == 0, (case, again.stderr)
                assert files(out) == first, 'rerun bytes changed'
                assert all(p.read_bytes() == BDF for p in source.glob('*.bdf'))
            else:
                assert run.returncode != 0, (case, 'unsafe export succeeded')
                assert 'aliases' in run.stderr or 'unresolved symlink' in run.stderr, (case, run.stderr)
                assert files(root) == before, (case, 'wrote files before rejection')
            results.append({'case': case, 'sources_unchanged': 20,
                            'expected_exit': run.returncode,
                            'no_output_mutation_on_rejection': case != 'ordinary-rerun',
                            'identical_84_output_rerun': case == 'ordinary-rerun'})
            print(case, 'PASS', flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps({'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
                                       'partial': False, 'results': results}, indent=2) + '\n')


if __name__ == '__main__':
    main()
