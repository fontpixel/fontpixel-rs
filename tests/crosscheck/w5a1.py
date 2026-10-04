#!/usr/bin/env python3
"""Independent monobit 0.54.0 ↔ native W5.A1 tests; original MIT rasters only.
Run after scripts/local-env.sh and building example w5a1_timing with its 8 features.
The installed independent oracle is never modified. No font source files are vendored.
"""
import argparse
import hashlib
import importlib.metadata
import io
from fractions import Fraction
import json
import os
from pathlib import Path
import struct
import subprocess
import time

import monobit
from monobit import Font, Glyph
import monobit.storage.fontformats

ROOT = Path(__file__).resolve().parents[2]
IDS = ('dosstart', 'edwin', 'mkwinfont', 'psf2txt', 'pcr', 'plus3dos', 'drhalo', 'writeon')
SOURCES = dict(zip(IDS, ('text/dosstart.py', 'text/edwin.py', 'text/fd.py', 'text/psf2txt.py', 'raw/optiks.py', 'raw/plus3dos.py', 'raw/drhalo.py', 'raw/writeon.py')))


def hash_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def original_font(codes, width=8, height=14, ascent=None):
    glyphs = []
    for cp in codes:
        rows = tuple(''.join('1' if (x * 3 + y * 5 + cp) % 7 < 3 else '0' for x in range(width)) for y in range(height))
        glyphs.append(Glyph(rows, inklevels='01', codepoint=cp, shift_up=0))
    return Font(tuple(glyphs), ascent=height if ascent is None else ascent, descent=0, name='Original W5A1', encoding='ascii')


def snapshot(font, ident):
    glyphs = []
    for i, glyph in enumerate(font.glyphs):
        chars = [[ord(c) for c in value] for value in glyph.chars]
        unicode = [v[0] for v in chars if len(v) == 1]
        sequences = [v for v in chars if len(v) > 1]
        codes = [int(value) for value in glyph.codepoints]
        if ident == 'psf2txt':
            codes = [] if unicode or sequences else [i]
        glyphs.append(dict(name=glyph.tags[0].value if ident == 'edwin' and glyph.tags else None, legacy=codes, unicode=unicode if ident == 'psf2txt' else [], sequences=sequences if ident == 'psf2txt' else [], width=glyph.width, height=glyph.height, x=glyph.left_bearing, y=glyph.shift_up, advance=glyph.advance_width, bytes=list(glyph.as_bytes())))
    point = font.get_properties().get('point_size') if ident == 'mkwinfont' else None
    point = Fraction(point) if point is not None else None
    return [dict(point_size=[point.numerator, point.denominator] if point is not None else None, glyphs=glyphs, ascent=font.ascent, descent=font.descent, family=str.__str__(font.name) if ident in ('dosstart', 'mkwinfont') else None)]


def original_drhalo(first=32):
    font = original_font(range(first, first+4), width=9, height=3)
    header = bytearray(20)
    header[:2] = b'AH'
    struct.pack_into('<HBBHHHHHHH', header, 2, 0xC8 if first == 0 else 0xDF, 0x0A, 3, 0x100 if first == 0 else 0x300, 0, 127, 6, 1 if first == 0 else 2, 9, 3)
    return bytes(header) + b''.join(g.as_bytes() for g in font.glyphs)


def emit(directory):
    directory.mkdir(parents=True, exist_ok=True)
    cases = []
    for ident in IDS:
        source = directory / f'{ident}.oracle'
        if ident == 'drhalo':
            source.write_bytes(original_drhalo())
        else:
            codes = range(256) if ident == 'pcr' else range(32, 128) if ident in ('dosstart', 'plus3dos', 'writeon') else range(65, 69)
            font = original_font(codes, height=8 if ident == 'plus3dos' else 14)
            monobit.save(font, source, format=ident, overwrite=True)
        cases.append((ident, source, 'spec-authored' if ident == 'drhalo' else 'independent-writer'))
    alternate = directory / 'drhalo-first0.oracle'
    alternate.write_bytes(original_drhalo(0))
    cases.append(('drhalo', alternate, 'spec-authored first0'))
    alternate = directory / 'pcr-v1.oracle'
    b = bytearray((directory / 'pcr.oracle').read_bytes()); b[4] = 1
    alternate.write_bytes(b)
    cases.append(('pcr', alternate, 'independent-writer with version1'))
    alternate = directory / 'edwin-wide.oracle'
    monobit.save(original_font(range(65, 69), width=129, height=4), alternate, format='edwin', overwrite=True)
    cases.append(('edwin', alternate, 'independent-writer width129'))
    alternate = directory / 'psf2txt-alias-sequence.oracle'
    alternate.write_text('%PSF2\nVersion: 0\nFlags: 1\nLength: 2\nWidth: 9\nHeight: 2\n%\nBitmap: #-#-#-#-# \\\n        -#-#-#-#-\nUnicode: [0041];[0391];[0061+0301];\n%\nBitmap: ###---### \\\n        ---###---\n')
    cases.append(('psf2txt', alternate, 'spec-authored aliases, sequence, unlabeled slot'))
    alternate = directory / 'mkwinfont-utf8.oracle'
    monobit.save(original_font(range(65, 69)).modify(name='字型 Café'), alternate, format='mkwinfont', overwrite=True)
    cases.append(('mkwinfont', alternate, 'independent-writer UTF-8 family name'))
    alternate = directory / 'mkwinfont-pointsize.oracle'
    monobit.save(original_font(range(65, 69)).modify(point_size=9), alternate, format='mkwinfont', overwrite=True)
    cases.append(('mkwinfont', alternate, 'independent-writer explicit point size'))
    alternate = directory / 'edwin-label-baseline.oracle'
    alternate.write_text('65 66\n字形: 2 3 5 6\nomega: 3 3 1 2 4\n')
    cases.append(('edwin', alternate, 'spec-authored UTF-8 labels and variable-height baseline'))
    return cases


