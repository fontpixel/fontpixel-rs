#!/usr/bin/env python3
"""Five-run whole Tong export benchmark through the released native CLI.

84 artifacts = 20 BDF copies + 20 PCFs + 20 OTBs + 8*(TTF, OTF, WOFF2).
The timed input is the complete frozen BDF set, not the glyph-source builder.
Compilation, correctness oracles and output hashing are outside timed intervals.
"""
import argparse
import concurrent.futures
import datetime
import hashlib
import json
import os
import platform
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tests/crosscheck'))
from glyf import load_reference
from phase2_vectors import common_args


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def commands(binary, ref, bdf, out, quality, threads):
    result = []
    for source in sorted(bdf.glob('*.bdf')):
        for kind in ['pcf', 'otb']:
            target = out / kind / (source.stem + '.' + kind)
            result.append((kind, [str(binary), 'convert', str(source), '-o', str(target),
                                  '--to', kind]))
    for size in ['Large', 'Small']:
        for spacing in ['prop', 'mono']:
            prefix = 'TongPixelSans' + ('Mono' if spacing == 'mono' else '') + '14' + size
            paths = {r: bdf / (prefix + '-' + r + '.bdf')
                     for r in ['SC', 'TC', 'JP', 'KR', 'Latin']}
            common = common_args(ref, paths, size, spacing)
            for shape in ['square', 'dot']:
                for kind in ['ttf', 'otf', 'woff2']:
                    target = out / kind / (prefix + '-' + shape + '.' + kind)
                    result.append((kind, list(map(str, [binary, *common, '--shape', shape,
                                   '-o', target, '--woff2-quality', quality,
                                   '--woff2-threads', threads]))))
    assert len(result) == 64
    return result


def execute(job):
    kind, command = job
    start = time.perf_counter()
    proc = subprocess.run(command, capture_output=True, text=True)
    seconds = time.perf_counter() - start
    if proc.returncode:
        raise RuntimeError((command, proc.returncode, proc.stdout, proc.stderr))
    return {'format': kind, 'seconds': seconds}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--bdf-dir', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--profile', action='append', choices=['default', 'intra4', 'quality8', 'quality5', 'serial5'])
    args = parser.parse_args()
    args.work.mkdir(parents=True, exist_ok=True)
    for kind in ['bdf', 'pcf', 'otb', 'ttf', 'otf', 'woff2']:
        (args.work / kind).mkdir(exist_ok=True)
    ref = load_reference(ROOT.parent)
    profiles = [('default', 11, 1, 8), ('intra4', 11, 4, 2),
                ('quality8', 8, 1, 8), ('quality5', 5, 1, 8), ('serial5', 5, 1, 1)]
    if args.profile:
        profiles = [p for p in profiles if p[0] in args.profile]
    binary_hash = sha(args.binary)
    result = {'partial': True, 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'binary_sha256': binary_hash,
              'machine': {'platform': platform.platform(), 'cpus': os.cpu_count(),
                          'cpu': next(line.split(':', 1)[1].strip() for line in
                                      Path('/proc/cpuinfo').read_text().splitlines()
                                      if line.startswith('model name')),
                          'memory': Path('/proc/meminfo').read_text().splitlines()[0]},
              'scope': '20 BDF copies + 20 PCF + 20 OTB + 8 TTF + 8 OTF + 8 WOFF2; '
                       'full CLI parsing/variant planning/encoding/I/O; frozen BDF inputs; '
                       'external bounded process pool pending phase-6 batch command',
              'input_hashes': {p.name: sha(p) for p in sorted(args.bdf_dir.glob('*.bdf'))},
              'profiles': []}
    assert len(result['input_hashes']) == 20
    for label, quality, threads, jobs in profiles:
        group = {'label': label, 'quality': quality, 'compression_threads': threads,
                 'jobs': jobs, 'runs': []}
        result['profiles'].append(group)
        tasks = commands(args.binary, ref, args.bdf_dir, args.work, quality, threads)
        first = None
        for repetition in range(5):
            assert sha(args.binary) == binary_hash
            started = time.perf_counter()
            for source in sorted(args.bdf_dir.glob('*.bdf')):
                shutil.copyfile(source, args.work / 'bdf' / source.name)
            with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
                timings = list(pool.map(execute, tasks))
            elapsed = time.perf_counter() - started
            outputs = {str(p.relative_to(args.work)): {'bytes': p.stat().st_size, 'sha256': sha(p)}
                       for kind in ['bdf', 'pcf', 'otb', 'ttf', 'otf', 'woff2']
                       for p in sorted((args.work / kind).iterdir()) if p.is_file()}
            assert len(outputs) == 84
            if first is None:
                first = outputs
                group['outputs'] = outputs
            else:
                assert outputs == first, (label, repetition, 'unstable bytes')
            run = {'wall_seconds': elapsed, 'task_seconds_sum_by_format':
                   {kind: sum(t['seconds'] for t in timings if t['format'] == kind)
                    for kind in ['pcf', 'otb', 'ttf', 'otf', 'woff2']}}
            group['runs'].append(run)
            args.output.write_text(json.dumps(result, indent=2) + '\n')
            print(label, repetition + 1, round(elapsed, 3), 's', flush=True)
        group['median_seconds'] = statistics.median(r['wall_seconds'] for r in group['runs'])
        group['total_bytes'] = sum(v['bytes'] for v in first.values())
        group['woff2_bytes'] = sum(v['bytes'] for k, v in first.items() if k.startswith('woff2/'))
        group['all_five_outputs_byte_identical'] = True
        # Retain only the eight small webfonts per strategy for independent verification.
        saved = args.work / ('saved-' + label)
        saved.mkdir(exist_ok=True)
        for source in (args.work / 'woff2').iterdir():
            shutil.copyfile(source, saved / source.name)
        args.output.write_text(json.dumps(result, indent=2) + '\n')
    result['partial'] = False
    result['binary_unchanged'] = sha(args.binary) == binary_hash
    args.output.write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
