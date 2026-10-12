#!/usr/bin/env python3
"""Compare a bdfparser / bdfparser-js reference-suite run with its expected failures.

    python3 tools/check_reference_results.py RESULTS.json EXPECTED_FAILURES.txt

RESULTS.json is the summary written by
crates/fontpixel-py/tests/compat_bdfparser/run_reference.py or
crates/fontpixel-wasm/tests/compat_bdfparser/run_reference.mjs (`--json`).
EXPECTED_FAILURES.txt lists one test id per line (`#` comments). Exit status 1
when a test fails or errors that is not listed, or when nothing passed; a
listed test that now passes or no longer exists is reported but accepted (the
list then needs updating). Standard library only.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    results = json.loads(Path(argv[1]).read_text(encoding='utf-8'))
    expected = {
        line.strip()
        for line in Path(argv[2]).read_text(encoding='utf-8').splitlines()
        if line.strip() and not line.lstrip().startswith('#')
    }
    tests: dict[str, dict] = results['tests']
    bad = {tid for tid, t in tests.items() if t['outcome'] in ('failed', 'error')}
    passed = sum(1 for t in tests.values() if t['outcome'] == 'passed')
    counts = ', '.join(f'{k} {v}' for k, v in sorted(results.get('counts', {}).items()))
    print(f'{argv[1]}: {counts}')
    status = 0
    for tid in sorted(bad - expected):
        message = (tests[tid].get('message') or '').strip().splitlines()
        print(f'NEW FAILURE: {tid}' + (f'\n    {message[0][:300]}' if message else ''))
        status = 1
    for tid in sorted(expected & set(tests)):
        if tid not in bad:
            print(f'note: expected failure now {tests[tid]["outcome"]}: {tid}')
    for tid in sorted(expected - set(tests)):
        print(f'note: expected failure not in this run: {tid}')
    if passed == 0:
        print('no test passed; the suite did not run')
        status = 1
    print(f'{len(bad & expected)} expected failures, {len(bad - expected)} unexpected')
    return status


if __name__ == '__main__':
    sys.exit(main(sys.argv))
