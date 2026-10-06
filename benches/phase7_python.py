#!/usr/bin/env python3
"""Phase 7 Python benchmark: fontpixel bindings versus pure-Python bdfparser.

Three engines run the same workloads, each in its own fresh interpreter:

  ref     pure-Python bdfparser, imported from a writable copy of
          reference/bdfparser/src (never from reference/ itself);
  compat  fontpixel.compat.bdfparser (the drop-in bdfparser API);
  native  the native fontpixel API (fontpixel.read/write, Font, Bitmap).

Subcommands:

  run --out RAW.json      measure every case (or --cases a,b) with --reps fresh
                          processes per engine, interleaving engines per rep;
                          writes RAW.json after each case so partial runs remain
  worker ENGINE CASE      one measurement process (internal; prints one JSON line)
  summarize RAW.json      print the Markdown tables used by PHASE7-REPORT.md
  cases                   list the cases

Contracts (see benches/phase7-README.md):
  whole   one untimed warmup call, then one timed call per process; gc.collect()
          before and after the warmup, outside the timer; automatic GC included.
  small   per process: double the loop count until a batch takes >= 20 ms, scale
          to ~50 ms batches, run one warmup batch, then 7 timed batches with
          gc.collect() before each (outside the timer); the per-call value is
          the median batch time / batch size. The `for` loop and closure call are
          included in every engine; the `noop` case measures that baseline.
Every case digests its result outside the timer, so engines are checked to
compute the same thing; a mismatch is reported, never hidden.
"""
from __future__ import annotations

import argparse
import gc
import hashlib
import json
import os
import platform
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve()
REPO = HERE.parents[1]
FONTPIXEL_ROOT = REPO.parent if (REPO.parent / 'samples').is_dir() else Path('/home/chen/githubprojects/fontpixel')
SAMPLES = FONTPIXEL_ROOT / 'samples' / 'tong-pixel-sans'
TONG_SC = SAMPLES / 'TongPixelSans14Large-SC.bdf'
TONG_LATIN = SAMPLES / 'TongPixelSans14Large-Latin.bdf'
PROTECTED_REFERENCE = FONTPIXEL_ROOT / 'reference'
TINY_BDF = PROTECTED_REFERENCE / 'bdfparser' / 'tests' / 'fonts' / 'unifont-13.0.04-2atest.bdf'
ENGINES = ('ref', 'compat', 'native')
YONG = '永'  # 永

# --------------------------------------------------------------------------- worker


class NotApplicable(Exception):
    """The case has no counterpart in this engine."""


def sha(data: bytes | str) -> str:
    if isinstance(data, str):
        data = data.encode('utf-8')
    return hashlib.sha256(data).hexdigest()


def rows_digest_bdfp(bitmap) -> str:
    """bdfparser-style Bitmap (ref or compat): rows of '0'/'1'/'2' labels."""
    return sha('\n'.join(bitmap.todata(1)))


def rows_digest_native(bitmap) -> str:
    return sha('\n'.join(''.join(str(v) for v in row) for row in bitmap.to_rows()))


def pattern_rows(w: int, h: int, a: int = 13, b: int = 7, m: int = 11, k: int = 4) -> list[str]:
    return [''.join('1' if (x * a + y * b) % m < k else '0' for x in range(w)) for y in range(h)]


