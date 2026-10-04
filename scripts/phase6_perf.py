#!/usr/bin/env python3
"""UNTESTED RECOVERY of phase-6 recorder, reconstructed from visible source.

Not asserted byte-identical to the unavailable historical recorder. Restore as
scripts/phase6_perf.py only after review and regression tests. A formal label
requires the coordinator's explicit fresh quiet authorization. No historical raw
observations may be regenerated from summaries or filled with invented values.
"""
from __future__ import annotations
import argparse
import datetime as dt
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys
import threading
import time
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''): h.update(block)
    return h.hexdigest()


def snapshot(paths):
    files = set()
    for raw in paths:
        path = Path(raw).resolve()
        if path.is_file(): files.add(path)
        elif path.is_dir():
            files.update(p for p in path.rglob('*') if p.is_file() and '__pycache__' not in p.parts)
        else: raise FileNotFoundError(path)
    return {str(p): {'sha256': digest(p), 'bytes': p.stat().st_size} for p in sorted(files)}


def verify_snapshot(records):
    for name, expected in records.items():
        path = Path(name)
        if not path.is_file() or path.stat().st_size != expected['bytes'] or digest(path) != expected['sha256']:
            raise ValueError('frozen file changed or absent: ' + name)
    return len(records)


def source_paths(root):
    paths = [root / 'Cargo.toml', root / 'Cargo.lock', root / 'rust-toolchain.toml']
    for directory, patterns in [('crates', ('*.rs', 'Cargo.toml')), ('benches', ('phase6*',)),
                                ('benches/support', ('*.rs',)), ('scripts', ('phase6_perf*',)),
                                ('tests/crosscheck', ('*.py',))]:
        for pattern in patterns: paths.extend(p for p in (root / directory).rglob(pattern) if p.is_file())
    return paths


def utc(): return dt.datetime.now(dt.timezone.utc).isoformat()


def machine():
    info = Path('/proc/cpuinfo').read_text(); cores = set()
    for section in info.split('\n\n'):
        fields = dict(line.split(':', 1) for line in section.splitlines() if ':' in line)
        fields = {k.strip(): v.strip() for k, v in fields.items()}
        if 'core id' in fields: cores.add((fields.get('physical id'), fields['core id']))
    def version(command):
        p = subprocess.run(command, capture_output=True, text=True)
        return {'command': command, 'exit': p.returncode, 'output': p.stdout + p.stderr}
    return {'platform': platform.platform(), 'cpu': next(line.split(':', 1)[1].strip() for line in info.splitlines() if line.startswith('model name')),
            'logical_cpus': os.cpu_count(), 'physical_cores_reported': len(cores) or None,
            'affinity': sorted(os.sched_getaffinity(0)), 'memory': Path('/proc/meminfo').read_text(),
            'python': sys.version, 'rustc': version(['rustc', '-Vv']), 'node': version(['node', '--version']),
            'project_release_profile': 'thin LTO; codegen-units=1',
            'original_tong_release_profile': 'thin LTO; original Cargo default codegen-units',
            'rustflags': os.environ.get('RUSTFLAGS'), 'cargo_encoded_rustflags': os.environ.get('CARGO_ENCODED_RUSTFLAGS'),
            'observed_utc': utc(), 'shared_host': True}


def process_tree_rss(root_pid):
    processes = {}
    for path in Path('/proc').iterdir():
        if not path.name.isdecimal(): continue
        try:
            fields = dict(line.split(':', 1) for line in (path / 'status').read_text().splitlines() if ':' in line)
            processes[int(path.name)] = (int(fields['PPid']), int(fields.get('VmRSS', '0 kB').split()[0]))
        except (OSError, ValueError, KeyError): continue
    selected = {root_pid}
    while True:
        expanded = selected | {pid for pid, (parent, _) in processes.items() if parent in selected}
        if expanded == selected: break
        selected = expanded
    return sum(rss for pid, (_, rss) in processes.items() if pid in selected)


