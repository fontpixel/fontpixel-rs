#!/usr/bin/env python3
"""Every-glyph PCF/OTB checks for both complete Tong dimensions and supplied BDFs.

The untouched original Tong Rust executable independently writes reference OTBs;
bdftopcf writes independent PCFs. All three faces (BDF/native/reference) must agree.
"""
import argparse
import hashlib
import importlib.metadata
import json
import subprocess
import tempfile
from pathlib import Path

import freetype
from fontTools.ttLib import TTFont

from glyf import bitmap, parse_bdf
from variants import invoke

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check(source, native, reference, kind):
    records = parse_bdf(source)
    expected = {cp for cp in records if kind == 'otb' or cp <= 0xffff}
    faces = [freetype.Face(str(path)) for path in [source, native, reference]]
    coverage = [{cp for cp, gid in f.get_chars() if gid} for f in faces]
    assert coverage[0] == set(records), (source.name, 'source coverage')
    assert coverage[1] == coverage[2] == expected, (source.name, kind, 'coverage')
    for cp in sorted(expected):
        images = [bitmap(face, face.get_char_index(cp)) for face in faces]
        assert images[0] == images[1] == images[2], (source.name, kind, cp, images)
    if kind == 'otb':
        with TTFont(native) as font:
            font.ensureDecompiled()
            assert set(font.getBestCmap()) == expected
            assert font['head'].unitsPerEm == 1400
            # Exercise the full TTX writer without retaining large text files.
            with tempfile.TemporaryFile(mode='w+b') as out:
                font.saveXML(out)
    query = subprocess.check_output(
        ['fc-query', '--format', '%{family}|%{style}|%{pixelsize}|%{spacing}', str(native)],
        text=True)
    family = next(line.partition(' ')[2].strip('"') for line in source.read_text().splitlines()
                  if line.startswith('FAMILY_NAME '))
    reference_query = subprocess.check_output(
        ['fc-query', '--format', '%{family}|%{style}|%{pixelsize}|%{spacing}', str(reference)],
        text=True)
    if kind == 'pcf':
        # Fontconfig prepends the X11 FOUNDRY for these PCFs, equally for bdftopcf.
        assert query == reference_query, (source.name, kind, 'fontconfig reference', query, reference_query)
        assert query.split('|')[0].endswith(family)
    else:
        assert query.split('|')[0] == family, (source.name, kind, 'fontconfig family', query)
    assert query.split('|')[2] == '14', (source.name, kind, 'fontconfig size', query)
    if 'Mono' in source.name and kind == 'otb':
        assert query.split('|')[3] in ['90', '100'], (source.name, kind, 'mono spacing', query)
    return {'file': source.name, 'format': kind, 'glyphs': len(expected),
            'source_glyphs': len(records), 'supplementary_not_representable': len(records) - len(expected),
            'source_sha256': sha(source), 'native_sha256': sha(native),
            'reference_sha256': sha(reference), 'bytes': native.stat().st_size,
            'pixels_bearings_advances': True, 'fontconfig': query,
            'reference_fontconfig': reference_query,
            'fonttools_full_ttx': kind == 'otb'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--reference-binary', type=Path, required=True)
    parser.add_argument('--bdf-dir', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.work.mkdir(parents=True, exist_ok=True)
    result = {'partial': True, 'binary_sha256': sha(args.binary),
              'reference_binary_sha256': sha(args.reference_binary),
              'tools': {n: importlib.metadata.version(n) for n in ['fonttools', 'freetype-py']},
              'results': []}
    for dataset in ['rebuilt', 'supplied']:
        stage = args.work / (dataset + '-inputs')
        stage.mkdir(exist_ok=True)
        supplied = ROOT.parent / 'samples/tong-pixel-sans'
        for source in sorted(args.bdf_dir.glob('*.bdf')):
            actual = supplied / source.name if dataset == 'supplied' and (supplied / source.name).is_file() else source
            target = stage / source.name
            if not target.exists():
                target.symlink_to(actual.resolve())
        reference_dir = args.work / (dataset + '-reference')
        process = subprocess.run([str(args.reference_binary), str(reference_dir), '--only', 'otb',
                                  '--bdf-dir', str(stage), '--font-version', 'phase2'],
                                 capture_output=True, text=True)
        (args.work / (dataset + '-reference.log')).write_text(process.stdout + process.stderr)
        assert process.returncode == 0, (dataset, process.stdout, process.stderr)
        sources = sorted(stage.glob('*.bdf')) if dataset == 'rebuilt' else sorted(supplied.glob('*.bdf'))
        for source in sources:
            with tempfile.TemporaryDirectory(prefix='bitmap-', dir=args.work) as temp:
                tmp = Path(temp)
                for kind in ['pcf', 'otb']:
                    native = tmp / ('native.' + kind)
                    invoke(args.binary, 'convert', source, '-o', native, '--to', kind)
                    if kind == 'pcf':
                        reference = tmp / 'reference.pcf'
                        subprocess.run(['bdftopcf', '-o', str(reference), str(source)],
                                       check=True, capture_output=True, text=True)
                    else:
                        reference = reference_dir / 'otb' / (source.stem + '.otb')
                    entry = check(source, native, reference, kind)
                    entry['dataset'] = dataset
                    result['results'].append(entry)
                    args.output.write_text(json.dumps(result, indent=2) + '\n')
                    print(dataset, source.name, kind, 'PASS', entry['glyphs'], flush=True)
    result['partial'] = False
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print('ALL PASS', len(result['results']), flush=True)


if __name__ == '__main__':
    main()
