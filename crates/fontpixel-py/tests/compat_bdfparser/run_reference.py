#!/usr/bin/env python3
"""Run bdfparser's own (unmodified) test suite against `fontpixel.compat.bdfparser`.

    python run_reference.py [--reference DIR] [--json OUT] [--slow] [-- PYTEST_ARGS...]

- `--reference`: bdfparser checkout (default: `$BDFPARSER_REFERENCE`, else the first
  `reference/bdfparser` found walking up from this file).
- The reference `tests/` directory is copied to a temporary directory (pytest
  needs a writable location; the files are not modified), the `shim/` directory
  is put first on `sys.path` and `PYTHONPATH` so `import bdfparser` resolves to
  the compatibility API, and pytest runs in this process.
- `--slow` sets `BDFPARSER_SLOW=1` (the reference's long tests).
- A JSON summary (counts, every test id with its outcome, failure messages) is
  written to `--json` (default: `reference_results.json` next to this file).

Requires `fontpixel` to be importable (e.g. `maturin develop`) and the
reference's test dependencies (`pytest`, `fonttools`, `pillow`).
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import shutil
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
SHIM = HERE / 'shim'


def find_reference(explicit: str | None) -> Path:
    if explicit:
        return Path(explicit).resolve()
    env = os.environ.get('BDFPARSER_REFERENCE')
    if env:
        return Path(env).resolve()
    for parent in HERE.parents:
        cand = parent / 'reference' / 'bdfparser'
        if (cand / 'tests').is_dir():
            return cand
    raise SystemExit('reference bdfparser not found; pass --reference or set BDFPARSER_REFERENCE')


class Recorder:
    """pytest plugin collecting one outcome per test id."""

    def __init__(self) -> None:
        self.results: dict[str, dict] = {}

    def pytest_runtest_logreport(self, report) -> None:  # noqa: ANN001
        entry = self.results.setdefault(report.nodeid, {'outcome': 'passed'})
        if report.when == 'call' or report.outcome != 'passed':
            if report.skipped:
                outcome = 'xfailed' if hasattr(report, 'wasxfail') else 'skipped'
            elif report.failed:
                outcome = 'error' if report.when != 'call' else 'failed'
            else:
                outcome = 'passed'
            if entry['outcome'] in ('passed',) or outcome in ('failed', 'error'):
                entry['outcome'] = outcome
            if report.failed:
                text = str(report.longrepr)
                entry['message'] = text.strip().splitlines()[-1][:500] if text.strip() else ''
            elif report.skipped and isinstance(report.longrepr, tuple):
                entry['message'] = str(report.longrepr[2])[:300]

    def pytest_collectreport(self, report) -> None:  # noqa: ANN001
        if report.failed:
            self.results[report.nodeid or '<collection>'] = {
                'outcome': 'error',
                'message': str(report.longrepr).strip().splitlines()[-1][:500],
            }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument('--reference')
    ap.add_argument('--json', default=str(HERE / 'reference_results.json'))
    ap.add_argument('--slow', action='store_true')
    ap.add_argument('pytest_args', nargs='*')
    args = ap.parse_args()

    ref = find_reference(args.reference)
    ref_src = str(ref / 'src')
    sys.path[:] = [p for p in sys.path if Path(p or '.').resolve() != Path(ref_src).resolve()]
    sys.path.insert(0, str(SHIM))
    os.environ['PYTHONPATH'] = os.pathsep.join(
        [str(SHIM)] + [p for p in os.environ.get('PYTHONPATH', '').split(os.pathsep) if p]
    )
    if args.slow:
        os.environ['BDFPARSER_SLOW'] = '1'

    import bdfparser  # noqa: E402

    assert Path(bdfparser.__file__).resolve().is_relative_to(SHIM), bdfparser.__file__
    import fontpixel  # noqa: E402
    import pytest  # noqa: E402

    work = Path(tempfile.mkdtemp(prefix='bdfparser-reference-'))
    cwd = os.getcwd()
    try:
        shutil.copytree(ref / 'tests', work / 'tests')
        # the reference tests open fonts by paths relative to the project root
        os.chdir(work)
        recorder = Recorder()
        t0 = time.monotonic()
        code = pytest.main(
            [
                'tests',
                '-p',
                'no:cacheprovider',
                '--rootdir',
                str(work),
                '-o',
                'markers=slow: long-running tests',
                '-q',
                '-rfE',
                *args.pytest_args,
            ],
            plugins=[recorder],
        )
        elapsed = time.monotonic() - t0
    finally:
        os.chdir(cwd)
        shutil.rmtree(work, ignore_errors=True)

    counts: dict[str, int] = {}
    for r in recorder.results.values():
        counts[r['outcome']] = counts.get(r['outcome'], 0) + 1
    tests = {
        k.replace(str(work) + os.sep, ''): v for k, v in sorted(recorder.results.items())
    }
    summary = {
        'reference': str(ref),
        'fontpixel': fontpixel.__version__,
        'bdfparser_api': bdfparser.__version__,
        'python': platform.python_version(),
        'slow': bool(args.slow),
        'pytest_exit_code': int(code),
        'seconds': round(elapsed, 1),
        'counts': counts,
        'tests': tests,
    }
    Path(args.json).write_text(json.dumps(summary, indent=1, ensure_ascii=False) + '\n')
    print(f'summary: {counts} -> {args.json}')
    return int(code)


if __name__ == '__main__':
    sys.exit(main())