def run_command(command, stem, *, env=None, cwd=None):
    # IMPORTANT: append, never Path.with_suffix: dotted case IDs must not alias.
    stdout, stderr = Path(str(stem) + '.stdout'), Path(str(stem) + '.stderr')
    stdout.parent.mkdir(parents=True, exist_ok=True)
    for path in (stdout, stderr):
        if os.path.lexists(path): raise FileExistsError('Refusing to overwrite raw stream: ' + str(path))
    with stdout.open('xb') as out, stderr.open('xb') as err:
        start = time.perf_counter_ns()
        proc = subprocess.Popen(list(map(str, command)), stdout=out, stderr=err, env=env, cwd=cwd)
        stop = threading.Event(); rss_samples = []
        def sample_rss():
            while not stop.is_set():
                rss_samples.append(process_tree_rss(proc.pid)); stop.wait(0.02)
        sampler = threading.Thread(target=sample_rss, daemon=True); sampler.start()
        _, status, usage = os.wait4(proc.pid, 0)
        proc.returncode = os.waitstatus_to_exitcode(status)
        elapsed = time.perf_counter_ns() - start
        stop.set(); sampler.join()
    return {'command': list(map(str, command)), 'cwd': str(cwd) if cwd else None, 'exit': proc.returncode,
            'process_wall_ns': elapsed, 'user_seconds': usage.ru_utime, 'system_seconds': usage.ru_stime,
            'peak_rss_kib': usage.ru_maxrss if sys.platform != 'darwin' else usage.ru_maxrss / 1024,
            'sampled_tree_peak_rss_kib': max(rss_samples, default=0), 'rss_sample_count': len(rss_samples),
            'rss_sample_period_seconds': 0.02, 'stdout': str(stdout.resolve()), 'stderr': str(stderr.resolve()),
            'stdout_sha256': digest(stdout), 'stderr_sha256': digest(stderr)}


def verify_logs(records):
    seen = set()
    for record in records:
        for stream in ('stdout', 'stderr'):
            path = Path(record[stream]).resolve()
            if path in seen: raise ValueError('duplicate invocation stream: ' + str(path))
            seen.add(path)
            if digest(path) != record[stream + '_sha256']: raise ValueError('raw stream SHA mismatch: ' + str(path))
    return {'commands': len(records), 'unique_streams': len(seen), 'all_sha256_verified': True}


