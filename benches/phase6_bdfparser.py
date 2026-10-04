#!/usr/bin/env python3
"""Full-input original Python single-call comparisons, reconstructed profile v2.

One warmup and one observation per operation; five fresh processes are external.
Forced collections before and after warmup are outside timers. In-call automatic
GC remains included. Output/report materialization is outside inner timers.
"""
import gc
import json
from pathlib import Path
import struct
import sys
import time
import warnings


def raster(path, bitmap):
    width, height = bitmap.width(), bitmap.height(); stride = (width + 7) // 8
    packed = bytearray(stride * height)
    for y, row in enumerate(bitmap.bindata):
        for x, pixel in enumerate(row):
            if pixel == '1': packed[y * stride + x // 8] |= 0x80 >> (x % 8)
            elif pixel != '0': raise ValueError('unexpected nonbinary raster label')
    path.write_bytes(b'P6R1' + struct.pack('>II', width, height) + packed)


def main():
    if len(sys.argv) != 4: raise SystemExit('phase6_bdfparser.py COPIED_REFERENCE_BDFPARSER INPUT OUTPUT')
    reference, source, out = map(Path, sys.argv[1:])
    protected = Path(__file__).resolve().parents[2] / 'reference'
    if reference.resolve().is_relative_to(protected.resolve()):
        raise ValueError('execute reference code only from its writable local-work copy')
    sys.dont_write_bytecode = True; sys.path.insert(0, str(reference / 'src'))
    from bdfparser import Font, Bitmap
    out.mkdir(parents=True, exist_ok=True)
    if any(out.iterdir()): raise ValueError('output directory must be empty')
    data = source.read_bytes(); metrics = {}
    def once(name, operation):
        gc.collect(); warmup = operation(); del warmup; gc.collect()
        start = time.perf_counter_ns(); value = operation()
        metrics[name] = time.perf_counter_ns() - start
        return value
    with warnings.catch_warnings(record=True) as captured:
        warnings.simplefilter('always')
        font = once('parse', lambda: Font().load_bytes(data))
        (out / 'compat.bdf').write_bytes(once('write_bdf', lambda: font.tobdf().encode('utf-8')))
        (out / 'compat.pcf').write_bytes(once('write_pcf', font.topcf))
        (out / 'compat.ttf').write_bytes(once('write_ttf', font.tottf))
        text = '永' * 2000
        bitmap = once('render_screen', lambda: font.draw(text, mode=0, linelimit=80 * font.headers['fbbx']))
        raster(out / 'screen.raster', bitmap); del bitmap
        bitmap = once('drawall_mode1', lambda: font.drawall(mode=1, linelimit=512))
        raster(out / 'drawall.raster', bitmap); del bitmap
        small = Bitmap([''.join('1' if (x*13+y*7)%11 < 4 else '0' for x in range(64)) for y in range(64)])
        bitmap = once('crop', lambda: small.clone().crop(31, 31, 3, 5))
        raster(out / 'crop.raster', bitmap); del bitmap
        bitmap = once('concat', lambda: small.clone().concat(small, direction=1, align=1, offset=0))
        raster(out / 'concat.raster', bitmap); del bitmap
    (out / 'warnings.report.json').write_text(json.dumps([str(w.message) for w in captured], ensure_ascii=False) + '\n')
    print(json.dumps({'schema': 2, 'profile': 'reconstructed-full-input-v2', 'engine': 'python',
        'python': sys.version, 'glyphs': len(font.glyphs) + len(font.unencoded), 'input_bytes': len(data),
        'metrics_ns': metrics, 'warmup_calls': 1, 'measured_calls': 1, 'atlas_mode': 1,
        'gc': 'forced before/after warmup outside timer; automatic in-call GC included',
        'small_bitmap': '64x64,(13*x+7*y)%11<4; crop31x31@(3,5); concat right/end/gap0; immutable input via timed clone'}))

if __name__ == '__main__': main()
