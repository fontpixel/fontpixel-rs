#!/usr/bin/env python3
"""Complete 84-artifact Tong workflows, one invocation per fresh process.

Native API, actual CLI batch and original Rust exporter are distinct profiles.
All generated reports/child diagnostics are retained. Native full-report file I/O
is included and must not be equated with historical omitted-report timings.
"""
import argparse
import concurrent.futures
import json
import os
from pathlib import Path
import shutil
import time
from phase6_reference import family_plan, load_module, load_tong
from phase6_support import batch_header, inventory, invoke

KINDS = ['bdf', 'pcf', 'otb', 'ttf', 'otf', 'woff2']


def expected_fonts():
    names = set()
    for size in ['Large', 'Small']:
        for spacing in ['', 'Mono']:
            stem = f'TongPixelSans{spacing}14{size}'
            for region in ['SC', 'TC', 'JP', 'KR', 'Latin']:
                for kind in ['bdf', 'pcf', 'otb']: names.add(f'{kind}/{stem}-{region}.{kind}')
            for shape in ['square', 'dot']:
                for kind in ['ttf', 'otf', 'woff2']: names.add(f'{kind}/{stem}-{shape}.{kind}')
    return names


def jobs_toml(bdf, output, ref, variant_limit=0):
    lines = ['version = 1']
    for path in sorted(bdf.glob('*.bdf')):
        for kind in ['pcf', 'otb']:
            lines.extend(['', '[[tasks]]', 'id = ' + json.dumps(path.stem + '.' + kind),
                'kind = "convert"', 'inputs = ' + json.dumps([str(path.resolve())]), 'from = "bdf"',
                'to = ' + json.dumps(kind), 'output = ' + json.dumps(str(output / kind / (path.stem + '.' + kind)))])
    for size in ['Large', 'Small']:
        for spacing in ['prop', 'mono']:
            stem = 'TongPixelSans' + ('Mono' if spacing == 'mono' else '') + '14' + size
            paths = {r: str((bdf / (stem + '-' + r + '.bdf')).resolve()) for r in ['SC', 'TC', 'JP', 'KR', 'Latin']}
            family = ref.family_of(spacing, size); names = ref.names(family, 'Square', '1.0')
            for shape in ['square', 'dot']:
                for kind in ['ttf', 'otf', 'woff2']:
                    lines.extend(['', '[[tasks]]', 'id = ' + json.dumps(stem + '.' + shape + '.' + kind),
                        'kind = "variants"', 'base = ' + json.dumps(paths['SC']), 'family = ' + json.dumps(family),
                        'output = ' + json.dumps(str(output / kind / (stem + '-' + shape + '.' + kind))),
                        # Batch expands its language map lexically. Use the
                        # original tags so TC is admitted before JP and KR,
                        # matching reference/native API alternative deduplication.
                        'languages = { ' + ', '.join(json.dumps(k) + ' = ' + json.dumps(v) for k, v in [('ZHT,ZHH', paths['TC']), ('ja', paths['JP']), ('ko', paths['KR'])]) + ' }',
                        'scripts = { "latn,grek,cyrl" = ' + json.dumps(paths['Latin']) + ' }',
                        '[tasks.options]', 'shape = ' + json.dumps(shape), 'vendor = "TONG"', 'font_version = "1.0"',
                        'copyright = ' + json.dumps(names['copyright']), 'license = ' + json.dumps(names['licenseDescription']),
                        'license_url = ' + json.dumps(names['licenseInfoURL'])])
                    if variant_limit: lines.append('max_total_bytes = ' + str(variant_limit))
    return '\n'.join(lines) + '\n'


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('engine', choices=['native', 'batch', 'reference'])
    p.add_argument('--binary', type=Path, required=True); p.add_argument('--reference-root', type=Path, required=True)
    p.add_argument('--bdf', type=Path, required=True); p.add_argument('--output', type=Path, required=True)
    p.add_argument('--jobs', type=int); p.add_argument('--quality', type=int, default=11)
    p.add_argument('--threads', type=int, default=1); p.add_argument('--dot-reuse', choices=['literal', 'rows', 'patterns'], default='patterns')
    p.add_argument('--variant-max-total-bytes', type=int, default=0)
    a = p.parse_args(); a.output.mkdir(parents=True, exist_ok=True)
    if any(a.output.iterdir()): raise ValueError('fresh output directory required')
    out = a.output.resolve() / 'export'; sources = sorted(a.bdf.glob('*.bdf'))
    if len(sources) != 20: raise ValueError('full Tong workflow requires exactly 20 BDF sources')
    expected_sources = {Path(name).name for name in expected_fonts() if name.startswith('bdf/')}
    if {p.name for p in sources} != expected_sources: raise ValueError('unexpected Tong BDF matrix')
    ref = load_tong(a.reference_root) if a.engine != 'native' else None
    metrics = {}; calls = []; start = time.perf_counter_ns()
    if a.engine == 'native':
        default = a.jobs is None and a.quality == 11 and a.threads == 1 and a.dot_reuse == 'patterns'
        command = [a.binary, a.bdf.resolve(), out]
        if not default:
            command += [str(a.jobs or min(os.cpu_count() or 1, 8)), str(a.quality), str(a.threads), '1.0', a.dot_reuse]
        calls.append(invoke(command, a.output / 'native'))
        metrics['pipeline_wall'] = time.perf_counter_ns() - start
        driver = json.loads((a.output / 'native.stdout').read_text())
        for key in ['read_seconds', 'bitmap_seconds', 'plan_seconds', 'vector_seconds', 'wall_seconds']:
            metrics['api_' + key.removesuffix('_seconds')] = round(driver[key] * 1e9)
        profile = {'kind': 'native-api-full-reports-v2', 'driver': driver, 'default_options': default}
    elif a.engine == 'batch':
        if a.quality != 11 or a.threads != 1 or a.dot_reuse != 'patterns':
            raise ValueError('batch default profile does not accept API-only tuning arguments')
        for kind in KINDS: (out / kind).mkdir(parents=True)
        for source in sources: shutil.copyfile(source, out / 'bdf' / source.name)
        taskfile = a.output / 'jobs.toml'; taskfile.write_text(jobs_toml(a.bdf, out, ref, a.variant_max_total_bytes))
        command = [a.binary, 'batch', taskfile.resolve()]
        if a.jobs is not None: command += ['--jobs', str(a.jobs)]
        calls.append(invoke(command, a.output / 'batch'))
        metrics['pipeline_wall'] = time.perf_counter_ns() - start
        header = batch_header(a.output / 'batch.stdout', 64)
        profile = {'kind': 'actual-batch-cli-full-reports-v2', 'default_options': a.jobs is None and not a.variant_max_total_bytes,
                   'batch': header, 'variant_max_total_bytes_override': a.variant_max_total_bytes,
                   'codec_options': 'CLI defaults: quality 11, WOFF2 threads 2, patterns reuse; model/report/pending budgets unchanged'}
    else:
        if a.quality != 11 or a.threads != 1 or a.dot_reuse != 'patterns' or a.variant_max_total_bytes:
            raise ValueError('reference workflow only supports its original export policy')
        for kind in KINDS: (out / kind).mkdir(parents=True)
        prepare = load_module('phase6_original_prepare', a.reference_root / 'reference/tong-pixel-sans/tools/export-rs/prepare.py')
        jobs = []
        for size in ['Large', 'Small']:
            for spacing in ['prop', 'mono']:
                plan = family_plan(ref, a.bdf, size, spacing)
                for shape in ['square', 'dot']:
                    jobs.append(prepare.prepare_job(plan, spacing, size, shape, '1.0', a.output))
        plan_path = a.output / 'input.json'; prepare.write_plan(plan_path, out, jobs)
        del jobs, plan
        metrics['reference_prepare'] = time.perf_counter_ns() - start
        env = {**os.environ, 'PYTHONDONTWRITEBYTECODE': '1'}
        if a.jobs is not None: env['RAYON_NUM_THREADS'] = str(a.jobs)
        stage = time.perf_counter_ns(); calls.append(invoke([a.binary, '--plan', plan_path.resolve()], a.output / 'reference-vectors', env=env))
        metrics['reference_vectors'] = time.perf_counter_ns() - stage
        stage = time.perf_counter_ns()
        for source in sources: shutil.copyfile(source, out / 'bdf' / source.name)
        def pcf(source):
            return invoke(['bdftopcf', '-o', out / 'pcf' / (source.stem + '.pcf'), source.resolve()], a.output / 'pcf-logs' / source.stem)
        with concurrent.futures.ThreadPoolExecutor(max_workers=a.jobs or min(os.cpu_count() or 1, 8)) as pool:
            calls.extend(pool.map(pcf, sources))
        calls.append(invoke([a.binary, out, '--only', 'otb', '--bdf-dir', a.bdf.resolve(), '--font-version', '1.0'], a.output / 'reference-bitmaps', env=env))
        # Original exporter capitalizes shape filenames. This declared filename
        # normalization is included in pipeline time and touches only new output.
        for kind in ['ttf', 'otf', 'woff2']:
            for path in (out / kind).iterdir():
                stem = path.stem.removesuffix('-Square').removesuffix('-Dot')
                shape = 'square' if path.stem.endswith('-Square') else 'dot'
                destination = path.with_name(stem + '-' + shape + path.suffix)
                if destination != path:
                    if destination.exists(): raise FileExistsError(destination)
                    path.rename(destination)
        metrics['reference_bitmaps'] = time.perf_counter_ns() - stage
        metrics['pipeline_wall'] = time.perf_counter_ns() - start
        profile = {'kind': 'original-export-rs-plus-original-Python-planning', 'rayon_jobs_override': a.jobs,
                   'pcf_jobs': a.jobs or min(os.cpu_count() or 1, 8), 'diagnostics': 'complete original stdout/stderr; original has no native structured Report equivalent'}
    artifacts = inventory(out)
    fonts = {name: value for name, value in artifacts.items() if name.split('/')[0] in KINDS}
    if set(fonts) != expected_fonts(): raise ValueError({'missing': sorted(expected_fonts()-set(fonts)), 'extra': sorted(set(fonts)-expected_fonts())})
    if a.engine == 'native' and len([n for n in artifacts if n.startswith('reports/')]) != 84:
        raise ValueError('native full-report profile requires all 84 report files')
    print(json.dumps({'schema': 2, 'metrics_ns': metrics, 'profile': profile, 'calls': calls,
        'font_artifacts': fonts, 'report_and_auxiliary_artifacts': {k:v for k,v in artifacts.items() if k not in fonts},
        'artifact_count': 84, 'export_count': 64, 'source_bdf_copies': 20,
        'scope': 'complete same frozen BDF matrix; source rebuilding excluded for every engine; reads/plans/encodes/font and diagnostic I/O included; independent hashing/validation outside pipeline timer'}))

if __name__ == '__main__': main()
