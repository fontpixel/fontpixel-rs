#!/usr/bin/env python3
"""W5.2 independent monobit wire and external GML selection oracle.

Only the original fixture generator and syntax adapter are distributed here.
Pinned third-party GML stays in --work. The GameMaker engine is not executed.
"""
import argparse
import hashlib
import importlib.util
import inspect
import json
from pathlib import Path
import re
import subprocess
import urllib.request
import monobit
from monobit.base import FileFormatError
import monobit.storage.fontformats.apple.iigs as external_iigs
import monobit.storage.fontformats.apple.nfnt as external_nfnt

p = argparse.ArgumentParser()
p.add_argument('--driver', type=Path, required=True)
p.add_argument('--work', type=Path, required=True)
p.add_argument('--sciibin', type=Path, help='optional original external sciibin 3.1 executable')
args = p.parse_args()
args.driver = args.driver.resolve()
args.work.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('seed_w52', root / 'fuzz/seed_w52.py')
seed = importlib.util.module_from_spec(spec)
spec.loader.exec_module(seed)
checks = []
sciibin_checks = []
oracle_limitations = []


def native(fmt, path):
    return json.loads(subprocess.check_output([str(args.driver), 'inspect', fmt, str(path)]))['fonts'][0]


def rewrite(fmt, src, out, choice='bare', canonical=False):
    result = subprocess.check_output([str(args.driver), 'canonical' if canonical else 'rewrite', fmt, str(src), str(out), choice], text=True)
    return json.loads(result)


def oracle(path, fmt='iigs'):
    f = monobit.load(path, format=fmt)[0]
    return dict(family=f.family, point=f.point_size, ascent=f.ascent, descent=f.descent,
                glyphs=[dict(codes=[int(c) for c in g.codepoints], width=g.width, height=g.height,
                             x=g.left_bearing, y=g.shift_up, advance=g.advance_width,
                             pixels=g.as_bytes().hex()) for g in f.glyphs])


def compare(path, fmt='iigs'):
    actual = native('iigs', path)
    expected = oracle(path, fmt)
    for key in ['family', 'point', 'ascent', 'descent', 'glyphs']:
        assert actual[key] == expected[key], (path, key, actual[key], expected[key])
    checks.append(dict(file=path.name, sha256=hashlib.sha256(path.read_bytes()).hexdigest(), glyphs=len(expected['glyphs'])))


# Original wire and both directions through the independent monobit codec.
for n in range(12):
    raw = bytearray(seed.iigs())
    raw[43:49] = bytes(((0xb8 ^ n, 0, 0xc8 ^ (n * 3), 0, 0xb8 ^ (n * 5), 0)))
    raw[0] = 4
    path = args.work / f'original-{n}.fon'
    path.write_bytes(raw)
    compare(path)
    for version in [0x101, 0x105]:
        fs = monobit.load(path, format='iigs')
        other = args.work / f'monobit-{n}-{version:x}.fon'
        monobit.save(fs, other, format='iigs', version=version, overwrite=True)
        compare(other)
        out = args.work / f'rust-{n}-{version:x}.fon'
        rewrite('iigs', path, out, 'v1.1' if version == 0x101 else 'v1.5', canonical=True)
        compare(out)

# Exercise the actual high offset word, not merely a 1.5 header with zero high.
extended = args.work / 'extended-offset105.fon'
extended.write_bytes(seed.extended_iigs())
compare(extended)
extended_out = args.work / 'rust-extended-offset105.fon'
rewrite('iigs', extended, extended_out, 'v1.5', canonical=True)
compare(extended_out)

# The external NFNT helper wrongly applies the Mac positive-nDescent high-word
# convention to IIgs too. Adapt only that branch condition for signed descent;
# all raster and metric interpretation still comes from the independent codec.
external_source = inspect.getsource(external_nfnt.extract_nfnt)
assert external_source.count('if fontrec.nDescent > 0:') == 1
adapted_scope = {}
adapted_extract_source = external_source.replace('if fontrec.nDescent > 0:',
                                                "if endian == 'big' and fontrec.nDescent > 0:")
exec(adapted_extract_source,
     external_nfnt.__dict__, adapted_scope)
external_header_source = inspect.getsource(external_nfnt.nfnt_header_struct)
adapted_header_source = external_header_source
for field in ['ascent', 'descent', 'leading']:
    needle = f"{field}='uint16'"
    assert adapted_header_source.count(needle) == 1
    adapted_header_source = adapted_header_source.replace(needle, f"{field}='int16'")
exec(adapted_header_source, external_nfnt.__dict__, adapted_scope)


