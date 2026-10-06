#!/usr/bin/env bash
# Build release abi3 manylinux2014 wheels of fontpixel-py with maturin + zig
# (no Docker), check them with auditwheel, install each into fresh CPython
# venvs and run the pytest suite against the installed wheel. Nothing is
# published.
#
# Usage: scripts/build_wheels.sh [OUT_DIR]
#   OUT_DIR           wheel directory (default: ../local-work/wheels)
# Environment:
#   TOOLS_VENV        venv with maturin, ziglang and auditwheel
#                     (default: ../local-work/py-bindings-venv)
#   TARGETS           Rust targets (default: "x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu";
#                     a target whose Rust std is not installed is reported and skipped)
#   TEST_PYTHONS      interpreters for the installed-wheel tests
#                     (default: ~/.pyenv/versions/3.10.20/bin/python ~/.pyenv/versions/3.12.13/bin/python)
#   WORK_DIR          scratch directory for test venvs and logs (default: OUT_DIR/../wheel-tests)
#   CARGO_TARGET_DIR  as usual (default: ../local-work/target-wheels)
#   SKIP_TESTS=1      build and audit only
set -euo pipefail
repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# local-work sits next to the main checkout (also when run from a git worktree)
main_checkout="$(dirname "$(git -C "$repo" rev-parse --path-format=absolute --git-common-dir)")"
work_root="$(dirname "$main_checkout")/local-work"
out="${1:-$work_root/wheels}"
tools="${TOOLS_VENV:-$work_root/py-bindings-venv}"
targets="${TARGETS:-x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu}"
pythons="${TEST_PYTHONS:-$HOME/.pyenv/versions/3.10.20/bin/python $HOME/.pyenv/versions/3.12.13/bin/python}"
work="${WORK_DIR:-$(dirname "$out")/wheel-tests}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$work_root/target-wheels}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-6}"
manifest="$repo/crates/fontpixel-py/Cargo.toml"

case "$out" in "$repo"|"$repo"/*) echo "refusing to write wheels inside the repository: $out" >&2; exit 2;; esac
for tool in maturin auditwheel; do
  [ -x "$tools/bin/$tool" ] || { echo "missing $tools/bin/$tool (pip install maturin ziglang auditwheel)" >&2; exit 2; }
done
"$tools/bin/python" -m ziglang version >/dev/null || { echo "missing ziglang in $tools" >&2; exit 2; }
# maturin packages everything under python/, including bytecode left there by
# an editable (`maturin develop`) install; refuse instead of shipping it.
stale="$(find "$repo/crates/fontpixel-py/python" \( -name __pycache__ -o -name '*.pyc' \) -print)"
if [ -n "$stale" ]; then
  printf 'bytecode under crates/fontpixel-py/python would be packaged; remove it first:\n%s\n' "$stale" >&2
  exit 2
fi
if ls "$out"/fontpixel-*.whl >/dev/null 2>&1; then
  echo "$out already holds fontpixel wheels; move them away first" >&2
  exit 2
fi
mkdir -p "$out" "$work"
export PATH="$tools/bin:$PATH"
installed_targets="$(rustup target list --installed --toolchain "$(cd "$repo" && rustup show active-toolchain | cut -d' ' -f1)")"

built=()
for target in $targets; do
  if ! grep -qx "$target" <<<"$installed_targets"; then
    echo "== $target: Rust std not installed (rustup target add $target); skipped" | tee -a "$work/build.log"
    continue
  fi
  echo "== building $target" | tee -a "$work/build.log"
  if maturin build --release --locked --zig --compatibility manylinux2014 \
      --target "$target" -m "$manifest" --out "$out" >>"$work/build.log" 2>&1; then
    built+=("$target")
  else
    echo "== $target: build FAILED (see $work/build.log)" | tee -a "$work/build.log"
  fi
done

echo "== wheels"
for whl in "$out"/fontpixel-*.whl; do
  [ -e "$whl" ] || continue
  printf '%s  %s bytes  sha256 %s\n' "$(basename "$whl")" "$(stat -c %s "$whl")" "$(sha256sum "$whl" | cut -d' ' -f1)"
  auditwheel show "$whl" 2>&1 | sed 's/^/    /'
  "$tools/bin/python" - "$whl" <<'EOF'
import sys
import zipfile

names = zipfile.ZipFile(sys.argv[1]).namelist()
bad = [n for n in names if n.endswith('.pyc') or '__pycache__' in n]
mods = [n for n in names if n.startswith('fontpixel/_native')]
print(f'    {len(names)} members; extension module {mods}')
if bad or len(mods) != 1:
    sys.exit(f'unexpected wheel contents: bytecode {bad}, extension modules {mods}')
EOF
done

[ "${SKIP_TESTS:-0}" = 1 ] && exit 0

# Installed-wheel tests: the suite runs from the repository's tests directory
# (its fixtures resolve paths relative to it), but `fontpixel` must come from
# the venv's site-packages; the plugin below aborts the session otherwise.
plugin_dir="$work/plugin"
mkdir -p "$plugin_dir"
cat >"$plugin_dir/installed_wheel_check.py" <<'EOF'
import sys
import sysconfig
from pathlib import Path


def pytest_sessionstart(session):
    import fontpixel
    import fontpixel._native as native
    site = Path(sysconfig.get_paths()["purelib"]).resolve()
    for mod in (fontpixel, native):
        path = Path(mod.__file__).resolve()
        if site not in path.parents:
            raise SystemExit(f"{mod.__name__} imported from {path}, not from {site}")
    print(f"\ninstalled wheel: {Path(native.__file__).resolve()} (Python {sys.version.split()[0]})")
EOF
host="$(uname -m)"
status=0
for py in $pythons; do
  ver="$("$py" -c 'import sys; print("%d.%d" % sys.version_info[:2])')"
  whl="$(ls "$out"/fontpixel-*-manylinux*_"$host"*.whl 2>/dev/null | head -n1)"
  [ -n "$whl" ] || { echo "no $host wheel to test" >&2; exit 1; }
  venv="$work/venv-$ver"
  if [ -e "$venv" ]; then echo "test venv $venv exists; remove it first" >&2; exit 2; fi
  "$py" -m venv "$venv"
  "$venv/bin/pip" install -q --disable-pip-version-check pytest "$whl"
  echo "== pytest on Python $ver with $(basename "$whl")"
  rc=0
  (cd "$work" && PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$plugin_dir" "$venv/bin/python" -m pytest \
      -p installed_wheel_check -p no:cacheprovider -q "$repo/crates/fontpixel-py/tests") \
      >"$work/pytest-$ver.log" 2>&1 || rc=$?
  grep -m1 '^installed wheel:' "$work/pytest-$ver.log" || true
  tail -n 1 "$work/pytest-$ver.log"
  [ "$rc" = 0 ] || { echo "pytest exit $rc on Python $ver (log $work/pytest-$ver.log)"; status=1; }
done
exit "$status"
