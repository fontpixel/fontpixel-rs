#!/usr/bin/env python3
"""Independent native source-.notdef and metric review checks through the CLI."""
import argparse
import hashlib
import json
from pathlib import Path

import freetype
import ots
from fontTools.ttLib import TTFont
from raster_collections import call, ft_ink


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--work', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    a.work.mkdir(parents=True, exist_ok=True)
    source = a.work / 'source-notdef.bdf'
    source.write_text('''STARTFONT 2.2
FONT NativeReview
SIZE 8 75 75
FONTBOUNDINGBOX 2 2 -2 -1
METRICSSET 2
STARTPROPERTIES 7
PIXEL_SIZE 8
FONT_ASCENT 7
FONT_DESCENT 1
UNDERLINE_POSITION -2
UNDERLINE_THICKNESS 1
CHARSET_REGISTRY "ISO10646"
CHARSET_ENCODING "1"
ENDPROPERTIES
CHARS 3
STARTCHAR .notdef
ENCODING -1
SWIDTH 750 0
DWIDTH 6 0
SWIDTH1 0 -1000
DWIDTH1 0 -8
VVECTOR 3 5
BBX 2 2 -2 -1
BITMAP
80
40
ENDCHAR
STARTCHAR A
ENCODING 65
SWIDTH 750 0
DWIDTH 6 0
SWIDTH1 0 -1000
DWIDTH1 0 -8
VVECTOR 3 5
BBX 2 2 -2 -1
BITMAP
80
40
ENDCHAR
STARTCHAR space
ENCODING 32
SWIDTH 750 0
DWIDTH 6 0
SWIDTH1 0 -1000
DWIDTH1 0 -8
VVECTOR 3 5
BBX 0 0 0 0
BITMAP
ENDCHAR
ENDFONT
''')
    results = []
    for shape in ['square', 'square-separate', 'dot']:
        for kind in ['ttf', 'otf']:
            out = a.work / f'{shape}.{kind}'
            call(a.binary, 'convert', source, '-o', out, '--shape', shape, '--embed-bitmaps')
            f = TTFont(out)
            f.ensureDecompiled()
            assert f.getGlyphOrder()[:3] == ['.notdef', 'A', 'space']
            assert (f['post'].underlinePosition, f['post'].underlineThickness) == (-200, 100)
            inset = 5 if shape == 'dot' else 0
            face = freetype.Face(str(out))
            face.set_pixel_sizes(0, 8)
            for gid in range(3):
                name = f.getGlyphName(gid)
                expected = set() if gid == 2 else {(-2, 0), (-1, -1)}
                ink, advance = ft_ink(face, gid)
                assert ink == expected, (shape, kind, name, ink, expected)
                assert advance == 6
                assert f['hmtx'][name] == (600, 0 if gid == 2 else -200 + inset)
                assert f['vmtx'][name] == (800, 500 if gid == 2 else 400 + inset)
                bm = f['EBDT'].strikeData[0][name].metrics
                assert bm.vertAdvance == 8
                assert bm.horiAdvance == 6
                if gid != 2:
                    assert (bm.vertBearingX, bm.vertBearingY) == (-5, -4)
                face.load_glyph(gid, freetype.FT_LOAD_SBITS_ONLY | freetype.FT_LOAD_RENDER
                                | freetype.FT_LOAD_VERTICAL_LAYOUT)
                assert face.glyph.metrics.vertAdvance == 8 * 64
                assert face.glyph.metrics.horiAdvance == 6 * 64
            result = ots.sanitize(str(out), capture_output=True)
            assert result.returncode == 0, result.stderr
            results.append({'shape': shape, 'flavor': kind, 'glyphs': 3,
                            'fonttools_metrics': True, 'freetype_ink_and_advances': True,
                            'ots': True, 'sha256': hashlib.sha256(out.read_bytes()).hexdigest()})
    a.output.write_text(json.dumps({'partial': False, 'cases': results}, indent=2) + '\n')
    print('PASS: six native exports, source .notdef and blank metrics, 18 glyph checks')


if __name__ == '__main__':
    main()
