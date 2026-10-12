#!/usr/bin/env bash
# Build release abi3 manylinux2014 wheels and the source distribution of
# fontpixel-py with maturin + zig (no Docker), check them with auditwheel and
# `twine check`, install the wheel into fresh CPython venvs (and optionally the
# sdist, which pip builds from source) and run the pytest suite against the
# installed package. Nothing is published. CI builds the release wheels with
# maturin-action instead (.github/workflows/release.yml).
#
# Usage: scripts/build_wheels.sh [OUT_DIR]
#   OUT_DIR           wheel/sdist directory (default: ../local-work/wheels)
# Environment:
#   TOOLS_VENV        venv with maturin, ziglang, auditwheel and twine
#                     (default: ../local-work/py-bindings-venv)
#   TARGETS           Rust targets (default: "x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu";
#                     a target whose Rust std is not installed is reported and skipped)
#   TEST_PYTHONS      interpreters for the installed-wheel tests
#                     (default: ~/.pyenv/versions/3.10.20/bin/python ~/.pyenv/versions/3.12.13/bin/python)
#   SDIST_TEST_PYTHON interpreter that also installs the sdist (built from source
#                     with the Rust toolchain on PATH) and runs the suite
#   TEST_REQUIREMENTS test-only packages for every test venv (default: pytest;
#                     add mypy to run the typing tests too)
#   WORK_DIR          scratch directory for test venvs and logs (default: OUT_DIR/../wheel-tests)
#   CARGO_TARGET_DIR  as usual (default: ../local-work/target-wheels)
#   SKIP_TESTS=1      build and audit only
# The suite's optional inputs come from the environment as usual:
# FONTPIXEL_FROZEN_CLI (a release `fontpixel` binary for the command-line
# identity tests) and FONTPIXEL_RUSTDOC_TARGET (tools/bindings/gate.sh output).
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
for tool in maturin auditwheel twine; do
  [ -x "$tools/bin/$tool" ] || { echo "missing $tools/bin/$tool (pip install maturin ziglang auditwheel twine)" >&2; exit 2; }
done
"$tools/bin/python" -m ziglang version >/dev/null || { echo "missing ziglang in $tools" >&2; exit 2; }
# maturin packages everything under python/, including bytecode left there by
# an editable (`maturin develop`) install; refuse instead of shipping it.
stale="$(find "$repo/crates/fontpixel-py/python" \( -name __pycache__ -o -name '*.pyc' -o -name '*.so' -o -name '*.pyd' \) -print)"
if [ -n "$stale" ]; then
  printf 'bytecode or a develop-mode extension under crates/fontpixel-py/python would be packaged; remove it first:\n%s\n' "$stale" >&2
  exit 2
fi
if ls "$out"/fontpixel-*.whl "$out"/fontpixel-*.tar.gz >/dev/null 2>&1; then
  echo "$out already holds fontpixel wheels or sdists; move them away first" >&2
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

echo "== sdist" | tee -a "$work/build.log"
maturin sdist -m "$manifest" --out "$out" >>"$work/build.log" 2>&1
sdist="$(ls "$out"/fontpixel-*.tar.gz)"
printf '%s  %s bytes  sha256 %s\n' "$(basename "$sdist")" "$(stat -c %s "$sdist")" "$(sha256sum "$sdist" | cut -d' ' -f1)"
"$tools/bin/python" - "$sdist" <<'EOF'
import sys
import tarfile

names = [n.split('/', 1)[1] for n in tarfile.open(sys.argv[1]).getnames() if '/' in n]
bad = [n for n in names if n.endswith('.pyc') or '__pycache__' in n or '/tests/' in n
       or n.startswith(('fixtures/', 'tests/'))]
need = ['PKG-INFO', 'pyproject.toml', 'LICENSE', 'Cargo.lock', 'python/fontpixel/py.typed']
missing = [n for n in need if n not in names]
print(f'    {len(names)} members')
if bad or missing:
    sys.exit(f'unexpected sdist contents: unwanted {bad[:10]}, missing {missing}')
EOF

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
# the extension itself; fontpixel/_native/*.pyi are its type stubs
mods = [n for n in names if n.endswith(('.so', '.pyd'))]
need = ['fontpixel/py.typed', 'fontpixel/_native/__init__.pyi', 'fontpixel/compat/bdfparser/__init__.py']
missing = [n for n in need if n not in names]
licenses = [n for n in names if '.dist-info/licenses/' in n]
print(f'    {len(names)} members; extension module {mods}; {len(licenses)} license files')
if bad or len(mods) != 1 or missing or not licenses:
    sys.exit(f'unexpected wheel contents: bytecode {bad}, extension modules {mods}, '
             f'missing {missing}, license files {licenses}')
EOF
done
echo "== twine check"
twine check --strict "$out"/fontpixel-*

[ "${SKIP_TESTS:-0}" = 1 ] && exit 0

# Installed-package tests: the suite runs from the repository's tests directory
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
whl="$(ls "$out"/fontpixel-*-manylinux*_"$host"*.whl 2>/dev/null | head -n1)"
[ -n "$whl" ] || { echo "no $host wheel to test" >&2; exit 1; }
requirements="${TEST_REQUIREMENTS:-pytest}"
status=0
# test_suite LABEL PYTHON PACKAGE: fresh venv, install PACKAGE, run the suite.
test_suite() {
  local label="$1" py="$2" package="$3" ver venv rc=0
  ver="$("$py" -c 'import sys; print("%d.%d" % sys.version_info[:2])')"
  venv="$work/venv-$label-$ver"
  if [ -e "$venv" ]; then echo "test venv $venv exists; remove it first" >&2; exit 2; fi
  "$py" -m venv "$venv"
  # shellcheck disable=SC2086 # a word list of requirements
  if ! "$venv/bin/pip" install --disable-pip-version-check $requirements "$package" \
      >"$work/install-$label-$ver.log" 2>&1; then
    echo "install of $(basename "$package") FAILED on Python $ver (log $work/install-$label-$ver.log)"
    status=1
    return
  fi
  echo "== pytest on Python $ver with $(basename "$package")"
  (cd "$work" && PYTHONDONTWRITEBYTECODE=1 PYTHONPATH="$plugin_dir" "$venv/bin/python" -m pytest \
      -p installed_wheel_check -p no:cacheprovider -q -rs "$repo/crates/fontpixel-py/tests") \
      >"$work/pytest-$label-$ver.log" 2>&1 || rc=$?
  grep -m1 '^installed wheel:' "$work/pytest-$label-$ver.log" || true
  tail -n 1 "$work/pytest-$label-$ver.log"
  [ "$rc" = 0 ] || { echo "pytest exit $rc on Python $ver (log $work/pytest-$label-$ver.log)"; status=1; }
}
for py in $pythons; do
  test_suite wheel "$py" "$whl"
done
if [ -n "${SDIST_TEST_PYTHON:-}" ]; then
  test_suite sdist "$SDIST_TEST_PYTHON" "$sdist"
fi
exit "$status"
