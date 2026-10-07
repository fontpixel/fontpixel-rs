#!/usr/bin/env bash
# WASM/JS binding gate: wasm32 clippy for every profile, generated TypeScript
# up to date, build the packages, node:test suites, `tsc --strict` smoke over
# the declarations, then the binding coverage gate.
#
# Usage: scripts/check_wasm.sh [PROFILE ...]   (default: full minimal merge web convert)
#
# Environment: CARGO_TARGET_DIR (packages go to $CARGO_TARGET_DIR/wasm-pkg),
# TSC (TypeScript compiler; default `tsc` on PATH, else the read-only copy in
# ../bdfparser-js/node_modules), GATE_REQUIRE (default python,js; the gate
# fails until every area has its JS entries), SKIP_GATE=1 to stop before it.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
profiles=("$@")
[ ${#profiles[@]} -gt 0 ] || profiles=(full minimal merge web convert)
target="${CARGO_TARGET_DIR:-$PWD/target}"
export CARGO_TARGET_DIR="$target"
[ "${WASM_BUILD_MODE:-release}" = release ] && [ "${WASM_OPT:-}" != off ] || {
  echo 'acceptance requires release builds with wasm-opt' >&2; exit 2;
}
# Reference bytes must come from the current portable-float native encoder.
cargo build --locked --release -p fontpixel-cli --all-features
export FONTPIXEL_CLI="${FONTPIXEL_CLI:-$target/release/fontpixel}"
[ -x "$FONTPIXEL_CLI" ] || { echo "missing native reference: $FONTPIXEL_CLI" >&2; exit 2; }

for features in full minimal merge web convert; do
  cargo clippy --locked -p fontpixel-wasm --target wasm32-unknown-unknown --all-targets \
    --no-default-features --features "$features" -- -D warnings
done
cargo test --locked -p fontpixel-wasm --lib

# Generated plain-data declarations must match the rustdoc inventory.
docs="${FONTPIXEL_RUSTDOC_TARGET:-$target/rustdoc-json}"
for crate in fontpixel fontpixel-ops; do
  CARGO_TARGET_DIR="$docs" RUSTDOCFLAGS="-Z unstable-options --output-format json" \
    cargo +nightly rustdoc -q --locked -p "$crate" --all-features --lib
done
"${PYTHON:-python3}" tools/bindings/gen_dts.py --rustdoc "$docs/doc" --check

for profile in "${profiles[@]}"; do
  scripts/build_wasm.sh "$profile" "$target/wasm-pkg/$profile"
  FONTPIXEL_WASM_PKG="$target/wasm-pkg/$profile" node --test crates/fontpixel-wasm/tests/*.test.mjs
  if [ "$profile" = full ]; then
    FONTPIXEL_WASM_PKG="$target/wasm-pkg/full" node examples/js/run.mjs
    "${PYTHON:-python3}" tools/bindings/gen_jsref.py --pkg "$target/wasm-pkg/full" --check
  fi
done

tsc="${TSC:-$(command -v tsc || true)}"
if [ -z "$tsc" ]; then
  # Read-only use of the sibling bdfparser-js checkout's compiler (worktrees
  # sit deeper, so look in every ancestor).
  dir="$PWD"
  while [ "$dir" != / ] && [ -z "$tsc" ]; do
    dir="$(dirname "$dir")"
    [ -x "$dir/bdfparser-js/node_modules/.bin/tsc" ] && tsc="$dir/bdfparser-js/node_modules/.bin/tsc"
  done
fi
[ -n "$tsc" ] && [ -x "$tsc" ] || { echo "TypeScript compiler not found (set TSC)" >&2; exit 2; }
typing="$target/wasm-typing"
mkdir -p "$typing"
cat > "$typing/tsconfig.json" <<EOF
{
  "compilerOptions": {
    "strict": true,
    "noEmit": true,
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "lib": ["ES2022", "DOM", "ESNext.Disposable"],
    "types": [],
    "skipLibCheck": false,
    "paths": {
      "fontpixel": ["$target/wasm-pkg/${profiles[0]}/index.d.ts"],
      "fontpixel/compat/bdfparser": ["$target/wasm-pkg/${profiles[0]}/compat_bdfparser.d.ts"],
      "fontpixel/compat/bdfparser/cli": ["$target/wasm-pkg/${profiles[0]}/compat_bdfparser_cli.d.ts"]
    }
  },
  "files": ["$PWD/crates/fontpixel-wasm/tests/typing/smoke.ts"]
}
EOF
"$tsc" --version
"$tsc" -p "$typing/tsconfig.json"
echo "tsc --strict smoke passed"

[ "${SKIP_GATE:-0}" = 1 ] && exit 0
FONTPIXEL_RUSTDOC_TARGET="$docs" tools/bindings/gate.sh "${GATE_REQUIRE:-python,js}"
