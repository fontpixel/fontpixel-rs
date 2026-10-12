// Shared helpers of the fontpixel WASM node:test suites.
//
// Environment:
//   FONTPIXEL_WASM_PKG      built package directory (default:
//                           ${CARGO_TARGET_DIR:-<repo>/target}/wasm-pkg/<profile>)
//   FONTPIXEL_WASM_PROFILE  profile of the default package directory (full)
//   FONTPIXEL_CLI           native CLI for reference outputs (default: the
//                           current target/release/fontpixel); tests needing it skip
//                           when it is missing. Tests may run the CLI to
//                           produce reference bytes; the binding never does.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { gunzipSync } from 'node:zlib';

export const REPO = path.resolve(fileURLToPath(new URL('../../..', import.meta.url)));
export const PROFILE = process.env.FONTPIXEL_WASM_PROFILE ?? 'full';
export const PKG = path.resolve(
  process.env.FONTPIXEL_WASM_PKG ??
    path.join(process.env.CARGO_TARGET_DIR ?? path.join(REPO, 'target'), 'wasm-pkg', PROFILE),
);
export const DATA = path.join(REPO, 'tests', 'data');
export const CLI =
  process.env.FONTPIXEL_CLI ??
  path.join(process.env.CARGO_TARGET_DIR ?? path.join(REPO, 'target'), 'release', 'fontpixel');
export const HAS_CLI = existsSync(CLI);

let loaded;
/** The built package, initialized (`await init()` once per process). */
export async function load() {
  if (!loaded) {
    loaded = (async () => {
      const fp = await import(pathToFileURL(path.join(PKG, 'index.js')).href);
      await fp.init();
      return fp;
    })();
  }
  return loaded;
}

/** The package's build-info.json (profile, tool versions, sizes). */
export function buildInfo() {
  return JSON.parse(readFileSync(path.join(PKG, 'build-info.json'), 'utf8'));
}

/**
 * The package.json that governs PKG: PKG's own (a scripts/build_wasm.sh
 * build or the npm package root) or, for a profile directory of the npm
 * package (`profiles/<name>/`), the package root's. `subpath` is the export
 * prefix ('.' or './<name>') and `dir` the directory of PKG's files ('.' or
 * './profiles/<name>').
 */
export function packageManifest() {
  const own = path.join(PKG, 'package.json');
  if (existsSync(own)) return { manifest: JSON.parse(readFileSync(own, 'utf8')), subpath: '.', dir: '.' };
  const name = path.basename(PKG);
  const root = path.resolve(PKG, '..', '..');
  const manifest = JSON.parse(readFileSync(path.join(root, 'package.json'), 'utf8'));
  return { manifest, subpath: `./${name}`, dir: `./profiles/${name}` };
}

/** Whether the loaded build has capability `name`. */
export function has(fp, name) {
  return fp.capabilities().some((c) => c.name === name && c.available);
}

/** Bytes of a file under tests/data. */
export function sample(relative) {
  return new Uint8Array(readFileSync(path.join(DATA, relative)));
}

/** The repository's gzip-compressed real-font fixtures (fixtures/README.md). */
export const FIXTURES = path.join(REPO, 'fixtures');
const fixtureCache = new Map();
/** Decompressed bytes of `fixtures/<relative>` (a `.gz` file), cached per process. */
export function fixture(relative) {
  if (!fixtureCache.has(relative)) {
    fixtureCache.set(relative, new Uint8Array(gunzipSync(readFileSync(path.join(FIXTURES, relative)))));
  }
  return fixtureCache.get(relative);
}

/** Common samples. */
export const SPLEEN = 'bdf/spleen-5x8.bdf';
export const BLOCK = 'bdf/3x5-block.bdf';

/** A fresh temporary directory; `cleanup()` removes exactly that directory. */
export function tempDir(prefix = 'fontpixel-wasm-test-') {
  const dir = mkdtempSync(path.join(tmpdir(), prefix));
  return { dir, cleanup: () => rmSync(dir, { recursive: true, force: true }) };
}

/**
 * Run the native CLI with `args` in `cwd`; returns {code, stdout, stderr}
 * (stdout as bytes, stderr as text).
 */
export function runCli(args, cwd) {
  const proc = spawnSync(CLI, args, { cwd, maxBuffer: 1 << 30 });
  return { code: proc.status, stdout: new Uint8Array(proc.stdout), stderr: proc.stderr.toString() };
}

/**
 * Write `inputs` ({name: bytes}) to a temp directory, run the CLI there and
 * return every file it left besides the inputs ({relative name: bytes}),
 * with the exit code, stdout and stderr.
 */
export function cliFiles(inputs, args) {
  const { dir, cleanup } = tempDir();
  try {
    for (const [name, bytes] of Object.entries(inputs)) writeFileSync(path.join(dir, name), bytes);
    const result = runCli(args, dir);
    const files = {};
    const walk = (sub) => {
      for (const entry of readdirSync(path.join(dir, sub))) {
        const rel = sub ? `${sub}/${entry}` : entry;
        if (statSync(path.join(dir, rel)).isDirectory()) walk(rel);
        else if (!(rel in inputs)) files[rel] = new Uint8Array(readFileSync(path.join(dir, rel)));
      }
    };
    walk('');
    return { ...result, files };
  } finally {
    cleanup();
  }
}

/** Plain data without the source-format field (for model comparisons). */
export function withoutSource(dict) {
  const { source_format: _ignored, ...rest } = dict;
  return rest;
}
