#!/usr/bin/env python3
"""Five-run bounded multicore full-family API export with output/RSS evidence.

Each run reads all 20 frozen BDFs, prepares four plans and writes 84 artifacts.
Compilation, independent correctness checks and hashing are outside the timer.
Use --runs 1 only for explicitly labelled engineering probes.
"""
import argparse
import datetime
import hashlib
import json
import os
import platform
import statistics
import subprocess
import time
from pathlib import Path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--binary', type=Path, required=True)
    p.add_argument('--bdf-dir', type=Path, required=True)
    p.add_argument('--work', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--jobs', type=int, default=8)
    p.add_argument('--quality', type=int, default=11)
    p.add_argument('--threads', type=int, default=1)
    p.add_argument('--runs', type=int, default=5)
    p.add_argument('--dot-reuse', choices=['literal', 'rows', 'patterns'], default='patterns')
    a = p.parse_args()
    assert a.runs >= 1
    a.work.mkdir(parents=True, exist_ok=True)
    a.output.parent.mkdir(parents=True, exist_ok=True)
    binary_hash = sha(a.binary)
    result = {
        'partial': True,
        'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'scope': '84 outputs: 20 BDF copies + 20 PCF + 20 OTB + 8 TTF + 8 OTF + 8 WOFF2; '
                 'native API example with shared immutable plans and bounded multicore jobs; '
                 'all input reads, parsing, planning, encoding and output I/O included',
        'shared_host_caveat': 'Shared virtualized host; five unrelated glyph workers may '
                              'run intermittent light Python render/checkpoint commands. '
                              'No other sustained software build or CPU-heavy job was '
                              'scheduled during accepted timed intervals. No CPU pinning '
                              'or exclusive-core guarantee.',
        'machine': {'platform': platform.platform(), 'logical_cpus': os.cpu_count(),
                    'cpu': next(s.split(':', 1)[1].strip() for s in
                                Path('/proc/cpuinfo').read_text().splitlines()
                                if s.startswith('model name')),
                    'memory': Path('/proc/meminfo').read_text().splitlines()[0]},
        'binary_sha256': binary_hash, 'jobs': a.jobs, 'quality': a.quality,
        'compression_threads': a.threads, 'dot_reuse': a.dot_reuse, 'requested_runs': a.runs,
        'is_five_run_measurement': a.runs == 5,
        'input_hashes': {p.name: sha(p) for p in sorted(a.bdf_dir.glob('*.bdf'))},
        'runs': []}
    assert len(result['input_hashes']) == 20
    first = None
    for i in range(a.runs):
        assert sha(a.binary) == binary_hash
        prefix = a.output.with_suffix('')
        stdout = Path(str(prefix) + f'-run{i + 1}.stdout')
        stderr = Path(str(prefix) + f'-run{i + 1}.stderr')
        load = os.getloadavg()
        start = time.perf_counter()
        with stdout.open('w') as out, stderr.open('w') as err:
            run = subprocess.Popen([str(a.binary), str(a.bdf_dir), str(a.work),
                                    str(a.jobs), str(a.quality), str(a.threads), 'phase2', a.dot_reuse],
                                   stdout=out, stderr=err)
            _, status, usage = os.wait4(run.pid, 0)
            run.returncode = os.waitstatus_to_exitcode(status)
        elapsed = time.perf_counter() - start
        assert run.returncode == 0, (run.returncode, stderr.read_text())
        driver = json.loads(stdout.read_text())
        resources = {'maximum_rss_kib': usage.ru_maxrss,
                     'user_seconds': usage.ru_utime, 'system_seconds': usage.ru_stime}
        outputs = {str(p.relative_to(a.work)): {'bytes': p.stat().st_size, 'sha256': sha(p)}
                   for kind in ['bdf', 'pcf', 'otb', 'ttf', 'otf', 'woff2']
                   for p in sorted((a.work / kind).iterdir()) if p.is_file()}
        assert len(outputs) == 84
        if first is None:
            first = outputs
            result['outputs'] = outputs
        else:
            assert outputs == first, (i + 1, 'unstable bytes')
        result['runs'].append({'wall_seconds': elapsed, 'load_start': load,
                               'load_end': os.getloadavg(), **resources, 'driver': driver})
        a.output.write_text(json.dumps(result, indent=2) + '\n')
        print(i + 1, round(elapsed, 3), 's', resources['maximum_rss_kib'], 'KiB', flush=True)
    result.update(partial=False, all_outputs_byte_identical=True,
                  binary_unchanged=sha(a.binary) == binary_hash,
                  median_seconds=statistics.median(r['wall_seconds'] for r in result['runs']),
                  peak_rss_kib=max(r['maximum_rss_kib'] for r in result['runs']),
                  total_bytes=sum(v['bytes'] for v in first.values()),
                  woff2_bytes=sum(v['bytes'] for k, v in first.items() if k.startswith('woff2/')))
    a.output.write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