def compare_signed(path):
    original = external_iigs.extract_nfnt
    original_header = external_nfnt.nfnt_header_struct
    try:
        unmodified = oracle(path)
    except FileFormatError as error:
        oracle_limitations.append(dict(file=path.name, error=str(error),
                                       cause='monobit applies Mac positive-nDescent high offset to IIgs; external adapter restricts that branch to big endian'))
    else:
        oracle_limitations.append(dict(file=path.name, unmodified_ascent=unmodified['ascent'],
                                       unmodified_descent=unmodified['descent'],
                                       cause='monobit declares ascent/descent/leading uint16; Apple IIgs specification permits signed words'))
    external_iigs.extract_nfnt = adapted_scope['extract_nfnt']
    external_nfnt.nfnt_header_struct = adapted_scope['nfnt_header_struct']
    try:
        compare(path)
        checks[-1]['oracle'] = 'monobit with explicit external IIgs signed-metric/high-offset correction; not unmodified consumer support'
    finally:
        external_iigs.extract_nfnt = original
        external_nfnt.nfnt_header_struct = original_header


# Signed metrics and rectangles entirely above/below the baseline, both ways.
for name, metrics in [('above-baseline', (8, -5, -2)), ('below-baseline', (-2, 5, 1))]:
    shifted = args.work / f'{name}.fon'
    shifted.write_bytes(seed.shifted_iigs(*metrics))
    compare_signed(shifted)
    for version in [0x101, 0x105]:
        out = args.work / f'rust-{name}-{version:x}.fon'
        rewrite('iigs', shifted, out, 'v1.1' if version == 0x101 else 'v1.5', canonical=True)
        compare_signed(out)

# Apple locates locTable immediately before owTable. monobit 0.54 assumes it
# immediately follows the raster and rejects a legal pre-table gap.
gap = bytearray(seed.iigs())
gap[49:49] = b'\xde\xad\xbe\xef'
gap[33:35] = (14).to_bytes(2, 'little')
gap_path = args.work / 'table-gap.fon'
gap_path.write_bytes(gap)
assert native('iigs', gap_path)['glyphs'] == native('iigs', args.work / 'original-0.fon')['glyphs']
try:
    oracle(gap_path)
except ValueError as error:
    oracle_limitations.append(dict(file=gap_path.name, sha256=hashlib.sha256(gap).hexdigest(), error=str(error),
                                   cause='monobit 0.54 assumes raster-adjacent locTable; Apple pointer-relative native test passes'))
else:
    compare(gap_path)

# The wrapper oracle reads only. Save raw generated wrappers for inspection.
base = args.work / 'original-0.fon'
for padding in [0, 12288, 30000]:
    source = args.work / f'padding-{padding}.fon'
    source.write_bytes(base.read_bytes() + b'\0' * padding)
    out = args.work / f'native-{padding}.bsc'
    rewrite('iigs', source, out, 'binscii')
    compare(out, 'iigs')
    if args.sciibin:
        decoded = args.work / f'sciibin-{padding}.fon'
        decoded.unlink(missing_ok=True)
        command = [str(args.sciibin.resolve()), '-o' + str(decoded.resolve()), str(out.resolve())]
        result = subprocess.run(command, capture_output=True, text=True, check=True)
        assert decoded.read_bytes() == source.read_bytes()
        assert 'crc' not in result.stderr.lower(), result.stderr
        sciibin_checks.append(dict(command=command, exit=result.returncode, stdout=result.stdout,
                                  stderr=result.stderr, bytes=decoded.stat().st_size,
                                  sha256=hashlib.sha256(decoded.read_bytes()).hexdigest()))

# These transport fixtures are independently authored by the Python generator,
# using the standard library CRC rather than the production codec's writer.
for padding in [0, 30000]:
    independent = args.work / f'python-{padding}.bsc'
    independent.write_bytes(seed.binscii(seed.iigs() + b'\0' * padding))
    compare(independent, 'iigs')

# Native raw components are independently decoded as 360 fixed 16x16 glyphs.
han = args.work / 'original.han'
han.write_bytes(seed.hanfont())
actual = native('components', han)['glyphs']
external = monobit.load(han, format='raw', cell=(16, 16), count=360)[0]
assert [g['pixels'] for g in actual] == [g.as_bytes().hex() for g in external.glyphs]
raw_out = args.work / 'monobit.han'
monobit.save(external, raw_out, format='raw', overwrite=True)
assert raw_out.read_bytes() == han.read_bytes()
rewrite('components', raw_out, args.work / 'rust.han')
assert (args.work / 'rust.han').read_bytes() == han.read_bytes()
checks.append(dict(file='original.han', sha256=hashlib.sha256(han.read_bytes()).hexdigest(), glyphs=360, oracle='monobit raw geometry; composition checked separately'))

# Execute the published renderer's lookup generator and syllable drawing branch
# via mechanical GML -> JavaScript syntax adaptation. No variant table is copied
# into this script or into the production Rust implementation.
commit = 'e21dc74c83491f8708a4a3391dc6ea7125470f0e'
files = {
    'hj_init.gml': '848805397c39e1601fc70485ea38c0ee4f4eab3d51e81831a38912f65c5cee40',
    'hj_gen_table.gml': 'a97ee00786124fdc7d8bbc2ade1abb8c7e569e97cba90818807cb128d849dbda',
    'hj_draw_raw.gml': '00100500377ebfdfff749995c0692ec3edab70cc2d9bcba835c7b9f1e31bbc96',
}
sources = {}
for name, digest in files.items():
    dest = args.work / name
    url = f'https://raw.githubusercontent.com/TandyRum1024/hangul-johab-render-gms/{commit}/project/Hangul_boy.gmx/scripts/{name}'
    if not dest.exists():
        dest.write_bytes(urllib.request.urlopen(url).read())
    assert hashlib.sha256(dest.read_bytes()).hexdigest() == digest, name
    sources[name] = dest.read_text()


