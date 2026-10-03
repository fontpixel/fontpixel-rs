#!/usr/bin/env python3
"""Five full original Tong Rust exports from the same frozen twenty BDFs.

Preparation uses the unchanged supplied Python plan/metadata functions. The
uncropped-source and BDF plans are independently proven identical. All planning,
JSON serialization, bitmap conversion and output I/O are inside the interval.
Glyph-source-to-BDF rebuilding is excluded for both native and reference engines.
"""
import argparse
import concurrent.futures
import hashlib
import importlib.util
import json
import os
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tests/crosscheck'))
from glyf import load_reference, parse_bdf


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, env=None):
    p = subprocess.run(list(map(str, command)), capture_output=True, text=True, env=env)
    assert p.returncode == 0, (command, p.stdout, p.stderr)
    return p.stdout + p.stderr


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--bdf-dir', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--jobs', type=int, default=8)
    parser.add_argument('--runs', type=int, default=5,
                        help='Use 1 for an explicitly non-benchmark smoke run; acceptance uses 5')
    args = parser.parse_args()
    args.work.mkdir(parents=True, exist_ok=True)
    out = args.work / 'out'
    for kind in ['bdf', 'pcf', 'otb', 'ttf', 'otf', 'woff2']:
        (out / kind).mkdir(parents=True, exist_ok=True)
    ref = load_reference(ROOT.parent)
    path = ROOT.parent / 'reference/tong-pixel-sans/tools/export-rs/prepare.py'
    spec = importlib.util.spec_from_file_location('tong_prepare_phase2', path)
    prepare = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prepare)
    result = {'partial': True, 'engine': 'unchanged original Tong Rust exporter',
              'binary_sha256': sha(args.binary), 'jobs': args.jobs,
              'input_hashes': {p.name: sha(p) for p in sorted(args.bdf_dir.glob('*.bdf'))},
              'scope': '84 outputs from frozen BDFs; Python reference planning and metadata '
                       'preparation, native original Rust vectors/WOFF2/OTB, bdftopcf; full I/O',
              'runs': []}
    result['requested_runs'] = args.runs
    assert args.runs >= 1
    for repetition in range(args.runs):
        started = time.perf_counter()
        jobs = []
        for size in ['Large', 'Small']:
            for spacing in ['prop', 'mono']:
                prefix = 'TongPixelSans' + ('Mono' if spacing == 'mono' else '') + '14' + size
                faces = {r: parse_bdf(args.bdf_dir / (prefix + '-' + r + '.bdf'))
                         for r in ['SC', 'TC', 'JP', 'KR', 'Latin']}
                ref.build.faces = lambda root, size: [(r, faces[r], faces[r])
                                                       for r in ['SC', 'TC', 'JP', 'KR']]
                ref.build.latin_faces = lambda root, size: (faces['Latin'], faces['Latin'])
                plan = ref.plan(spacing, size)
                ref.set_size(size)
                for shape in ['square', 'dot']:
                    jobs.append(prepare.prepare_job(plan, spacing, size, shape,
                                                    'phase2', args.work))
        plan_path = args.work / 'input.json'
        prepare.write_plan(plan_path, out, jobs)
        prepared = time.perf_counter() - started
        del jobs, faces, plan
        vector_start = time.perf_counter()
        env = {**os.environ, 'RAYON_NUM_THREADS': str(args.jobs)}
        log = run([args.binary, '--plan', plan_path], env=env)
        vectors = time.perf_counter() - vector_start
        bitmap_start = time.perf_counter()
        for source in sorted(args.bdf_dir.glob('*.bdf')):
            shutil.copyfile(source, out / 'bdf' / source.name)
        pcf_jobs = [['bdftopcf', '-o', out / 'pcf' / (source.stem + '.pcf'), source]
                    for source in sorted(args.bdf_dir.glob('*.bdf'))]
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
            list(pool.map(run, pcf_jobs))
        log += run([args.binary, out, '--only', 'otb', '--bdf-dir', args.bdf_dir,
                    '--font-version', 'phase2'], env=env)
        bitmaps = time.perf_counter() - bitmap_start
        elapsed = time.perf_counter() - started
        outputs = {str(p.relative_to(out)): {'bytes': p.stat().st_size, 'sha256': sha(p)}
                   for kind in ['bdf', 'pcf', 'otb', 'ttf', 'otf', 'woff2']
                   for p in sorted((out / kind).iterdir()) if p.is_file()}
        assert len(outputs) == 84
        result['runs'].append({'wall_seconds': elapsed, 'prepare_seconds': prepared,
                               'vectors_woff2_seconds': vectors, 'bitmaps_seconds': bitmaps,
                               'outputs': outputs})
        (args.work / f'run-{repetition + 1}.log').write_text(log)
        args.output.write_text(json.dumps(result, indent=2) + '\n')
        print('reference', repetition + 1, round(elapsed, 3), 's', flush=True)
    result['median_seconds'] = statistics.median(r['wall_seconds'] for r in result['runs'])
    result['partial'] = False
    result['total_bytes'] = sum(v['bytes'] for v in outputs.values())
    result['woff2_bytes'] = sum(v['bytes'] for k, v in outputs.items() if k.startswith('woff2/'))
    args.output.write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
