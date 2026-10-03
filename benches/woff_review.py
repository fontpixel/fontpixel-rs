#!/usr/bin/env python3
"""Retest outputs affected by the mandatory WOFF2 head-flag correction.

The original 29 groups remain historical candidate evidence. Each newly timed
group has five samples; unchanged outputs are regenerated once and byte-checked.
Run only after correctness/build/fuzz jobs have finished.
"""
import argparse
import datetime
import hashlib
import json
import os
import platform
import statistics
import subprocess
from pathlib import Path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--original-work', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.work.mkdir(parents=True, exist_ok=True)
    result = {
        'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'binary_sha256': sha(args.binary),
        'machine': {'platform': platform.platform(), 'cpus': os.cpu_count(),
                    'cpu': next(line.split(':', 1)[1].strip() for line in
                                Path('/proc/cpuinfo').read_text().splitlines()
                                if line.startswith('model name')),
                    'memory': Path('/proc/meminfo').read_text().splitlines()[0]},
        'results': [], 'byte_regressions': [],
    }

    def save():
        args.output.write_text(json.dumps(result, indent=2) + '\n')

    def invoke(*command):
        return json.loads(subprocess.check_output(
            [str(args.binary), *map(str, command)], text=True))

    for filename in ['Tong-SC-square.otf', 'Tong-SC-dot.ttf', 'Tong-Latin-dot.ttf']:
        source = args.original_work / filename
        linked = args.work / filename
        if not linked.exists():
            linked.symlink_to(source.resolve())
        strategies = [(11, 1, True)] if 'Latin' in filename else [
            (5, 1, True), (8, 1, True), (9, 1, True), (11, 1, True),
            (11, 2, True), (11, 4, True), (5, 1, False)]
        for quality, threads, transform in strategies:
            label = f'native-{source.stem}-q{quality}-t{threads}-x{int(transform)}'
            output = args.work / (label + '.woff2')
            changed = 'square' in filename or not transform
            data = invoke('bench' if changed else 'encode', 'woff2', source,
                          output, quality, threads, *([] if transform else ['null']))
            data.update(label=label, sha256=sha(output), input_sha256=sha(source))
            if changed:
                assert len(data['seconds']) == 5
                data['median_seconds'] = statistics.median(data['seconds'])
                result['results'].append(data)
            else:
                assert output.read_bytes() == (args.original_work / output.name).read_bytes()
                result['byte_regressions'].append({
                    'label': label, 'bytes': data['bytes'], 'sha256': data['sha256'],
                    'checks': 'exact candidate/final output equality; not a timing sample',
                })
            save()
            print(label, 'MEASURED' if changed else 'BYTE-IDENTICAL', data['bytes'], flush=True)
    paths = [args.work / 'Tong-SC-square.otf', args.work / 'Tong-SC-dot.ttf']
    for jobs in [1, 2, 4]:
        data = invoke('batch', 5, 1, jobs, *paths, *paths)
        assert len(data['seconds']) == 5
        data.update(label=f'native-four-fonts-q5-jobs{jobs}',
                    median_seconds=statistics.median(data['seconds']))
        result['results'].append(data)
        save()
        print(data['label'], data['median_seconds'], data['bytes'], flush=True)
    assert len({r['bytes'] for r in result['results'] if 'four-fonts' in r['label']}) == 1
    print('PASS', len(result['results']), 'five-sample groups;',
          len(result['byte_regressions']), 'unchanged-output checks', flush=True)


if __name__ == '__main__':
    main()
