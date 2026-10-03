#!/usr/bin/env python3
"""Final complete-family subset closure and native raster reader interoperability."""
import argparse
import gc
import hashlib
import json
import tempfile
from pathlib import Path

from fontTools.ttLib import TTFont
from fontTools.ttLib.woff2 import WOFF2GlyfTable
from variants import compare_subset
from raster_collections import raster_case


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--raster-driver', type=Path, required=True)
    p.add_argument('--native', type=Path, required=True)
    p.add_argument('--work', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    a.work.mkdir(parents=True, exist_ok=True)
    original = WOFF2GlyfTable._decodeComponents

    def components(self, glyph):
        self.compositeStream = memoryview(self.compositeStream)
        return original(self, glyph)

    WOFF2GlyfTable._decodeComponents = components
    report = {'partial': True, 'binary_sha256': sha(a.binary),
              'raster_driver_sha256': sha(a.raster_driver), 'subsets': [], 'rasters': []}

    def save():
        a.output.write_text(json.dumps(report, indent=2) + '\n')

    sfnts = sorted([*(a.native / 'ttf').glob('*.ttf'), *(a.native / 'otf').glob('*.otf')])
    assert len(sfnts) == 16
    for source in sfnts:
        cmap = sorted(TTFont(source).getBestCmap())
        cps = sorted(set(cmap[::max(1, len(cmap) // 24)]) |
                     (set(map(ord, 'A“”‘’…漢骨辻国语あ한')) & set(cmap)))
        with tempfile.TemporaryDirectory(prefix='subset-', dir=a.work) as temp:
            result = compare_subset(a.binary, source, Path(temp), source.stem, cps)
        result['source_sha256'] = sha(source)
        report['subsets'].append(result)
        save()
        print(source.name, 'subset closure and shaping PASS', flush=True)
        gc.collect()
    webfonts = sorted((a.native / 'woff2').glob('*.woff2'))
    assert len(webfonts) == 8
    for source in [*sfnts, *webfonts]:
        with tempfile.TemporaryDirectory(prefix='raster-', dir=a.work) as temp:
            result = raster_case(a.raster_driver, source, Path(temp), 14)
        report['rasters'].append(result)
        save()
        print(source.name, 'all-glyph native raster PASS', result['glyphs'], flush=True)
        gc.collect()
    report['partial'] = False
    save()


if __name__ == '__main__':
    main()
