#!/usr/bin/env python3
"""Combined full Tong matrix on immutable supplied and rebuilt source inputs.

Every native sfnt is checked against the unchanged supplied Python exporter.
Recovered complete sources use its original, uncropped plan. Input BDFs are
never rewritten. Generated oracle files are temporary; compact evidence persists.
"""
import argparse
import gc
import hashlib
import importlib.metadata
import json
import subprocess
import tempfile
import time
from pathlib import Path

import freetype
import ots
from fontTools.ttLib import TTFont
from fontTools.ttLib.woff2 import WOFF2GlyfTable

from glyf import bitmap, load_reference, parse_bdf
from variants import (LANGUAGES, SCRIPTS, compare_full, hbfont, invoke, shape)
from woff import equivalent

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def common_args(ref, paths, size, spacing):
    family = ref.family_of(spacing, size)
    names = ref.names(family, 'Square', 'phase2')
    return ['variants', '--base', paths['SC'],
            '--lang', f'zh-Hant,zh-HK={paths["TC"]}',
            '--lang', f'ja={paths["JP"]}', '--lang', f'ko={paths["KR"]}',
            '--script', f'latn,grek,cyrl={paths["Latin"]}',
            '--family', family, '--vendor', 'TONG', '--font-version', 'phase2',
            '--copyright', names['copyright'], '--license', names['licenseDescription'],
            '--license-url', names['licenseInfoURL']]


def wrapper(binary, source, kind, tmp, label):
    web = tmp / (label + '.' + kind)
    decoded = tmp / (label + '.decoded.sfnt')
    sanitized = tmp / (label + '.ots.sfnt')
    subprocess.run([str(binary), 'encode', kind, str(source), str(web),
                    '9' if kind == 'woff' else '5', '1'],
                   check=True, capture_output=True, text=True)
    count = equivalent(source, web, label, True)
    run = ots.sanitize(str(web), str(sanitized), capture_output=True)
    assert run.returncode == 0, (label, 'OTS', run.stderr)
    left, right = freetype.Face(str(source)), freetype.Face(str(sanitized))
    for gid in range(count):
        assert bitmap(left, gid) == bitmap(right, gid), (label, gid, 'OTS pixels')
    subprocess.run([str(binary), 'decode', kind, str(web), str(decoded)],
                   check=True, capture_output=True, text=True)
    equivalent(source, decoded, label + '-native-read', False)
    if kind == 'woff':
        assert source.read_bytes() == decoded.read_bytes(), (label, 'lossless WOFF1')
    else:
        assert TTFont(web)['head'].flags & 0x800
    text = ''.join(chr(cp) for cp in TTFont(source).getBestCmap())
    a, b = hbfont(source.read_bytes()), hbfont(decoded.read_bytes())
    batches = 0
    for script in SCRIPTS:
        for lang in LANGUAGES:
            for start in range(0, len(text), 2048):
                part = text[start:start + 2048]
                assert shape(a, part, lang, script) == shape(b, part, lang, script)
                batches += 1
    return {'case': label, 'glyphs': count, 'bytes': web.stat().st_size,
            'sha256': sha(web), 'quality': 9 if kind == 'woff' else 5,
            'fonttools_all_tables_glyphs': True, 'ots_all_pixels': True,
            'native_read': True, 'shaping_batches': batches}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--woff-driver', type=Path, required=True)
    parser.add_argument('--bdf-dir', type=Path, required=True)
    parser.add_argument('--source-root', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--only', help='Optional exact configuration label')
    args = parser.parse_args()
    args.work.mkdir(parents=True, exist_ok=True)
    # Same independent decoder algorithm, avoiding quadratic whole-tail copies.
    original = WOFF2GlyfTable._decodeComponents

    def decode_components(self, glyph):
        self.compositeStream = memoryview(self.compositeStream)
        return original(self, glyph)

    WOFF2GlyfTable._decodeComponents = decode_components
    ref = load_reference(ROOT.parent)
    original_faces, original_latin = ref.build.faces, ref.build.latin_faces
    results = {'partial': True, 'binary_sha256': sha(args.binary),
               'woff_driver_sha256': sha(args.woff_driver),
               'tools': {n: importlib.metadata.version(n) for n in
                         ['fonttools', 'freetype-py', 'uharfbuzz', 'opentype-sanitizer']},
               'woff2_oracle_readonly_zero_copy': True, 'inputs': [], 'results': []}

    def save():
        args.output.write_text(json.dumps(results, indent=2) + '\n')

    configs = [('supplied-Large-prop', 'Large', 'prop',
                ROOT.parent / 'samples/tong-pixel-sans')]
    configs += [(f'rebuilt-{size}-{spacing}', size, spacing, args.bdf_dir)
                for size in ['Large', 'Small'] for spacing in ['prop', 'mono']]
    for config, size, spacing, directory in configs:
        if args.only and args.only != config:
            continue
        prefix = 'TongPixelSans' + ('Mono' if spacing == 'mono' else '') + '14' + size
        paths = {r: directory / (prefix + '-' + r + '.bdf')
                 for r in ['SC', 'TC', 'JP', 'KR', 'Latin']}
        results['inputs'].append({'configuration': config, 'files':
                                 [{'name': p.name, 'sha256': sha(p)} for p in paths.values()]})
        if config.startswith('supplied'):
            faces = {r: parse_bdf(p) for r, p in paths.items()}
            ref.build.faces = lambda root, size: [(r, faces[r], faces[r])
                                                   for r in ['SC', 'TC', 'JP', 'KR']]
            ref.build.latin_faces = lambda root, size: (faces['Latin'], faces['Latin'])
        else:
            ref.build.faces, ref.build.latin_faces = original_faces, original_latin
            ref.ROOT = args.source_root
        plan = ref.plan(spacing, size)
        ref.set_size(size)
        common = common_args(ref, paths, size, spacing)
        for shape_name in ['square', 'dot']:
            for kind, builder in [('ttf', ref.build_ttf), ('otf', ref.build_otf)]:
                label = config + '-' + shape_name + '-' + kind
                started = time.monotonic()
                native = args.work / (label + '.' + kind)
                invoke(args.binary, *common, '--shape', shape_name, '-o', native)
                with tempfile.TemporaryDirectory(prefix='oracle-', dir=args.work) as temp:
                    tmp = Path(temp)
                    reference = tmp / ('reference.' + kind)
                    font = builder(spacing, shape_name, 'phase2', plan, size)
                    font.recalcTimestamp = False
                    font.save(reference)
                    result = compare_full(native, reference, plan, shape_name, label)
                    result['seconds_including_validation'] = time.monotonic() - started
                    result['fontconfig'] = subprocess.check_output(
                        ['fc-query', '--format', '%{family}|%{style}|%{spacing}', str(native)],
                        text=True)
                    results['results'].append(result)
                    save()
                    print(label, 'SFNT PASS', result['glyphs'], flush=True)
                    del font
                    gc.collect()
                    for container in ['woff', 'woff2']:
                        result = wrapper(args.woff_driver, native, container, tmp,
                                         label + '-' + container)
                        results['results'].append(result)
                        save()
                        print(result['case'], 'PASS', result['glyphs'], flush=True)
                gc.collect()
        del plan
        gc.collect()
    results['partial'] = False
    save()
    print('ALL PASS', len(results['results']), flush=True)


if __name__ == '__main__':
    main()
