#!/usr/bin/env python3
"""Validate complete CLI exports against the unchanged original Tong Rust exporter.

All sixteen sfnts use full glyph/metric/cmap/name/FreeType/OTS/locl checks.
Every native and original-reference WOFF2 is independently decoded, rasterized,
and read through the final native reader. No timing claims are made here.
"""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

import freetype
import ots
from fontTools.ttLib import TTFont
from fontTools.ttLib.woff2 import WOFF2GlyfTable

from glyf import bitmap, load_reference
from variants import compare_full
from woff import equivalent

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def streams(path):
    font = TTFont(path)
    return {str(tag): {'length': entry.origLength, 'transform': entry.transformVersion,
                      'sha256': hashlib.sha256(entry.loadData(font.reader.transformBuffer)).hexdigest()}
            for tag, entry in font.reader.tables.items()}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--native', type=Path, required=True)
    p.add_argument('--reference', type=Path, required=True)
    p.add_argument('--source-root', type=Path, required=True)
    p.add_argument('--woff-driver', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    original = WOFF2GlyfTable._decodeComponents

    def components(self, glyph):
        self.compositeStream = memoryview(self.compositeStream)
        return original(self, glyph)

    WOFF2GlyfTable._decodeComponents = components
    ref = load_reference(ROOT.parent)
    ref.ROOT = a.source_root
    report = {'partial': True, 'results': [], 'native_woff2_streams': {}}

    def save():
        a.output.write_text(json.dumps(report, indent=2) + '\n')

    for size in ['Large', 'Small']:
        for spacing in ['prop', 'mono']:
            ref.set_size(size)
            plan = ref.plan(spacing, size)
            prefix = 'TongPixelSans' + ('Mono' if spacing == 'mono' else '') + '14' + size
            for shape in ['square', 'dot']:
                for kind in ['ttf', 'otf']:
                    native = a.native / kind / (prefix + '-' + shape + '.' + kind)
                    reference = a.reference / kind / (prefix + '-' + shape.capitalize() + '.' + kind)
                    label = prefix + '-' + shape + '-' + kind
                    result = compare_full(native, reference, plan, shape, label)
                    result['reference_sha256'] = sha(reference)
                    result['reference_engine'] = 'original Tong Rust exporter'
                    report['results'].append(result)
                    save()
                    print(label, 'ORIGINAL RUST PASS', result['glyphs'], flush=True)
                kind = 'otf' if shape == 'square' else 'ttf'
                for engine, directory, suffix in [('native', a.native, shape),
                                                   ('reference', a.reference, shape.capitalize())]:
                    base = prefix + '-' + suffix
                    source = directory / kind / (base + '.' + kind)
                    web = directory / 'woff2' / (base + '.woff2')
                    label = base + '-' + engine + '-woff2'
                    count = equivalent(source, web, label, True)
                    with tempfile.TemporaryDirectory(prefix='verify-', dir=a.native) as temp:
                        decoded, sanitized = Path(temp) / 'decoded.sfnt', Path(temp) / 'ots.sfnt'
                        run = ots.sanitize(str(web), str(sanitized), capture_output=True)
                        assert run.returncode == 0, (label, run.stderr)
                        left, right = freetype.Face(str(source)), freetype.Face(str(sanitized))
                        for gid in range(count):
                            assert bitmap(left, gid) == bitmap(right, gid), (label, gid, 'OTS pixels')
                        subprocess.run([str(a.woff_driver), 'decode', 'woff2', str(web), str(decoded)],
                                       check=True, capture_output=True)
                        equivalent(source, decoded, label + '-native-decode', False)
                    if engine == 'native':
                        report['native_woff2_streams'][web.name] = streams(web)
                    report['results'].append({'case': label, 'glyphs': count,
                                              'bytes': web.stat().st_size, 'sha256': sha(web),
                                              'independent_full_glyphs': True, 'ots_full_pixels': True,
                                              'native_reader_full_tables': True})
                    save()
                    print(label, 'PASS', count, flush=True)
    report['partial'] = False
    save()


if __name__ == '__main__':
    main()
