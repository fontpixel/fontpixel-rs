#!/usr/bin/env python3
"""Original Tong/fontTools full vector and identical resident-sfnt WOFF2 calls."""
import argparse
import gc
import importlib.metadata
import io
import json
from pathlib import Path
import time
from phase6_reference import load_tong, parse_bdf, single_plan
from phase6_support import digest, invoke


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('mode', choices=['vector-native', 'vector-fonttools', 'woff2-fonttools', 'prepare-dot'])
    p.add_argument('--reference-root', type=Path, required=True); p.add_argument('--source', type=Path, required=True)
    p.add_argument('--binary', type=Path); p.add_argument('--output', type=Path, required=True)
    a = p.parse_args(); a.output.mkdir(parents=True, exist_ok=True)
    if any(a.output.iterdir()): raise ValueError('fresh output directory required')
    metrics = {}; calls = []; warmups = 0
    if a.mode == 'woff2-fonttools':
        from fontTools.ttLib import TTFont
        from fontTools.ttLib.woff2 import WOFF2FlavorData
        raw = a.source.read_bytes()
        def encode():
            font = TTFont(io.BytesIO(raw), recalcTimestamp=False)
            font.flavor = 'woff2'; font.flavorData = WOFF2FlavorData(transformedTables={'glyf', 'loca', 'hmtx'})
            buffer = io.BytesIO(); font.save(buffer)
            value = buffer.getvalue(); font.close()
            return value
        gc.collect(); warmup = encode(); del warmup; gc.collect(); warmups = 1
        start = time.perf_counter_ns(); encoded = encode(); metrics['woff2_encode'] = time.perf_counter_ns() - start
        target = a.output / 'font.woff2'; target.write_bytes(encoded)
        profile = 'same resident sfnt; parse/transform/Brotli encode; quality 11/font mode, one worker; input/output I/O excluded'
    else:
        ref = load_tong(a.reference_root); ref.set_size('Large')
        shape = 'dot' if a.mode == 'prepare-dot' else 'square'
        target = a.output / 'font.ttf'
        names = ref.names(ref.family_of('prop', 'Large'), shape.capitalize(), '1.0')
        if a.mode == 'vector-native':
            if a.binary is None: raise ValueError('--binary required for native vector command')
            command = [a.binary, 'variants', '--base', a.source.resolve(), '--family', names['familyName'],
                '--vendor', 'TONG', '--font-version', '1.0', '--copyright', names['copyright'],
                '--license', names['licenseDescription'], '--license-url', names['licenseInfoURL'],
                '--shape', shape, '--report', 'json', '-o', target.resolve()]
            start = time.perf_counter_ns(); calls.append(invoke(command, a.output / 'native'))
            metrics['vector_pipeline'] = time.perf_counter_ns() - start
        else:
            start = time.perf_counter_ns()
            plan = single_plan(ref, parse_bdf(a.source))
            font = ref.build_ttf('prop', shape, '1.0', plan, 'Large')
            font['head'].created = font['head'].modified = 2082844800
            font.recalcTimestamp = False; font.save(target); font.close(); del font, plan
            metrics['vector_pipeline'] = time.perf_counter_ns() - start
        profile = 'complete BDF read, original-compatible plan/metadata, full outlines and output I/O; native command retains complete report; original reference emits its own available diagnostics'
    print(json.dumps({'schema': 2, 'mode': a.mode, 'metrics_ns': metrics, 'warmup_calls': warmups,
        'measured_calls': 1, 'calls': calls, 'input_sha256': digest(a.source), 'output_sha256': digest(target),
        'output_bytes': target.stat().st_size, 'profile': profile,
        'fonttools_version': importlib.metadata.version('fonttools'),
        'formal_eligible': a.mode != 'prepare-dot'}))

if __name__ == '__main__': main()
