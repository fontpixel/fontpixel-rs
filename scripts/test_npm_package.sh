#!/usr/bin/env bash
# Install a packed `fontpixel` npm tarball (scripts/build_npm.sh) into a fresh
# scratch project and test it there: the bare-specifier smoke test
# (crates/fontpixel-wasm/tests/package/smoke.mjs), the node:test suites against
# the installed package (root = full profile, optionally every profile
# directory), and `tsc --strict` over the installed declarations.
#
# Usage: scripts/test_npm_package.sh TARBALL WORK_DIR
# Environment:
#   NPM_TEST_ALL_PROFILES=1  also run the suites against profiles/<name>/
#   FONTPIXEL_CLI            native CLI for the byte-identity tests (as for
#                            the suites; they skip without it)
#   TSC                      TypeScript compiler (default: npm-install
#                            typescript@6.0.3 into WORK_DIR/tools)
set -euo pipefail
repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
tarball="$(realpath "${1:?usage: scripts/test_npm_package.sh TARBALL WORK_DIR}")"
work="${2:?usage: scripts/test_npm_package.sh TARBALL WORK_DIR}"
[ ! -e "$work" ] || { echo "$work exists; remove it first" >&2; exit 2; }
mkdir -p "$work/project"
work="$(cd "$work" && pwd)"
case "$work" in "$repo"|"$repo"/*) echo "refusing to install inside the repository: $work" >&2; exit 2;; esac
project="$work/project"
echo "== Node $(node --version), npm $(npm --version)"
(cd "$project" && printf '{ "name": "fontpixel-package-test", "private": true, "type": "module" }\n' >package.json &&
  npm install --no-audit --no-fund --loglevel=error "$tarball")
pkg="$project/node_modules/fontpixel"
mapfile -t profiles < <(cd "$pkg" && ls profiles 2>/dev/null || true)

echo "== smoke (bare specifiers)"
cp "$repo/crates/fontpixel-wasm/tests/package/smoke.mjs" "$project/smoke.mjs"
(cd "$project" && node smoke.mjs "${profiles[@]}")

# shellcheck source=scripts/wasm_suites.sh
. "$repo/scripts/wasm_suites.sh"
run_suites() { # run_suites DIR LABEL (the package root is the full build)
  echo "== node:test suites against $2"
  local tests=()
  mapfile -t tests < <(wasm_suites "$repo" "$([ "$2" = root ] && echo full || echo "$2")")
  FONTPIXEL_WASM_PKG="$1" node --test "${tests[@]}" >"$work/node-test-$2.log" 2>&1 || {
    grep -E -A20 '^ *(not ok|✖)' "$work/node-test-$2.log" | head -n 400
    tail -n 15 "$work/node-test-$2.log"; echo "node:test failed for $2 (log $work/node-test-$2.log)"; return 1; }
  grep -E '^# (tests|pass|fail|skipped|cancelled|todo)' "$work/node-test-$2.log" | tr '\n' ' '; echo
}
status=0
run_suites "$pkg" root || status=1
if [ "${NPM_TEST_ALL_PROFILES:-0}" = 1 ]; then
  for profile in "${profiles[@]}"; do run_suites "$pkg/profiles/$profile" "$profile" || status=1; done
fi

echo "== tsc --strict over the installed declarations"
tsc="${TSC:-}"
if [ -z "$tsc" ]; then
  npm install --prefix "$work/tools" --no-audit --no-fund --loglevel=error typescript@6.0.3 >/dev/null
  tsc="$work/tools/node_modules/.bin/tsc"
fi
cp "$repo/crates/fontpixel-wasm/tests/typing/smoke.ts" "$project/smoke.ts"
for resolution in NodeNext Bundler; do
  module=ESNext; [ "$resolution" = NodeNext ] && module=NodeNext
  cat >"$project/tsconfig.json" <<JSON
{
  "compilerOptions": {
    "strict": true, "noEmit": true, "target": "ES2022",
    "module": "$module", "moduleResolution": "$resolution",
    "lib": ["ES2022", "DOM", "ESNext.Disposable"], "types": [], "skipLibCheck": false
  },
  "files": ["smoke.ts"]
}
JSON
  (cd "$project" && "$tsc" -p tsconfig.json) || { echo "tsc failed (moduleResolution $resolution)"; status=1; }
  echo "tsc $("$tsc" --version) --strict, moduleResolution $resolution: done"
done
exit "$status"
