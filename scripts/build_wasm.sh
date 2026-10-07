#!/usr/bin/env bash
# Build the `fontpixel` WebAssembly package for one profile.
#
# Usage: scripts/build_wasm.sh [full|minimal|merge|web|custom] [OUT_DIR]
# `custom` uses WASM_FEATURES for experimental feature measurements only.
#
#   full     every format, the bdfparser-js compatibility layer, CJK charset
#            tables and TOML batch text (the complete API; the default)
#   minimal  BDF, PCF and YAFF only
#   merge    the W8.3 merge page: BDF/PCF/OTB/TTF/OTF/WOFF/WOFF2 + CJK tables
#   web      catalogue tool: common formats + CJK tables, no compat/TOML
#
# OUT_DIR defaults to $FONTPIXEL_WASM_OUT/<profile>, else
# ${CARGO_TARGET_DIR:-target}/wasm-pkg/<profile> (outside the repository when
# CARGO_TARGET_DIR is set, and gitignored otherwise).
#
# Steps: wasm-pack (wasm-bindgen CLI matching Cargo.lock, `--target web`,
# release profile with fat LTO, no wasm-opt) -> the pinned wasm-opt pass list
# below -> the package layout (glue + .wasm + js/ wrapper + package.json) ->
# build-info.json with tool versions and sizes before/after wasm-opt (raw and
# gzip -9).
#
# wasm-opt policy: WASM_OPT_FLAGS (below) on every release build; the binary
# is $WASM_OPT, else `wasm-opt` on PATH, else the one wasm-pack downloaded
# (~/.cache/.wasm-pack/wasm-opt-*/bin/wasm-opt). WASM_OPT=off skips it (sizes
# are then pre-optimization only and build-info.json says so).
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
profile="${1:-full}"
case "$profile" in
  full) features="full" ;;
  minimal) features="minimal" ;;
  merge) features="merge" ;;
  web) features="web" ;;
  convert) features="convert" ;;
  custom) features="${WASM_FEATURES:?custom requires WASM_FEATURES}" ;;
  *) echo "unknown profile '$profile' (full|minimal|merge|web|convert|custom)" >&2; exit 2 ;;
esac
out="${2:-${FONTPIXEL_WASM_OUT:+$FONTPIXEL_WASM_OUT/$profile}}"
out="${out:-${CARGO_TARGET_DIR:-target}/wasm-pkg/$profile}"
WASM_OPT_FLAGS=(-Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext
  --enable-mutable-globals --enable-reference-types --enable-multivalue)
wasm_pack="${WASM_PACK:-$(command -v wasm-pack || echo "$HOME/.cargo/bin/wasm-pack")}"
[ -x "$wasm_pack" ] || { echo "wasm-pack not found (set WASM_PACK)" >&2; exit 2; }

stage="${CARGO_TARGET_DIR:-target}/wasm-stage/$profile"
mkdir -p "$stage" "$out"
# Fast builds are for integration, never acceptance or size measurements.
build_mode="${WASM_BUILD_MODE:-release}"
case "$build_mode" in
  release)
    export CARGO_PROFILE_RELEASE_LTO=fat
    # The website's two runtimes are downloaded by visitors: optimise them for
    # size (opt-level "s": ~12% smaller, ~15-30% slower than 3; "z" was ~18%
    # smaller but up to 2.4x slower, measured 2026-10-08). Other profiles keep 3.
    case "$profile" in web|convert) export CARGO_PROFILE_RELEASE_OPT_LEVEL="${CARGO_PROFILE_RELEASE_OPT_LEVEL:-s}" ;; esac
    ;;
  fast)
    export CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_OPT_LEVEL=1
    export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
    export WASM_OPT=off
    ;;
  *) echo "unknown WASM_BUILD_MODE '$build_mode' (release|fast)" >&2; exit 2 ;;
esac
# Fat LTO for the module only (environment override of the release profile;
# native builds keep the workspace's thin LTO).
"$wasm_pack" build crates/fontpixel-wasm --release --target web \
  --no-pack --no-opt --out-dir "$(realpath "$stage")" --out-name fontpixel_wasm \
  -- --locked --no-default-features --features "$features"

wasm="$stage/fontpixel_wasm_bg.wasm"
pre=$(stat -c %s "$wasm")
pre_gz=$(gzip -n -9 -c "$wasm" | wc -c)
pre_sha=$(sha256sum "$wasm" | cut -d ' ' -f 1)
opt="${WASM_OPT:-}"
if [ -z "$opt" ]; then
  opt="$(command -v wasm-opt || true)"
  if [ -z "$opt" ]; then
    opt="$(ls -t "$HOME"/.cache/.wasm-pack/wasm-opt-*/bin/wasm-opt 2>/dev/null | head -n 1 || true)"
  fi
