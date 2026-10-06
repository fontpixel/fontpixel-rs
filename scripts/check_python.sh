#!/usr/bin/env bash
# Python bindings gate: build the extension into a CPython 3.10 venv, run the
# pytest suite, then the binding coverage gate (`python` required).
# Usage: scripts/check_python.sh [VENV]   (default: ../local-work/py310-venv)
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
venv="${1:-$(dirname "$PWD")/local-work/py310-venv}"
if [ ! -x "$venv/bin/python" ]; then
  echo "missing venv $venv (create it with CPython 3.10 and pip install maturin pytest mypy)" >&2
  exit 2
fi
# The coverage tools need Python >= 3.11 (tomllib); keep the system interpreter.
PYTHON="$(command -v python3)"
export PYTHON
export VIRTUAL_ENV="$venv" PATH="$venv/bin:$PATH"
(cd crates/fontpixel-py && maturin develop --release --locked)
python -m pytest crates/fontpixel-py/tests -q
tools/bindings/gate.sh python
