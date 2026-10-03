#!/usr/bin/env python3
"""Generate an independent-checked packed-axis CFF seed crossing stack-run bounds."""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

import ots
from fontTools.ttLib import TTFont


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--work', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    a = p.parse_args()
    a.work.mkdir(parents=True, exist_ok=True)
    width = 101
    rows = [[int(x % 2 == 0) for x in range(width)]] * 2 + [[1] * width]
    def row_hex(row):
        bits = ''.join(map(str, row)) + '0' * ((-len(row)) % 8)
        return int(bits, 2).to_bytes(len(bits) // 8, 'big').hex().upper()
    source = a.work / 'packed-axis.bdf'
    source.write_text('STARTFONT 2.1\nFONT PackedAxisSeed\nSIZE 14 75 75\n'
                      'FONTBOUNDINGBOX 101 3 0 0\nSTARTPROPERTIES 3\nPIXEL_SIZE 14\n'
                      'FONT_ASCENT 12\nFONT_DESCENT 2\nENDPROPERTIES\nCHARS 1\n'
                      'STARTCHAR A\nENCODING 65\nSWIDTH 7286 0\nDWIDTH 102 0\n'
                      'BBX 101 3 0 0\nBITMAP\n' + '\n'.join(map(row_hex, rows)) +
                      '\nENDCHAR\nENDFONT\n')
    sfnt = a.work / 'packed-axis.otf'
    subprocess.run([str(a.binary), 'variants', '--base', str(source), '-o', str(sfnt)],
                   check=True, capture_output=True)
    font = TTFont(sfnt)
    font.ensureDecompiled()
    top = font['CFF '].cff.topDictIndex[0]
    charstring = top.CharStrings[font.getBestCmap()[65]]
    charstring.decompile()
    count = 0
    runs = []
    for item in charstring.program:
        if isinstance(item, str):
            if item in ['hlineto', 'vlineto']:
                runs.append(count)
            count = 0
        else:
            count += 1
    assert max(runs) == 40 and runs.count(40) >= 2, runs
    check = ots.sanitize(str(sfnt), capture_output=True)
    assert check.returncode == 0, check.stderr
    raw = a.work / 'packed-axis.cff'
    raw.write_bytes(font.reader['CFF '])
    result = {'synthetic_source': True, 'fonttools_axis_operand_runs': runs,
              'ots': True, 'glyphs': font['maxp'].numGlyphs,
              'seeds': [{'name': f.name, 'bytes': f.stat().st_size,
                         'sha256': hashlib.sha256(f.read_bytes()).hexdigest()}
                        for f in [source, sfnt, raw]]}
    a.output.write_text(json.dumps(result, indent=2) + '\n')
    print(result)


if __name__ == '__main__':
    main()
