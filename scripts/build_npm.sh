#!/usr/bin/env bash
# Assemble and pack the npm package `fontpixel` (nothing is published).
#
# Usage: scripts/build_npm.sh OUT_DIR
#
# Layout: the `full` profile at the package root (`fontpixel`,
# `fontpixel/compat/bdfparser`, `/cjk`, `/cli`, `fontpixel/node-files`), and
# each smaller profile in profiles/<name>/, exported as `fontpixel/<name>`
# (plus `fontpixel/<name>/compat/bdfparser...` when the profile contains the
# compatibility layer). docs/decisions.md ("npm package profiles") explains
# the choice.
#
# Environment:
#   NPM_PROFILES       smaller profiles to ship (default: "bdfparser web minimal")
#   NPM_REUSE_BUILDS=1 reuse $CARGO_TARGET_DIR/wasm-pkg/<profile> when it was
#                      built from the current commit (default: build every profile
#                      with scripts/build_wasm.sh)
#   NPM_ALLOW_DIRTY=1  accept builds from a dirty work tree (never for a release)
#   CARGO_TARGET_DIR   as for scripts/build_wasm.sh
#
# Output: OUT_DIR/package/ (the staged package) and OUT_DIR/fontpixel-<version>.tgz.
set -euo pipefail
repo="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"
out="${1:?usage: scripts/build_npm.sh OUT_DIR}"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
case "$out" in "$repo"|"$repo"/*) echo "refusing to write the package inside the repository: $out" >&2; exit 2;; esac
stage="$out/package"
[ ! -e "$stage" ] || { echo "$stage exists; remove it first" >&2; exit 2; }
read -r -a profiles <<<"${NPM_PROFILES-bdfparser web minimal}"
pkgs="${CARGO_TARGET_DIR:-$repo/target}/wasm-pkg"
commit="$(git rev-parse HEAD)"

for profile in full "${profiles[@]}"; do
  dir="$pkgs/$profile"
  if [ "${NPM_REUSE_BUILDS:-0}" = 1 ] && [ -f "$dir/build-info.json" ] &&
      grep -q "\"source_commit\": \"$commit\"" "$dir/build-info.json"; then
    echo "== reusing $dir"
  else
    scripts/build_wasm.sh "$profile" "$dir"
  fi
  grep -q '"build_mode": "release"' "$dir/build-info.json" || { echo "$dir is not a release build" >&2; exit 2; }
  if [ "${NPM_ALLOW_DIRTY:-0}" != 1 ] && ! grep -q '"source_dirty": false' "$dir/build-info.json"; then
    echo "$dir was built from a dirty work tree (commit first, or NPM_ALLOW_DIRTY=1 for a trial)" >&2
    exit 2
  fi
done

copy_build() { # copy_build SRC DST: every file of a profile build except its package.json
  mkdir -p "$2"
  find "$1" -maxdepth 1 -type f ! -name package.json -exec cp -- {} "$2/" \;
}
copy_build "$pkgs/full" "$stage"
for profile in "${profiles[@]}"; do copy_build "$pkgs/$profile" "$stage/profiles/$profile"; done
cp crates/fontpixel-wasm/README.md LICENSE THIRD_PARTY_NOTICES.md "$stage/"
cp -R LICENSES "$stage/LICENSES"

# package.json: the template without `private`, plus the profile exports.
node --input-type=module - crates/fontpixel-wasm/js/package.json "$stage" "${profiles[@]}" <<'EOF'
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
const [template, stage, ...profiles] = process.argv.slice(2);
const manifest = JSON.parse(readFileSync(template, 'utf8'));
delete manifest.private;
const version = /^version = "(.*)"$/m.exec(readFileSync('Cargo.toml', 'utf8'))[1];
manifest.version = version;
const exportsMap = {};
for (const [key, value] of Object.entries(manifest.exports)) {
  if (key === './package.json') continue;
  exportsMap[key] = value;
}
for (const profile of profiles) {
  const dir = `./profiles/${profile}`;
  const info = JSON.parse(readFileSync(path.join(stage, 'profiles', profile, 'build-info.json'), 'utf8'));
  const compat = /(^|,)(full|bdfparser|compat-bdfparser)(,|$)/.test(info.features);
  exportsMap[`./${profile}`] = { types: `${dir}/index.d.ts`, default: `${dir}/index.js` };
  exportsMap[`./${profile}/node-files`] = { types: `${dir}/ops-node-files.d.ts`, node: `${dir}/ops-node-files.js` };
  exportsMap[`./${profile}/ops-worker.js`] = `${dir}/ops-worker.js`;
  if (compat) {
    exportsMap[`./${profile}/compat/bdfparser`] = { types: `${dir}/compat_bdfparser.d.ts`, default: `${dir}/compat_bdfparser.js` };
    exportsMap[`./${profile}/compat/bdfparser/cjk`] = { types: `${dir}/compat_bdfparser_cjk.d.ts`, default: `${dir}/compat_bdfparser_cjk.js` };
    exportsMap[`./${profile}/compat/bdfparser/cli`] = { types: `${dir}/compat_bdfparser_cli.d.ts`, default: `${dir}/compat_bdfparser_cli.js` };
  }
  exportsMap[`./${profile}/fontpixel_wasm_bg.wasm`] = `${dir}/fontpixel_wasm_bg.wasm`;
  manifest.sideEffects.push(
    ...['index.js', 'guard.js', 'ops-worker.js', 'compat_bdfparser.js', 'compat_bdfparser_cjk.js'].map((f) => `${dir}/${f}`),
  );
}
exportsMap['./package.json'] = './package.json';
manifest.exports = exportsMap;
writeFileSync(path.join(stage, 'package.json'), `${JSON.stringify(manifest, null, 2)}\n`);

// README: the size table's SIZE_<PROFILE> / GZ_<PROFILE> placeholders take the
// measured sizes of these builds; a placeholder left over is an error.
const mb = (bytes) => `${(bytes / 1e6).toFixed(2)} MB`;
const readmePath = path.join(stage, 'README.md');
let readme = readFileSync(readmePath, 'utf8');
for (const profile of ['full', ...profiles]) {
  const dir = profile === 'full' ? stage : path.join(stage, 'profiles', profile);
  const info = JSON.parse(readFileSync(path.join(dir, 'build-info.json'), 'utf8'));
  const key = profile.toUpperCase();
  readme = readme.replaceAll(`SIZE_${key}`, mb(info.wasm_bytes)).replaceAll(`GZ_${key}`, mb(info.wasm_gzip_bytes));
}
const left = readme.match(/\b(SIZE|GZ)_[A-Z]+\b/g);
if (left) throw new Error(`README.md: no build for the size placeholders ${[...new Set(left)].join(', ')}`);
writeFileSync(readmePath, readme);
EOF

(cd "$stage" && npm pack --pack-destination "$out" --json >"$out/pack.json")
node --input-type=module - "$out/pack.json" <<'EOF'
import { readFileSync } from 'node:fs';
// npm <= 11 prints an array of results, npm 12 an object keyed by package name.
const parsed = JSON.parse(readFileSync(process.argv[2], 'utf8'));
const pack = Array.isArray(parsed) ? parsed[0] : Object.values(parsed)[0];
console.log(`${pack.filename}: ${pack.size} bytes packed, ${pack.unpackedSize} bytes unpacked, ${pack.entryCount} files`);
for (const f of pack.files) console.log(`  ${String(f.size).padStart(10)}  ${f.path}`);
EOF