def syntax(code):
    code = re.sub(r'/\*.*?\*/', '', code, flags=re.S)
    code = re.sub(r'//[^\n]*', '', code)
    code = re.sub(r'\$([0-9A-Fa-f]+)', r'0x\1', code)
    return code.replace('[@', '[')


lookup = syntax(sources['hj_gen_table.gml'])
# GameMaker implicitly changes a scalar to an array on indexed assignment.
lookup = lookup.replace('var _strlen', 'if (_map === -1) _map = [];\nvar _strlen')
initialization = '\n'.join(line for line in syntax(sources['hj_init.gml']).splitlines()
                           if line.strip().startswith('global.hjLUT') and '= hj_gen_table(' in line)
draw = syntax(sources['hj_draw_raw.gml'])
start = draw.index('_kr = _curord - 0xAC00;')
end = draw.index('draw_sprite_ext(global.hjSpriteHan, _tailidx', start)
end = draw.index(';', end) + 1
draw = draw[start:end]
draw = re.sub(r'\(\(_kr % (\d+)\) div (\d+)\)', r'Math.floor((_kr % \1) / \2)', draw)
draw = re.sub(r'\(_kr div (\d+)\)', r'Math.floor(_kr / \1)', draw)
js = '''const global = {};
const string_length = s => s.length;
const string_ord_at = (s,i) => s.codePointAt(i-1);
const string_pos = (s,t) => t.indexOf(s)+1;
const chr = String.fromCodePoint;
let result = [];
function draw_sprite_ext(sprite,index) { result.push(index); }
function hj_gen_table(argument0,argument1,argument2) {\n''' + lookup + '\n}\n' + initialization
js += '\nfunction compose(_curord) {let _kr, _offx=0, _offy=0, _strcol=0; result=[];\n' + draw + '\nreturn result;}\n'
js += 'process.stdout.write(JSON.stringify(Array.from({length:11172},(_,n)=>compose(0xac00+n))));\n'
adapter = args.work / 'external-gml-adapted.js'
adapter.write_text(js)
external_slots = json.loads(subprocess.check_output(['node', str(adapter)]))
rust_slots = json.loads(subprocess.check_output([str(args.driver), 'slots']))
# The renderer stores every variation in 28-column sprite rows, including blank
# columns. Translate that storage geometry only; selection comes from its code.
projected = []
for n, (a, v, t) in enumerate(external_slots):
    projected.append([a // 28 * 20 + a % 28,
                      160 + (v // 28 - 8) * 22 + v % 28,
                      None if n % 28 == 0 else 248 + (t // 28 - 12) * 28 + t % 28])
assert rust_slots == projected
# Compose actual bytes selected by the independent program, all 11,172 rasters.
data = han.read_bytes()
glyphs = native('hanfont', han)['glyphs']
for n, (glyph, slots) in enumerate(zip(glyphs, projected)):
    raster = bytes([data[slots[0] * 32 + k] | data[slots[1] * 32 + k] |
                    (data[slots[2] * 32 + k] if slots[2] is not None else 0) for k in range(32)])
    assert glyph['codes'] == [0xac00 + n] and glyph['pixels'] == raster.hex(), n
report = dict(monobit=monobit.__version__, checks=len(checks), cases=checks,
              sciibin=sciibin_checks,
              oracle_limitations=oracle_limitations,
              signed_metric_adapter=dict(cases=6, scope='supplementary only; process-local function replacement, shared installed sources unchanged',
                                         original_extract_sha256=hashlib.sha256(external_source.encode()).hexdigest(),
                                         original_header_sha256=hashlib.sha256(external_header_source.encode()).hexdigest(),
                                         adapted_extract_sha256=hashlib.sha256(adapted_extract_source.encode()).hexdigest(),
                                         adapted_header_sha256=hashlib.sha256(adapted_header_source.encode()).hexdigest(),
                                         changes=['nDescent high word only for big-endian Mac NFNT', 'ascent/descent/leading declarations int16 per Apple IIgs p16-48']),
              composition=dict(syllables=len(glyphs), source_commit=commit, files=files,
                               adapter_sha256=hashlib.sha256(adapter.read_bytes()).hexdigest(),
                               scope='GML lookup generator and drawing branch under JS syntax adaptation; not GameMaker runtime',
                               limitation='absent-final filler is ignored; no third-party artwork; 28-column sprite rows remapped to compact wire slots'))
print(json.dumps(report, indent=2))
