#!/usr/bin/env bash
# Build the `fontpixel` WebAssembly package for one profile.
#
# Usage: scripts/build_wasm.sh [full|minimal|merge] [OUT_DIR]
#
#   full     every format, the bdfparser-js compatibility layer, CJK charset
#            tables and TOML batch text (the complete API; the default)
#   minimal  BDF, PCF and YAFF only
#   merge    the W8.3 merge page: BDF/PCF/OTB/TTF/OTF/WOFF/WOFF2 + CJK tables
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
  *) echo "unknown profile '$profile' (full|minimal|merge)" >&2; exit 2 ;;
esac
out="${2:-${FONTPIXEL_WASM_OUT:+$FONTPIXEL_WASM_OUT/$profile}}"
out="${out:-${CARGO_TARGET_DIR:-target}/wasm-pkg/$profile}"
WASM_OPT_FLAGS=(-Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext
  --enable-mutable-globals --enable-reference-types --enable-multivalue)
wasm_pack="${WASM_PACK:-$(command -v wasm-pack || echo "$HOME/.cargo/bin/wasm-pack")}"
[ -x "$wasm_pack" ] || { echo "wasm-pack not found (set WASM_PACK)" >&2; exit 2; }

stage="${CARGO_TARGET_DIR:-target}/wasm-stage/$profile"
mkdir -p "$stage" "$out"
# Fat LTO for the module only (environment override of the release profile;
# native builds keep the workspace's thin LTO).
CARGO_PROFILE_RELEASE_LTO=fat "$wasm_pack" build crates/fontpixel-wasm --release --target web \
  --no-pack --no-opt --out-dir "$(realpath "$stage")" --out-name fontpixel_wasm \
  -- --locked --no-default-features --features "$features"

wasm="$stage/fontpixel_wasm_bg.wasm"
pre=$(stat -c %s "$wasm")
pre_gz=$(gzip -9 -c "$wasm" | wc -c)
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
  "$opt" "${WASM_OPT_FLAGS[@]}" "$wasm" -o "$stage/fontpixel_wasm_bg.opt.wasm"
  mv "$stage/fontpixel_wasm_bg.opt.wasm" "$wasm"
  post=$(stat -c %s "$wasm")
  post_gz=$(gzip -9 -c "$wasm" | wc -c)
fi

# Package layout: replace only the files this script writes.
files=(fontpixel_wasm.js fontpixel_wasm.d.ts fontpixel_wasm_bg.wasm fontpixel_wasm_bg.wasm.d.ts)
js=(index.js index.d.ts errors.js errors.d.ts plain.js guard.js resources.js resources.d.ts
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
  "features": "$features",
  "wasm_bindgen": "$bindgen_version",
  "wasm_pack": "$("$wasm_pack" --version | sed 's/^wasm-pack //')",
  "rustc": "$(rustc --version)",
  "wasm_opt": "$opt_version",
  "wasm_opt_flags": "$(if [ "$opt" = off ]; then echo; else echo "${WASM_OPT_FLAGS[*]}"; fi)",
  "wasm_bytes_before_opt": $pre,
  "wasm_gzip_bytes_before_opt": $pre_gz,
  "wasm_bytes": $post,
  "wasm_gzip_bytes": $post_gz
}
EOF
echo "fontpixel wasm package ($profile): $(realpath "$out")"
cat "$out/build-info.json"