def mixed_text(data: bytes, n: int = 2000) -> str:
    """n characters spread evenly over the font's encoded codepoints (> U+0020)."""
    cps = sorted({int(v) for v in re.findall(rb'^ENCODING (\d+)', data, re.M)})
    cps = [c for c in cps if c > 0x20 and not 0xD800 <= c <= 0xDFFF]
    return ''.join(chr(cps[i * len(cps) // n]) for i in range(n))


class Ctx:
    """Lazily loaded inputs and engine modules for one worker process."""

    def __init__(self, engine: str, ref_src: Path | None) -> None:
        self.engine = engine
        if engine == 'ref':
            if ref_src is None:
                raise SystemExit('--ref-src is required for the ref engine')
            sys.dont_write_bytecode = True
            sys.path.insert(0, str(ref_src))
            import bdfparser  # noqa: PLC0415
            self.mod = bdfparser
            self.Font, self.Bitmap = bdfparser.Font, bdfparser.Bitmap
        elif engine == 'compat':
            from fontpixel.compat import bdfparser  # noqa: PLC0415
            self.mod = bdfparser
            self.Font, self.Bitmap = bdfparser.Font, bdfparser.Bitmap
        else:
            import fontpixel  # noqa: PLC0415
            self.mod = fontpixel
            self.fp = fontpixel
        self._cache: dict[str, object] = {}

    def data(self, path: Path) -> bytes:
        key = 'data:' + str(path)
        if key not in self._cache:
            self._cache[key] = path.read_bytes()
        return self._cache[key]

    def font(self, path: Path = TONG_SC):
        """bdfparser Font (ref/compat) or (FontSet, Font) (native)."""
        key = 'font:' + str(path)
        if key not in self._cache:
            if self.engine == 'native':
                fonts = self.fp.read(self.data(path), 'bdf').fonts
                self._cache[key] = (fonts, fonts[0])
            else:
                self._cache[key] = self.Font().load_bytes(self.data(path))
        return self._cache[key]

    def bitmap(self, w: int = 64, h: int = 64, depth: str = 'mono', **kw):
        rows = pattern_rows(w, h, **kw)
        if self.engine == 'native':
            return self.fp.Bitmap.from_rows([[int(c) for c in r] for r in rows], depth)
        return self.Bitmap(rows)


CASES: dict[str, tuple[str, str, object]] = {}


# Each builder returns (op, digest): op() is timed; digest(result) -> str is not.
# Builders raise NotApplicable when an engine has no counterpart.

def _engine_split(bdfp_builder, native_builder):
    def build(ctx: Ctx):
        return native_builder(ctx) if ctx.engine == 'native' else bdfp_builder(ctx)
    return build


def define(name: str, kind: str, label: str, bdfp_builder, native_builder) -> None:
    CASES[name] = (kind, label, _engine_split(bdfp_builder, native_builder))


def count_digest(n) -> str:
    return str(n)


# ---- whole workloads (Tong SC, 38,653 glyphs, 5,746,021 bytes) ----

def _parse_bdfp(path):
    def b(ctx):
        data = ctx.data(path)
        return (lambda: ctx.Font().load_bytes(data)), (lambda f: str(len(f.glyphs) + len(f.unencoded)))
    return b


def _parse_native(path):
    def b(ctx):
        data = ctx.data(path)
        return (lambda: ctx.fp.read(data, 'bdf')), (lambda l: str(sum(len(f) for f in l.fonts)))
    return b


define('parse_tong_sc', 'whole', 'Parse Tong SC BDF (5.7 MB, 38,653 glyphs)',
       _parse_bdfp(TONG_SC), _parse_native(TONG_SC))
define('parse_latin', 'whole', 'Parse Tong Latin BDF (419 KB)',
       _parse_bdfp(TONG_LATIN), _parse_native(TONG_LATIN))


def _write_bdfp(method):
    def b(ctx):
        font = ctx.font()
        return getattr(font, method), sha
    return b


def _write_native(fmt):
    def b(ctx):
        fonts, _ = ctx.font()
        return (lambda: ctx.fp.write(fonts, fmt)), (lambda w: sha(w.output.assets[0].data))
    return b


define('write_bdf', 'whole', 'Write BDF (Tong SC)', _write_bdfp('tobdf'), _write_native('bdf'))
define('write_pcf', 'whole', 'Write PCF (Tong SC)', _write_bdfp('topcf'), _write_native('pcf'))
define('write_ttf', 'whole', 'Write TTF (Tong SC)', _write_bdfp('tottf'), _write_native('ttf'))


def _draw_bdfp(text_of, **kw):
    def b(ctx):
        font = ctx.font()
        text = text_of(ctx)
        opts = dict(kw)
        if opts.get('linelimit') == 'fbbx80':
            opts['linelimit'] = 80 * font.headers['fbbx']
        return (lambda: font.draw(text, **opts)), rows_digest_bdfp
    return b


def _draw_native(text_of, options):
    def b(ctx):
        _, font = ctx.font()
        text = text_of(ctx)
        opts = dict(options)
        if opts.get('line_limit') == 'fbbx80':
            opts['line_limit'] = 80 * font.metrics['bounds']['width']
        return (lambda: font.draw(text, opts)), (lambda r: rows_digest_native(r.bitmap))
    return b


def _repeat(ctx):
    return YONG * 2000


def _mixed(ctx):
    return mixed_text(ctx.data(TONG_SC))


define('draw_2000_repeat', 'whole', "Draw '永'×2000 (mode 0, linelimit 80×fbbx; phase-6 workload)",
       _draw_bdfp(_repeat, mode=0, linelimit='fbbx80'),
       _draw_native(_repeat, {'mode': 'cell', 'line_limit': 'fbbx80'}))
define('draw_2000_mixed', 'whole', 'Draw 2,000 different characters (mode 1, linelimit 512)',
       _draw_bdfp(_mixed, mode=1, linelimit=512),
       _draw_native(_mixed, {'mode': 'advance', 'line_limit': 512}))


def _drawall_bdfp(**kw):
    def b(ctx):
        font = ctx.font()
        return (lambda: font.drawall(**kw)), rows_digest_bdfp
    return b


def _drawall_native(options):
    def b(ctx):
        _, font = ctx.font()
        return (lambda: font.draw_all(options=options)), (lambda r: rows_digest_native(r.bitmap))
    return b


define('drawall_mode1', 'whole', 'drawall(mode=1), all 38,653 glyphs',
       _drawall_bdfp(mode=1), _drawall_native({'mode': 'advance', 'line_limit': 512}))
# bdfparser's default drawall() is mode 0: a 512×43,488 (22.3 Mpx) bitmap. The
# compat layer keeps the native 16,777,216-pixel default budget and fails; the
# native run raises its bitmap budget explicitly (recorded in the label).
define('drawall_mode0_default', 'whole',
       'drawall() defaults (mode 0, 22.3 Mpx); native with max_pixels raised to 2^26',
       _drawall_bdfp(),
       _drawall_native({'mode': 'cell', 'line_limit': 512,
                        'limits': {'bitmap': {'max_pixels': 1 << 26}}}))


def _iter_bdfp(meta: bool):
    def b(ctx):
        font = ctx.font()
        if meta:
            return (lambda: [(g.cp(), g.meta['dwx0']) for g in font.iterglyphs()]), (lambda v: sha(repr(sorted(v))))
        return (lambda: sum(1 for _ in font.iterglyphs())), count_digest
    return b


def _iter_native(meta: bool):
    def b(ctx):
        _, font = ctx.font()
        if meta:
            def op():
                out = []
                for _, g in font.glyphs():
                    cps = g.codepoints
                    if cps:
                        out.append((cps[0].value, g.metrics['advance']['x']))
                return out
            return op, (lambda v: sha(repr(sorted(v))))
        return (lambda: sum(1 for _ in font.glyphs())), count_digest
    return b


define('iterglyphs', 'whole', 'Iterate all glyph objects (Tong SC)', _iter_bdfp(False), _iter_native(False))
define('iterglyphs_meta', 'whole', 'Iterate all glyphs reading codepoint + advance',
       _iter_bdfp(True), _iter_native(True))

# ---- small single calls ----

define('noop', 'small', 'Baseline: empty Python closure call',
       lambda ctx: ((lambda: None), (lambda v: 'None')),
       lambda ctx: ((lambda: None), (lambda v: 'None')))
define('parse_tiny', 'small', 'Parse a 1.2 KB BDF (unifont 2atest, 3 glyphs)',
       _parse_bdfp(TINY_BDF), _parse_native(TINY_BDF))


def _lookup_bdfp(by_cp: bool):
    def b(ctx):
        font = ctx.font()
        op = (lambda: font.glyphbycp(0x6C38)) if by_cp else (lambda: font.glyph(YONG))
        return op, (lambda g: str(g.cp()))
    return b


def _lookup_native(ctx):
    _, font = ctx.font()
    return (lambda: font.glyph_for_codepoint(0x6C38)), (lambda g: str(g.codepoints[0].value))


define('glyph_lookup', 'small', "font.glyph('永') / native glyph_for_codepoint", _lookup_bdfp(False), _lookup_native)
define('glyphbycp', 'small', 'font.glyphbycp(0x6C38) / native glyph_for_codepoint', _lookup_bdfp(True), _lookup_native)


def _hdr_bdfp(ctx):
    font = ctx.font()
    return (lambda: font.headers['fbbx']), str


def _hdr_native(ctx):
    _, font = ctx.font()
    return (lambda: font.metrics['bounds']['width']), str


def _prop_bdfp(ctx):
    font = ctx.font()
    return (lambda: font.props['font_ascent']), (lambda v: str(int(v)))


def _prop_native(ctx):
    _, font = ctx.font()
    return (lambda: font.properties.get('FONT_ASCENT')), (lambda v: str(v['Integer']))


def _glyph(ctx):
    if ctx.engine == 'native':
        _, font = ctx.font()
        return font, font.glyph_for_codepoint(0x6C38)
    font = ctx.font()
    return font, font.glyph(YONG)


def _meta_bdfp(ctx):
    _, g = _glyph(ctx)
    return (lambda: g.meta['dwx0']), str


def _meta_native(ctx):
    _, g = _glyph(ctx)
    return (lambda: g.metrics['advance']['x']), str


def _cp_bdfp(ctx):
    _, g = _glyph(ctx)
    return g.cp, str


def _cp_native(ctx):
    _, g = _glyph(ctx)
    return (lambda: g.codepoints[0].value), str


define('attr_font_header', 'small', "font.headers['fbbx'] / native font.metrics['bounds']['width']",
       _hdr_bdfp, _hdr_native)
define('attr_font_prop', 'small', "font.props['font_ascent'] / native font.properties.get('FONT_ASCENT')",
       _prop_bdfp, _prop_native)
define('attr_glyph_meta', 'small', "glyph.meta['dwx0'] / native glyph.metrics['advance']['x']",
       _meta_bdfp, _meta_native)
define('attr_glyph_cp', 'small', 'glyph.cp() / native glyph.codepoints[0].value', _cp_bdfp, _cp_native)


def _gdraw_bdfp(ctx):
    _, g = _glyph(ctx)
    return g.draw, rows_digest_bdfp


def _gdraw_native(ctx):
    font, g = _glyph(ctx)
    gid = g.id
    return (lambda: font.glyph_in_cell(gid)), rows_digest_native


define('glyph_draw', 'small', 'glyph.draw() in the font box / native font.glyph_in_cell(id)',
       _gdraw_bdfp, _gdraw_native)


def _draw1_bdfp(ctx):
    font = ctx.font()
    return (lambda: font.draw(YONG)), rows_digest_bdfp


def _draw1_native(ctx):
    _, font = ctx.font()
    return (lambda: font.draw(YONG)), (lambda r: rows_digest_native(r.bitmap))


define('draw_one_char', 'small', "font.draw('永') (one-glyph render)", _draw1_bdfp, _draw1_native)


def _bm_construct_bdfp(n):
    def b(ctx):
        rows = pattern_rows(n, n)
        return (lambda: ctx.Bitmap(rows)), rows_digest_bdfp
    return b


def _bm_construct_native(n):
    def b(ctx):
        text = '\n'.join(pattern_rows(n, n))
        palette = {'0': 0, '1': 1}
        return (lambda: ctx.fp.Bitmap.from_text(text, palette)), rows_digest_native
    return b


define('bitmap_new_16', 'small', 'Bitmap(16 row strings) / native Bitmap.from_text',
       _bm_construct_bdfp(16), _bm_construct_native(16))
define('bitmap_new_64', 'small', 'Bitmap(64 row strings of 64) / native Bitmap.from_text',
       _bm_construct_bdfp(64), _bm_construct_native(64))


def _bm_bdfp(expr):
    def b(ctx):
        bm = ctx.bitmap()
        other = ctx.bitmap(32, 48, a=5, b=3, m=7, k=3)
        return (lambda: expr(bm, other)), rows_digest_bdfp
    return b


def _bm_native(expr, depth='mono'):
    def b(ctx):
        bm = ctx.bitmap(depth=depth)
        other = ctx.bitmap(32, 48, a=5, b=3, m=7, k=3)
        return (lambda: expr(bm, other)), rows_digest_native
    return b


define('bitmap_clone', 'small', 'b.clone() / native b.copy() (64×64)',
       _bm_bdfp(lambda b, o: b.clone()), _bm_native(lambda b, o: b.copy()))
define('bitmap_crop', 'small', 'b.clone().crop(31, 31, 3, 5) / native b.crop(...) (64×64)',
       _bm_bdfp(lambda b, o: b.clone().crop(31, 31, 3, 5)), _bm_native(lambda b, o: b.crop(31, 31, 3, 5)))
define('bitmap_concat', 'small', 'b.clone().concat(o) right/bottom / native b.concat(o, "right", "end")',
       _bm_bdfp(lambda b, o: b.clone().concat(o, direction=1, align=1, offset=0)),
       _bm_native(lambda b, o: b.concat(o, 'right', 'end', 0)))
define('bitmap_shift', 'small', 'b.clone().shift(3, -2) / native b.shift(3, -2) (64×64)',
       _bm_bdfp(lambda b, o: b.clone().shift(3, -2)), _bm_native(lambda b, o: b.shift(3, -2)))
define('bitmap_glow', 'small', 'b.clone().glow() / native gray8 b.glow(False, 1, 2) (64×64)',
       _bm_bdfp(lambda b, o: b.clone().glow()),
       _bm_native(lambda b, o: b.glow(False, 1, 2), depth='gray8'))


def _todata_bdfp(k):
    def b(ctx):
        bm = ctx.bitmap()
        return (lambda: bm.todata(k)), (lambda v: sha(repr(v)))
    return b


_TODATA_NATIVE = {
    0: ('to_text({0:"0",1:"1"})', lambda b: b.to_text({0: '0', 1: '1'})),
    1: ('to_text(...).split("\\n")', lambda b: b.to_text({0: '0', 1: '1'}).split('\n')),
    2: ('to_rows()', lambda b: b.to_rows()),
    3: ('to_list()', lambda b: b.to_list()),
    4: ('to_numeric_hex_rows()', lambda b: b.to_numeric_hex_rows()),
    5: ('to_row_values()', lambda b: b.to_row_values()),
}


def _todata_native(k):
    def b(ctx):
        bm = ctx.bitmap()
        fn = _TODATA_NATIVE[k][1]
        return (lambda: fn(bm)), (lambda v: sha(repr(v)))
    return b


for _k in range(6):
    define(f'todata_{_k}', 'small', f'b.todata({_k}) / native {_TODATA_NATIVE[_k][0]} (64×64)',
           _todata_bdfp(_k), _todata_native(_k))


def _getpixel_bdfp(ctx):
    bm = ctx.bitmap()
    return (lambda: bm.getpixel(10, 20)), str


def _getpixel_native(ctx):
    bm = ctx.bitmap()
    return (lambda: bm.get_pixel(10, 20)), str


define('getpixel', 'small', 'b.getpixel(10, 20) / native b.get_pixel(10, 20)', _getpixel_bdfp, _getpixel_native)


def time_whole(op):
    gc.collect()
    warm = op()
    del warm
    gc.collect()
    c0 = time.process_time_ns()
    t0 = time.perf_counter_ns()
    value = op()
    t1 = time.perf_counter_ns()
    c1 = time.process_time_ns()
    return value, {'ns': t1 - t0, 'cpu_ns': c1 - c0, 'calls': 1}


def time_small(op, batches: int = 7, target_ns: int = 50_000_000):
    gc.collect()
    n = 1
    while True:
        t0 = time.perf_counter_ns()
        for _ in range(n):
            op()
        dt = time.perf_counter_ns() - t0
        if dt >= 20_000_000:
            break
        n *= 2
    k = max(1, round(n * target_ns / dt))
    for _ in range(k):  # warmup batch
        op()
    samples = []
    for _ in range(batches):
        gc.collect()
        t0 = time.perf_counter_ns()
        for _ in range(k):
            op()
        samples.append((time.perf_counter_ns() - t0) / k)
    value = op()
    return value, {'ns': statistics.median(samples), 'batch_ns': samples, 'calls': k}


def worker(engine: str, name: str, ref_src: Path | None) -> dict:
    import resource  # noqa: PLC0415
    import warnings  # noqa: PLC0415
    warnings.simplefilter('ignore')  # identical filter in every engine
    kind, label, build = CASES[name]
    ctx = Ctx(engine, ref_src)
    out = {'engine': engine, 'case': name, 'kind': kind, 'python': sys.version.split()[0],
           'module_file': getattr(ctx.mod, '__file__', None)}
    try:
        op, digest = build(ctx)
    except NotApplicable as e:
        out['outcome'] = 'n/a'
        out['reason'] = str(e)
        return out
    try:
        value, timing = (time_whole if kind == 'whole' else time_small)(op)
    except Exception as e:  # noqa: BLE001 - recorded as the outcome
        out['outcome'] = 'error'
        out['error'] = f'{type(e).__module__}.{type(e).__qualname__}: {e}'
        return out
    out.update(timing)
    out['outcome'] = 'ok'
    out['digest'] = digest(value)
    out['maxrss_kib'] = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    loaded = sorted(m for m in sys.modules if m == 'fontpixel' or m.startswith('fontpixel.') or m == 'bdfparser' or m.startswith('bdfparser.'))
    out['foreign_modules'] = [m for m in loaded if (engine == 'ref') == m.startswith('fontpixel')]
    return out


# --------------------------------------------------------------------------- driver


def loadavg() -> list[float]:
    return [float(x) for x in Path('/proc/loadavg').read_text().split()[:3]]


def file_sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def tree_sha(root: Path) -> str:
    h = hashlib.sha256()
    for p in sorted(root.rglob('*.py')):
        h.update(str(p.relative_to(root)).encode() + b'\0' + p.read_bytes())
    return h.hexdigest()


def machine() -> dict:
    cpu = ''
    for line in Path('/proc/cpuinfo').read_text().splitlines():
        if line.startswith('model name'):
            cpu = line.split(':', 1)[1].strip()
            break
    mem = Path('/proc/meminfo').read_text().splitlines()[0]
    return {'cpu': cpu, 'logical_cpus': os.cpu_count(), 'mem': mem, 'kernel': platform.release(),
            'platform': platform.platform()}


def engine_identity(python: str, ref_src: Path) -> dict:
    probe = ('import json,fontpixel,fontpixel._native as n,sys;'
             'print(json.dumps({"fontpixel":fontpixel.__file__,"native":n.__file__,'
             '"version":fontpixel.__version__,"python":sys.version}))')
    info = json.loads(subprocess.run([python, '-I', '-c', probe], check=True, capture_output=True, text=True).stdout)
    info['native_sha256'] = file_sha(Path(info['native']))
    info['ref_src'] = str(ref_src)
    info['ref_tree_sha256'] = tree_sha(ref_src)
    info['ref_version'] = re.search(r"__version__ = '([^']+)'", (ref_src / 'bdfparser' / '__init__.py').read_text()).group(1)
    try:
        info['repo_head'] = subprocess.run(['git', '-C', str(REPO), 'rev-parse', 'HEAD'], check=True,
                                           capture_output=True, text=True).stdout.strip()
        info['repo_dirty'] = bool(subprocess.run(['git', '-C', str(REPO), 'status', '--porcelain'], check=True,
                                                 capture_output=True, text=True).stdout.strip())
    except (OSError, subprocess.CalledProcessError):
        info['repo_head'] = None
    return info


def run(args) -> None:
    ref_src = Path(args.ref_src).resolve()
    if ref_src.is_relative_to(PROTECTED_REFERENCE.resolve()):
        raise SystemExit('run the reference from a writable copy, not from reference/')
    out = Path(args.out)
    if out.exists() and not args.resume:
        raise SystemExit(f'{out} exists (use --resume to continue it)')
    cases = args.cases.split(',') if args.cases else list(CASES)
    engines = args.engines.split(',')
    for c in cases:
        if c not in CASES:
            raise SystemExit(f'unknown case {c}')
    prefix: list[str] = []
    if not args.no_scope:
        prefix += ['systemd-run', '--user', '--scope', '-q', '-p', 'CPUWeight=10000', '--']
    if args.cpus:
        prefix += ['taskset', '-c', args.cpus]
    if out.exists():
        raw = json.loads(out.read_text())
    else:
        raw = {'schema': 1, 'profile': 'phase7-python-v1', 'started': time.strftime('%Y-%m-%dT%H:%M:%S%z'),
               'machine': machine(), 'identity': engine_identity(args.python, ref_src),
               'isolation': {'prefix': prefix, 'reps': args.reps, 'note': args.note},
               'inputs': {str(p): file_sha(p) for p in (TONG_SC, TONG_LATIN, TINY_BDF)},
               'cases': {}}
    for c in cases:
        if c in raw['cases'] and args.resume:
            continue
        kind, label, _ = CASES[c]
        rec = {'kind': kind, 'label': label, 'runs': {e: [] for e in engines}}
        for rep in range(args.reps):
            for e in engines:
                cmd = prefix + [args.python, '-I', str(HERE), 'worker', e, c, '--ref-src', str(ref_src)]
                before = loadavg()
                t0 = time.perf_counter()
                p = subprocess.run(cmd, capture_output=True, text=True)
                wall = time.perf_counter() - t0
                after = loadavg()
                if p.returncode != 0:
                    result = {'outcome': 'crash', 'returncode': p.returncode, 'stderr': p.stderr[-4000:]}
                else:
                    result = json.loads(p.stdout.strip().splitlines()[-1])
                result.update({'rep': rep, 'process_wall_s': wall, 'load_before': before, 'load_after': after})
                rec['runs'][e].append(result)
                shown = f"{result['ns'] / 1e6:.3f} ms" if result.get('outcome') == 'ok' else result.get('outcome')
                print(f'{c:24s} {e:7s} rep {rep}: {shown}  load {before[0]:.2f}->{after[0]:.2f}', flush=True)
        raw['cases'][c] = rec
        raw['finished'] = time.strftime('%Y-%m-%dT%H:%M:%S%z')
        tmp = out.with_suffix('.tmp')
        tmp.write_text(json.dumps(raw, indent=1) + '\n')
        tmp.replace(out)


# --------------------------------------------------------------------------- summary


def fmt_time(ns: float) -> str:
    if ns >= 1e9:
        return f'{ns / 1e9:.2f} s'
    if ns >= 1e6:
        return f'{ns / 1e6:.1f} ms'
    if ns >= 1e3:
        return f'{ns / 1e3:.2f} µs'
    return f'{ns:.0f} ns'


def fmt_ratio(r: float) -> str:
    return f'{r:.2f}×' if r >= 0.1 else f'{r:.2g}×'


def verdict(ratio: float | None) -> str:
    if ratio is None:
        return '—'
    if ratio >= 1.2:
        return 'faster'
    if ratio > 1 / 1.2:
        return 'about equal'
    return 'slower'


def summarize_case(rec: dict) -> dict:
    res = {}
    for e, runs in rec['runs'].items():
        ok = [r for r in runs if r.get('outcome') == 'ok']
        if not ok:
            first = runs[0] if runs else {}
            res[e] = {'outcome': first.get('outcome'), 'error': first.get('error') or first.get('stderr', '')[-300:]}
            continue
        vals = [r['ns'] for r in ok]
        digests = sorted({r['digest'] for r in ok})
        res[e] = {'outcome': 'ok' if len(ok) == len(runs) else 'partial', 'median_ns': statistics.median(vals),
                  'min_ns': min(vals), 'max_ns': max(vals), 'n': len(ok), 'digests': digests,
                  'load': [r['load_before'][0] for r in ok] + [r['load_after'][0] for r in ok],
                  'foreign': sorted({m for r in ok for m in r.get('foreign_modules', [])})}
    ref = res.get('ref', {})
    for e, s in res.items():
        if s.get('outcome') in ('ok', 'partial') and ref.get('outcome') in ('ok', 'partial'):
            s['speedup_vs_ref'] = ref['median_ns'] / s['median_ns']
            s['same_result_as_ref'] = s['digests'] == ref['digests']
    return res


def summarize(path: Path) -> None:
    raw = json.loads(path.read_text())
    loads = []
    print('| Case | pure-Python bdfparser | compat | native | compat vs ref | native vs ref | same result as ref (compat / native) |')
    print('| --- | ---: | ---: | ---: | --- | --- | --- |')
    for name, rec in raw['cases'].items():
        s = summarize_case(rec)
        cells = []
        for e in ENGINES:
            x = s.get(e, {})
            if x.get('outcome') in ('ok', 'partial'):
                cell = fmt_time(x['median_ns'])
                if x['outcome'] == 'partial':
                    cell += f" ({x['n']} ok)"
                loads += x['load']
            elif x:
                cell = f"**{x.get('outcome')}**"
            else:
                cell = '—'
            cells.append(cell)
        ratios = []
        for e in ('compat', 'native'):
            x = s.get(e, {})
            r = x.get('speedup_vs_ref')
            ratios.append(f'{fmt_ratio(r)} {verdict(r)}' if r is not None else '—')
        same = []
        for e in ('compat', 'native'):
            x = s.get(e, {})
            v = x.get('same_result_as_ref')
            same.append('—' if v is None else ('yes' if v else 'no'))
            if x.get('foreign'):
                same[-1] += f" (foreign modules {x['foreign']})"
        print(f"| `{name}` ({rec['kind']}): {rec['label']} | {cells[0]} | {cells[1]} | {cells[2]} | {ratios[0]} | {ratios[1]} | {' / '.join(same)} |")
    print()
    for name, rec in raw['cases'].items():
        s = summarize_case(rec)
        for e, x in s.items():
            if x.get('outcome') not in ('ok', 'partial'):
                print(f'- `{name}` {e}: {x.get("outcome")}: {x.get("error")}')
            elif x['max_ns'] > 1.25 * x['min_ns']:
                print(f'- `{name}` {e}: spread min {fmt_time(x["min_ns"])} / max {fmt_time(x["max_ns"])}')
    if loads:
        print(f'\nload average (1 min) at child start/end: min {min(loads):.2f}, median {statistics.median(loads):.2f}, max {max(loads):.2f}')


def paired_ratios(rec: dict, engine: str) -> list[float]:
    """ref/engine per repetition (engines of one repetition ran back to back)."""
    ref = {r['rep']: r for r in rec['runs'].get('ref', []) if r.get('outcome') == 'ok'}
    out = []
    for r in rec['runs'].get(engine, []):
        if r.get('outcome') == 'ok' and r['rep'] in ref:
            out.append(ref[r['rep']]['ns'] / r['ns'])
    return out


def compare(paths: list[Path]) -> None:
    """Speedup versus ref per run (ratio of medians) and the range of the
    per-repetition paired ratios over all runs."""
    raws = [json.loads(p.read_text()) for p in paths]
    names = list(raws[0]['cases'])
    runs_hdr = ' / '.join(f'run {i + 1}' for i in range(len(raws)))
    print(f'| Case | compat vs ref ({runs_hdr}) | paired range | native vs ref ({runs_hdr}) | paired range | verdict compat / native |')
    print('| --- | --- | --- | --- | --- | --- |')
    for name in names:
        cells = []
        verdicts = []
        for e in ('compat', 'native'):
            per_run, paired = [], []
            for raw in raws:
                rec = raw['cases'].get(name)
                if rec is None:
                    per_run.append(None)
                    continue
                per_run.append(summarize_case(rec).get(e, {}).get('speedup_vs_ref'))
                paired += paired_ratios(rec, e)
            if all(r is None for r in per_run):
                cells += ['—', '—']
                verdicts.append('n/a')
                continue
            cells.append(' / '.join('—' if r is None else fmt_ratio(r) for r in per_run))
            cells.append(f'{fmt_ratio(min(paired))[:-1]}–{fmt_ratio(max(paired))}' if paired else '—')
            vs = {verdict(r) for r in per_run if r is not None}
            verdicts.append(vs.pop() if len(vs) == 1 else 'mixed: ' + ', '.join(sorted(vs)))
        print(f"| `{name}` | {cells[0]} | {cells[1]} | {cells[2]} | {cells[3]} | {' / '.join(verdicts)} |")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    r = sub.add_parser('run')
    r.add_argument('--out', required=True)
    r.add_argument('--reps', type=int, default=5)
    r.add_argument('--cases')
    r.add_argument('--engines', default=','.join(ENGINES))
    r.add_argument('--python', default=sys.executable, help='interpreter with fontpixel installed')
    r.add_argument('--ref-src', required=True, help='writable copy of reference/bdfparser/src')
    r.add_argument('--cpus', default='', help='taskset CPU list for every child (same for all engines)')
    r.add_argument('--no-scope', action='store_true', help='do not wrap children in systemd-run --scope')
    r.add_argument('--resume', action='store_true')
    r.add_argument('--note', default='')
    w = sub.add_parser('worker')
    w.add_argument('engine', choices=ENGINES)
    w.add_argument('case')
    w.add_argument('--ref-src')
    s = sub.add_parser('summarize')
    s.add_argument('raw')
    c = sub.add_parser('compare')
    c.add_argument('raws', nargs='+')
    sub.add_parser('cases')
    args = ap.parse_args()
    if args.cmd == 'run':
        if args.reps < 5:
            print('warning: fewer than 5 repetitions is not a formal run', file=sys.stderr)
        run(args)
    elif args.cmd == 'worker':
        print(json.dumps(worker(args.engine, args.case, Path(args.ref_src) if args.ref_src else None)))
    elif args.cmd == 'summarize':
        summarize(Path(args.raw))
    elif args.cmd == 'compare':
        compare([Path(p) for p in args.raws])
    else:
        for name, (kind, label, _) in CASES.items():
            print(f'{name:24s} {kind:6s} {label}')


if __name__ == '__main__':
    main()
