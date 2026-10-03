#!/usr/bin/env python3
"""Validate selectable dot representations without weakening baseline fidelity."""
import argparse
import gc
import hashlib
import json
import tempfile
from pathlib import Path

from glyf import load_reference
from variants import compare_full, invoke
from phase2_vectors import common_args


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ['binary', 'bdf-dir', 'source-root', 'reference', 'baseline-benchmark', 'work', 'output']:
        p.add_argument('--' + name, type=Path, required=True)
    a = p.parse_args()
    a.work.mkdir(parents=True, exist_ok=True)
    root = Path(__file__).resolve().parents[2]
    ref = load_reference(root.parent)
    ref.ROOT = a.source_root
    expected = json.loads(a.baseline_benchmark.read_text())['profiles'][0]['outputs']
    report = {'partial': True, 'results': []}
    for size in ['Large', 'Small']:
        for spacing in ['prop', 'mono']:
            prefix = 'TongPixelSans' + ('Mono' if spacing == 'mono' else '') + '14' + size
            paths = {r: a.bdf_dir / (prefix + '-' + r + '.bdf')
                     for r in ['SC', 'TC', 'JP', 'KR', 'Latin']}
            args = common_args(ref, paths, size, spacing)
            ref.set_size(size)
            plan = ref.plan(spacing, size)
            with tempfile.TemporaryDirectory(prefix='policy-', dir=a.work) as temp:
                output = Path(temp) / (prefix + '-dot.ttf')
                for policy in ['literal', 'rows']:
                    invoke(a.binary, *args, '--shape', 'dot', '--dot-reuse', policy, '-o', output)
                    digest = hashlib.sha256(output.read_bytes()).hexdigest()
                    if policy == 'literal':
                        assert digest == expected['ttf/' + output.name]['sha256'], (prefix, 'literal changed')
                        result = {'case': prefix + '-literal', 'sha256': digest,
                                  'bytes': output.stat().st_size,
                                  'byte_identical_to_complete_baseline_oracle': True}
                    else:
                        result = compare_full(output, a.reference / 'ttf' / (prefix + '-Dot.ttf'),
                                              plan, 'dot', prefix + '-rows')
                    report['results'].append(result)
                    a.output.write_text(json.dumps(report, indent=2) + '\n')
                    print(prefix, policy, 'PASS', flush=True)
            del plan
            gc.collect()
    report['partial'] = False
    a.output.write_text(json.dumps(report, indent=2) + '\n')


if __name__ == '__main__':
    main()