def archive_outputs(directory, archive):
    files = sorted(p for p in directory.rglob('*') if p.is_file()); result = {}
    with zipfile.ZipFile(archive, 'x', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as z:
        for path in files:
            name = str(path.relative_to(directory))
            result[name] = {'bytes': path.stat().st_size, 'sha256': digest(path)}; z.write(path, name)
    return {'path': str(archive.resolve()), 'sha256': digest(archive), 'files': result}


def verify_archive(record):
    archive = record['outputs']
    if digest(archive['path']) != archive['sha256']: raise ValueError('archive SHA mismatch')
    with zipfile.ZipFile(archive['path']) as z:
        if len(z.namelist()) != len(archive['files']) or set(z.namelist()) != set(archive['files']):
            raise ValueError('archive inventory mismatch')
        for name, expected in archive['files'].items():
            h, size = hashlib.sha256(), 0
            with z.open(name) as f:
                for chunk in iter(lambda: f.read(1024*1024), b''): h.update(chunk); size += len(chunk)
            if {'sha256': h.hexdigest(), 'bytes': size} != expected: raise ValueError('archive member SHA mismatch: ' + name)
    return len(archive['files'])


def metrics_from_log(record):
    # Reconstructed equivalent logic; original exact source was unavailable.
    metrics = {'process_wall_ns': record['process_wall_ns']}
    if record['internal_json']:
        raw = json.loads(Path(record['stdout']).read_text())['metrics_ns']
        if 'process_wall_ns' in raw: raise ValueError('reserved metric collision')
        metrics.update(raw)
    if any(isinstance(n, bool) or not isinstance(n, (int, float)) or not math.isfinite(n) or n < 0 for n in metrics.values()):
        raise ValueError('invalid timing metric')
    return metrics


def summarize(records, runs):
    # Reconstructed equivalent logic; original exact source was unavailable.
    groups = {}
    for record in records: groups.setdefault((record['case'], record['engine']), []).append(record)
    result = []
    for (case, engine), observations in sorted(groups.items()):
        if len(observations) != runs or sorted(r['repetition'] for r in observations) != list(range(1, runs + 1)):
            raise ValueError('missing/duplicate observations')
        keys = set(observations[0]['metrics_ns'])
        if any(set(r['metrics_ns']) != keys for r in observations): raise ValueError('metric keys changed')
        metrics = {}
        for key in keys:
            values = [r['metrics_ns'][key] for r in observations]
            metrics[key] = {'median': statistics.median(values), 'min': min(values), 'max': max(values)}
        rss = [r['peak_rss_kib'] for r in observations]
        result.append({'case': case, 'engine': engine, 'metrics_ns': metrics,
                       'peak_rss_kib': max(rss), 'median_peak_rss_kib': statistics.median(rss)})
    return result


def freeze(args):
    config = json.loads(args.config.read_text())
    unique = [(c['id'], c['engine']) for c in config['cases']]
    if len(unique) != len(set(unique)): raise ValueError('duplicate case/engine')
    for case in config['cases']:
        for value in (case['id'], case['engine']):
            if not value or any(c not in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-' for c in value):
                raise ValueError('unsafe case/engine ID')
    result = {'schema': 1, 'created_utc': utc(), 'config': config,
              'files': snapshot(source_paths(ROOT) + [args.config] + [Path(p) for p in config['identity_paths']]),
              'machine': machine(), 'formal': False, 'purpose': 'frozen preparation only'}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open('x') as out: json.dump(result, out, indent=2); out.write('\n')


def measure(args):
    manifest = json.loads(args.manifest.read_text())
    if args.formal and (not args.quiet_window_confirmed or not args.environment_note.strip()):
        raise ValueError('formal timing requires coordinator-confirmed quiet window and environment note')
    verify_snapshot(manifest['files']); args.directory.mkdir(parents=True, exist_ok=False)
    records = []; runs = 5 if args.formal else 1
    env = {**os.environ, 'PYTHONDONTWRITEBYTECODE': '1', 'TMPDIR': str(args.directory.resolve() / 'tmp')}
    Path(env['TMPDIR']).mkdir(); start = utc()
    for case in manifest['config']['cases']:
        for repeat in range(1, runs + 1):
            verify_snapshot(manifest['files'])
            ident = case['id'] + '-' + case['engine'] + '-' + str(repeat)
            work = args.directory / 'work' / ident; work.mkdir(parents=True)
            command = [arg.replace('{output}', str(work.resolve())) for arg in case['command']]
            load_start = list(os.getloadavg())
            record = run_command(command, args.directory / 'raw' / ident, env=env, cwd=ROOT)
            record.update(case=case['id'], engine=case['engine'], repetition=repeat,
                          internal_json=case.get('internal_json', True), load_start=load_start, load_end=list(os.getloadavg()))
            with (args.directory / 'progress.jsonl').open('a') as out: out.write(json.dumps(record) + '\n')
            if record['exit']: raise RuntimeError('benchmark child failed: ' + json.dumps(record))
            record['metrics_ns'] = metrics_from_log(record)
            archive = args.directory / 'outputs' / (ident + '.zip'); archive.parent.mkdir(exist_ok=True)
            record['outputs'] = archive_outputs(work, archive); verify_archive(record)
            shutil.rmtree(work)  # Only this runner's freshly created, verified-archived output.
            records.append(record)
            with (args.directory / 'records.jsonl').open('a') as out: out.write(json.dumps(record) + '\n')
            print(ident, round(record['process_wall_ns'] / 1e9, 3), 's', flush=True)
    verify_snapshot(manifest['files'])
    result = {'schema': 1, 'formal': args.formal, 'runs': runs, 'started_utc': start, 'finished_utc': utc(),
              'manifest': str(args.manifest.resolve()), 'manifest_sha256': digest(args.manifest),
              'environment_note': args.environment_note, 'machine': machine(), 'records': records,
              'raw_log_verification': verify_logs(records), 'summary': summarize(records, runs)}
    with (args.directory / 'result.json').open('x') as out: json.dump(result, out, indent=2); out.write('\n')


def audit(args):
    result = json.loads(args.result.read_text())
    if digest(result['manifest']) != result['manifest_sha256']: raise ValueError('manifest SHA mismatch')
    manifest = json.loads(Path(result['manifest']).read_text()); count = verify_snapshot(manifest['files'])
    logs = verify_logs(result['records']); outputs = sum(verify_archive(r) for r in result['records'])
    for record in result['records']:
        if record['metrics_ns'] != metrics_from_log(record): raise ValueError('metrics differ from raw stream')
    if summarize(result['records'], result['runs']) != result['summary']: raise ValueError('summary differs')
    print(json.dumps({'status': 'pass', 'frozen_files': count, 'output_files': outputs, 'raw_logs': logs,
                      'independently_recomputed_summary_rows': len(result['summary']), 'result_sha256': digest(args.result)}))


def main():
    p = argparse.ArgumentParser(description=__doc__); sub = p.add_subparsers(dest='action', required=True)
    q = sub.add_parser('freeze'); q.add_argument('--config', type=Path, required=True); q.add_argument('--output', type=Path, required=True)
    q = sub.add_parser('measure'); q.add_argument('--manifest', type=Path, required=True); q.add_argument('--directory', type=Path, required=True)
    q.add_argument('--formal', action='store_true'); q.add_argument('--quiet-window-confirmed', action='store_true'); q.add_argument('--environment-note', default='')
    q = sub.add_parser('audit'); q.add_argument('--result', type=Path, required=True)
    a = p.parse_args(); {'freeze': freeze, 'measure': measure, 'audit': audit}[a.action](a)

if __name__ == '__main__': main()