fi
if [ "$opt" = "off" ]; then
  opt_version="off"
  post=$pre post_gz=$pre_gz
else
  [ -n "$opt" ] && [ -x "$opt" ] || { echo "wasm-opt not found (set WASM_OPT, or WASM_OPT=off)" >&2; exit 2; }
  opt_version="$("$opt" --version)"
  # Reuse only an optimization of byte-identical input with identical flags
  # and tool version, after hashing the existing output. wasm-pack above still
  # builds the current sources and generates fresh JS/declarations every time.
  if python3 - "$out" "$pre_sha" "$opt_version" "${WASM_OPT_FLAGS[*]}" <<'PY'
import hashlib, json, pathlib, sys
out, before, version, flags = sys.argv[1:]
try:
    directory = pathlib.Path(out)
    info = json.loads((directory / 'build-info.json').read_text())
    digest = hashlib.sha256((directory / 'fontpixel_wasm_bg.wasm').read_bytes()).hexdigest()
    valid = (info.get('wasm_sha256_before_opt') == before and
             info.get('wasm_opt') == version and info.get('wasm_opt_flags') == flags and
             info.get('wasm_sha256') == digest)
except (OSError, ValueError):
    valid = False
sys.exit(0 if valid else 1)
PY
  then
    echo "wasm-opt: reusing verified output for SHA-256 $pre_sha"
    cp -- "$out/fontpixel_wasm_bg.wasm" "$wasm"
  else
    "$opt" "${WASM_OPT_FLAGS[@]}" "$wasm" -o "$stage/fontpixel_wasm_bg.opt.wasm"
    mv "$stage/fontpixel_wasm_bg.opt.wasm" "$wasm"
  fi
  post=$(stat -c %s "$wasm")
  post_gz=$(gzip -n -9 -c "$wasm" | wc -c)
fi

# Package layout: replace only the files this script writes.
files=(fontpixel_wasm.js fontpixel_wasm.d.ts fontpixel_wasm_bg.wasm fontpixel_wasm_bg.wasm.d.ts)
js=(index.js index.d.ts errors.js errors.d.ts plain.js guard.js resources.js resources.d.ts
  model.js model.d.ts ops-client.js ops-client.d.ts ops-worker.js ops-node-files.js ops-node-files.d.ts
  compat_bdfparser.js compat_bdfparser.d.ts compat_bdfparser_cjk.js compat_bdfparser_cjk.d.ts
  compat_bdfparser_cli.js compat_bdfparser_cli.d.ts)
for f in "${files[@]}"; do cp -- "$stage/$f" "$out/$f"; done
for f in "${js[@]}"; do cp -- "crates/fontpixel-wasm/js/$f" "$out/$f"; done
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
sed "s/\"version\": \"[^\"]*\"/\"version\": \"$version\"/" crates/fontpixel-wasm/js/package.json > "$out/package.json"
bindgen_version="$(sed -n '/^name = "wasm-bindgen"$/{n;s/^version = "\(.*\)"/\1/p}' Cargo.lock)"
cat > "$out/build-info.json" <<EOF
{
  "profile": "$profile",
  "build_mode": "$build_mode",
  "opt_level": "${CARGO_PROFILE_RELEASE_OPT_LEVEL:-3}",
  "features": "$features",
  "source_commit": "$(git rev-parse HEAD)",
  "source_dirty": $(if [ -n "$(git status --porcelain --untracked-files=no)" ]; then echo true; else echo false; fi),
  "wasm_bindgen": "$bindgen_version",
  "wasm_pack": "$("$wasm_pack" --version | sed 's/^wasm-pack //')",
  "rustc": "$(rustc --version)",
  "wasm_opt": "$opt_version",
  "wasm_opt_flags": "$(if [ "$opt" = off ]; then echo; else echo "${WASM_OPT_FLAGS[*]}"; fi)",
  "wasm_bytes_before_opt": $pre,
  "wasm_sha256_before_opt": "$pre_sha",
  "wasm_gzip_bytes_before_opt": $pre_gz,
  "wasm_bytes": $post,
  "wasm_sha256": "$(sha256sum "$wasm" | cut -d ' ' -f 1)",
  "wasm_gzip_bytes": $post_gz
}
EOF
echo "fontpixel wasm package ($profile): $(realpath "$out")"
cat "$out/build-info.json"