def main():
    parser = argparse.ArgumentParser()
    work = Path(os.environ.get('FONTPIXEL_WORK_ROOT', ROOT.parent / 'local-work'))
    parser.add_argument('--bench', choices=IDS)
    parser.add_argument('--file', type=Path)
    parser.add_argument('--count', type=int, default=100)
    parser.add_argument('--adapter', type=Path, default=work / 'target-w5a1/debug/examples/w5a1_timing')
    parser.add_argument('--work', type=Path, default=work / 'w5a1-crosscheck')
    parser.add_argument('--report', type=Path, default=ROOT / 'reports/w5.a1-crosscheck.json')
    args = parser.parse_args()
    if args.bench:
        if args.file is None or args.count < 1:
            parser.error('--bench requires --file and positive --count')
        b = args.file.read_bytes()
        started = time.perf_counter_ns()
        for _ in range(args.count):
            monobit.load(io.BufferedReader(io.BytesIO(b)), format=args.bench)
        read_ns = time.perf_counter_ns()-started
        write_ns = None
        if args.bench != 'drhalo':
            font = monobit.load(io.BufferedReader(io.BytesIO(b)), format=args.bench)[0]
            if args.bench == 'edwin': font = font.label(char_from='ascii')
            started = time.perf_counter_ns()
            for _ in range(args.count):
                monobit.save(font, io.BytesIO(), format=args.bench)
            write_ns = time.perf_counter_ns()-started
        print(json.dumps(dict(format=args.bench, count=args.count, read_ns=read_ns, write_ns=write_ns, oracle='monobit', version=importlib.metadata.version('monobit'))))
        return
    commands = []

    def run(*argv):
        started = time.monotonic()
        result = subprocess.run([str(args.adapter), *map(str, argv)], capture_output=True, text=True, timeout=60)
        commands.append(dict(argv=[str(args.adapter), *map(str, argv)], exit=result.returncode, elapsed_seconds=time.monotonic()-started))
        if result.returncode:
            raise RuntimeError(result.stderr)
        return result.stdout

    checks = []
    for ident, source, provenance in emit(args.work):
        expected = snapshot(monobit.load(source, format=ident)[0], ident)
        actual = json.loads(run('dump', ident, source))
        if actual != expected:
            (args.work / f'{source.stem}.actual.json').write_text(json.dumps(actual, indent=2))
            (args.work / f'{source.stem}.expected.json').write_text(json.dumps(expected, indent=2))
            raise AssertionError(f'{ident}: independent reader -> native reader mismatch at {source}')
        target = source.with_suffix('.native')
        run('rewrite', ident, source, target)
        independent = snapshot(monobit.load(target, format=ident)[0], ident)
        if independent != expected:
            raise AssertionError(f'{ident}: native writer -> independent reader mismatch at {source}')
        if json.loads(run('dump', ident, target)) != independent:
            raise AssertionError(f'{ident}: native output differs between independent/native readers')
        checks.append(dict(format=ident, case=source.name, provenance=provenance, glyphs=len(expected[0]['glyphs']), fields=['legacy positions', 'Unicode aliases', 'Unicode sequences', 'width', 'height', 'left bearing', 'baseline', 'advance', 'all packed pixels', 'font ascent/descent', 'native family name', 'native glyph label', 'explicit point size'], source_sha256=hash_file(source), native_sha256=hash_file(target), status='pass'))
    oracle_root = Path(monobit.storage.fontformats.__file__).parent
    report = dict(status='pass', monobit_version=importlib.metadata.version('monobit'), oracle_sources={key: dict(file=str(oracle_root/value), sha256=hash_file(oracle_root/value)) for key, value in SOURCES.items()}, adapter_sha256=hash_file(args.adapter), samples_license='Original generated raster patterns and independently authored wire fixtures: project MIT; monobit tool MIT', psf_position_convention='Unlabeled PSF glyph slots acquire their implicit legacy index in native model; oracle comparison explicitly derives that wire index.', cases=checks, commands=commands)
    args.report.write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(status='pass', cases=len(checks), formats=len(IDS), report=str(args.report))))


if __name__ == '__main__':
    main()
