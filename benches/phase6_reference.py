#!/usr/bin/env python3
"""Read-only original Tong policy loader and complete frozen-BDF plan adapter."""
import importlib.util
from pathlib import Path
import sys


def load_module(name, path):
    protected = Path(__file__).resolve().parents[2] / 'reference'
    path = Path(path).resolve()
    if path.is_relative_to(protected.resolve()):
        raise ValueError('execute reference code only from a writable local-work copy')
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None: raise ImportError(path)
    module = importlib.util.module_from_spec(spec); spec.loader.exec_module(module)
    return module


def load_tong(work):
    return load_module('phase6_original_tong', Path(work) / 'reference/tong-pixel-sans/tools/export-rs/python_reference.py')


def parse_bdf(path):
    """The existing crosscheck/glyf.py plan projection, over every encoded glyph.

    This original-exporter record projection is not a native decoder or a
    substitute for complete BDF/PCF unencoded/metadata correctness validation.
    """
    records = {}; glyph = None; bitmap = False
    for line in Path(path).read_text().splitlines():
        key, _, value = line.partition(' ')
        if key == 'STARTCHAR': glyph = {}; bitmap = False
        elif key == 'ENCODING': glyph['cp'] = int(value)
        elif key == 'DWIDTH': glyph['adv'] = int(value.split()[0])
        elif key == 'BBX': glyph['w'], glyph['h'], glyph['x'], glyph['y'] = map(int, value.split())
        elif key == 'BITMAP': bitmap = True; glyph['rows'] = []
        elif key == 'ENDCHAR':
            if glyph['cp'] >= 0: records[glyph['cp']] = {k: glyph[k] for k in ('adv', 'x', 'y', 'rows')}
            glyph = None; bitmap = False
        elif bitmap:
            bits = bin(int(line, 16))[2:].zfill(len(line) * 4)[:glyph['w']]
            glyph['rows'].append(''.join('#' if b == '1' else '.' for b in bits))
    return records


def family_plan(ref, bdf, size, spacing):
    prefix = 'TongPixelSans' + ('Mono' if spacing == 'mono' else '') + '14' + size
    faces = {region: parse_bdf(Path(bdf) / (prefix + '-' + region + '.bdf'))
             for region in ['SC', 'TC', 'JP', 'KR', 'Latin']}
    ref.build.faces = lambda root, requested_size: [(region, faces[region], faces[region]) for region in ['SC', 'TC', 'JP', 'KR']]
    ref.build.latin_faces = lambda root, requested_size: (faces['Latin'], faces['Latin'])
    ref.set_size(size)
    return ref.plan(spacing, size)


def single_plan(ref, records):
    ref.build.faces = lambda root, size: [('SC', records, records), ('TC', {}, {}), ('JP', {}, {}), ('KR', {}, {})]
    ref.build.latin_faces = lambda root, size: ({}, {})
    ref.set_size('Large'); return ref.plan('prop', 'Large')
