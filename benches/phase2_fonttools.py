#!/usr/bin/env python3
"""Paired final-code, five-run vectorization and WOFF2 comparisons with fontTools.

Native and Python vectorization both read the complete supplied SC BDF, plan
the same single-face export, construct metadata/outlines and write the TTF.
Compression measurements exclude reads/writes equally for both implementations.
"""
import argparse
import hashlib
import importlib.metadata
import io
import json
import statistics
import subprocess
import sys
import time
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.ttLib.woff2 import WOFF2FlavorData

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tests/crosscheck'))
from glyf import load_reference, parse_bdf
from variants import compare_full, invoke
from woff import equivalent


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def single_plan(ref, records):
    ref.build.faces = lambda root, size: [('SC', records, records), ('TC', {}, {}),
                                         ('JP', {}, {}), ('KR', {}, {})]
    ref.build.latin_faces = lambda root, size: ({}, {})
    return ref.plan('prop', 'Large')


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--woff-driver', type=Path, required=True)
    p.add_argument('--validated-square-cff', type=Path, required=True)
    p.add_argument('--work', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    a.work.mkdir(parents=True, exist_ok=True)
    ref = load_reference(ROOT.parent)
    sample = ROOT.parent / 'samples/tong-pixel-sans'
    source = sample / 'TongPixelSans14Large-SC.bdf'
    ref.set_size('Large')
    names = ref.names(ref.family_of('prop', 'Large'), 'Square', 'phase2')
    common = ['variants', '--base', source, '--family', names['familyName'],
              '--vendor', 'TONG', '--font-version', 'phase2',
              '--copyright', names['copyright'], '--license', names['licenseDescription'],
              '--license-url', names['licenseInfoURL'], '--shape', 'square']
    result = {'partial': True, 'binary_sha256': sha(a.binary),
              'woff_driver_sha256': sha(a.woff_driver),
              'fonttools_version': importlib.metadata.version('fonttools'),
              'brotli_version': importlib.metadata.version('brotli'), 'results': []}

    def save():
        a.output.write_text(json.dumps(result, indent=2) + '\n')

    for engine in ['native', 'fonttools']:
        out = a.work / ('sc-square-' + engine + '.ttf')
        seconds, hashes = [], []
        for repetition in range(5):
            start = time.perf_counter()
            if engine == 'native':
                invoke(a.binary, *common, '-o', out)
            else:
                plan = single_plan(ref, parse_bdf(source))
                font = ref.build_ttf('prop', 'square', 'phase2', plan, 'Large')
                font['head'].created = font['head'].modified = 2082844800
                font.recalcTimestamp = False
                font.save(out)
                del font, plan
            seconds.append(time.perf_counter() - start)
            hashes.append(sha(out))
            print('vector', engine, repetition + 1, round(seconds[-1], 3), flush=True)
        assert len(set(hashes)) == 1
        result['results'].append({'case': 'full-SC-square-ttf', 'engine': engine,
                                  'seconds': seconds, 'median_seconds': statistics.median(seconds),
                                  'bytes': out.stat().st_size, 'sha256': hashes[0],
                                  'input_sha256': sha(source), 'includes': 'read/plan/encode/write'})
        save()
    plan = single_plan(ref, parse_bdf(source))
    result['vector_correctness'] = compare_full(a.work / 'sc-square-native.ttf',
                                                a.work / 'sc-square-fonttools.ttf', plan,
                                                'square', 'paired-full-SC-square')
    del plan
    save()
    records = parse_bdf(sample / 'TongPixelSans14Large-Latin.bdf')
    plan = single_plan(ref, records)
    latin = a.work / 'latin-dot-source.ttf'
    font = ref.build_ttf('prop', 'dot', 'phase2', plan, 'Large')
    font['head'].created = font['head'].modified = 2082844800
    font.recalcTimestamp = False
    font.save(latin)
    del font, plan, records
    for source in [a.validated_square_cff, latin]:
        raw = source.read_bytes()
        for engine in ['native', 'fonttools']:
            out = a.work / (source.stem + '-' + engine + '.woff2')
            if engine == 'native':
                data = json.loads(subprocess.check_output(
                    [str(a.woff_driver), 'bench', 'woff2', str(source), str(out), '11', '1'],
                    text=True, stderr=subprocess.DEVNULL))
                seconds = data['seconds']
            else:
                seconds = []
                for repetition in range(5):
                    start = time.perf_counter()
                    font = TTFont(io.BytesIO(raw), recalcTimestamp=False)
                    font.flavor = 'woff2'
                    font.flavorData = WOFF2FlavorData(transformedTables={'glyf', 'loca', 'hmtx'})
                    buffer = io.BytesIO()
                    font.save(buffer)
                    encoded = buffer.getvalue()
                    seconds.append(time.perf_counter() - start)
                    del font
                out.write_bytes(encoded)
            assert len(seconds) == 5
            entry = {'case': 'woff2-' + source.stem, 'engine': engine,
                     'seconds': seconds, 'median_seconds': statistics.median(seconds),
                     'bytes': out.stat().st_size, 'sha256': sha(out), 'input_sha256': sha(source),
                     'quality': 11, 'threads': 1, 'includes': 'encode only'}
            entry['all_glyphs_verified'] = equivalent(source, out, entry['case'], True)
            decoded = out.with_suffix('.sfnt')
            subprocess.run([str(a.woff_driver), 'decode', 'woff2', str(out), str(decoded)],
                           check=True, capture_output=True)
            equivalent(source, decoded, entry['case'] + '-native-read', True)
            result['results'].append(entry)
            save()
            print(entry['case'], engine, round(entry['median_seconds'], 3), flush=True)
    result['partial'] = False
    save()


if __name__ == '__main__':
    main()
