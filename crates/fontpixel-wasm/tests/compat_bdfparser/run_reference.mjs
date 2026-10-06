#!/usr/bin/env node
// Run bdfparser-js's own (unmodified) test suite against `fontpixel/compat/bdfparser`.
//
//   node run_reference.mjs --work DIR [--reference DIR] [--fonts DIR] [--pkg DIR]
//                          [--slow] [--json OUT] [--install] [-- VITEST_ARGS...]
//
// - `--reference`: the bdfparser-js checkout (default: `$BDFPARSER_JS_REFERENCE`,
//   else the first `reference/bdfparser-js` found walking up from this file).
//   It is only read.
// - `--work`: a writable directory outside the reference and the repository
//   (required, or `$BDFPARSER_JS_WORK`). The reference `test/` tree is copied
//   there (fresh on every run), the fonts the bdfparser-js package removed as
//   duplicates are restored from the Python bdfparser (`test/fonts/DEDUPED.txt`;
//   `--fonts`, default `<reference>/../bdfparser/tests/fonts`), and vitest runs
//   there with its cache directory inside it.
// - The test dependencies (vitest 5.0.3, readlineiter, iconv-lite, opentype.js,
//   fetchline, @types/node, typescript) must be installed in `<work>/node_modules`;
//   `--install` runs `npm install --prefix <work>` for them first.
// - Every `../src/<module>` import of the tests resolves to the shim module
//   `shim/<module>.mjs`, which re-exports the compatibility API of the built
//   package (`--pkg`, default `$FONTPIXEL_WASM_PKG`, else
//   `${CARGO_TARGET_DIR:-<repo>/target}/wasm-pkg/full`). The package is
//   externalized, so `vi.resetModules()` keeps one WebAssembly instance.
// - `--slow` sets `BDFPARSER_SLOW_TESTS=1` (the reference's long TrueType tests).
// - A JSON summary (counts, every test with its outcome, failure messages) is
//   written to `--json` (default: `reference_results.json` next to this file).
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SHIM = path.join(HERE, 'shim');
const REPO = path.resolve(HERE, '../../../..');

const DEPS = {
  vitest: '5.0.3',
  readlineiter: '1',
  'iconv-lite': '0.7',
  'opentype.js': '2',
  fetchline: '1',
  '@types/node': '^26.6.4',
  typescript: '6.0.3',
};

const parseArgs = (argv) => {
  const out = { rest: [], slow: false, install: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--') {
      out.rest = argv.slice(i + 1);
      break;
    }
    if (a === '--slow') out.slow = true;
    else if (a === '--install') out.install = true;
    else if (['--reference', '--work', '--fonts', '--pkg', '--json'].includes(a)) out[a.slice(2)] = argv[++i];
    else throw new Error(`unknown argument ${a}`);
  }
  return out;
};

const findReference = (explicit) => {
  if (explicit) return path.resolve(explicit);
  if (process.env.BDFPARSER_JS_REFERENCE) return path.resolve(process.env.BDFPARSER_JS_REFERENCE);
  for (let dir = HERE; dir !== path.dirname(dir); dir = path.dirname(dir)) {
    const cand = path.join(dir, 'reference', 'bdfparser-js');
    if (existsSync(path.join(cand, 'test'))) return cand;
  }
  throw new Error('reference bdfparser-js not found; pass --reference or set BDFPARSER_JS_REFERENCE');
};

const args = parseArgs(process.argv.slice(2));
const reference = findReference(args.reference);
const work = path.resolve(args.work ?? process.env.BDFPARSER_JS_WORK ?? '');
if (!args.work && !process.env.BDFPARSER_JS_WORK) {
  throw new Error('pass --work DIR (a writable directory outside the reference and the repository)');
}
if (work.startsWith(path.resolve(reference)) || work.startsWith(REPO + path.sep)) {
  throw new Error(`--work must be outside the reference and the repository: ${work}`);
}
const fonts = path.resolve(args.fonts ?? path.join(reference, '..', 'bdfparser', 'tests', 'fonts'));
const pkg = path.resolve(
  args.pkg ??
    process.env.FONTPIXEL_WASM_PKG ??
    path.join(process.env.CARGO_TARGET_DIR ?? path.join(REPO, 'target'), 'wasm-pkg', 'full'),
);
const jsonOut = path.resolve(args.json ?? path.join(HERE, 'reference_results.json'));
if (!existsSync(path.join(pkg, 'compat_bdfparser.js'))) {
  throw new Error(`no compatibility module in ${pkg} (build with scripts/build_wasm.sh full)`);
}

// 1. a fresh copy of the reference tests, deduplicated fonts restored
mkdirSync(work, { recursive: true });
const testDir = path.join(work, 'test');
rmSync(testDir, { recursive: true, force: true });
cpSync(path.join(reference, 'test'), testDir, { recursive: true });
let restored = 0;
for (const f of readdirSync(fonts)) {
  const dst = path.join(testDir, 'fonts', f);
  if (f.endsWith('.bdf') && !existsSync(dst)) {
    cpSync(path.join(fonts, f), dst);
    restored++;
  }
}

// 2. test dependencies, installed into the work directory only
if (args.install) {
  const deps = Object.entries(DEPS).map(([n, v]) => `${n}@${v}`);
  const r = spawnSync('npm', ['install', '--no-save', '--no-audit', '--no-fund', '--prefix', work, ...deps], {
    stdio: 'inherit',
  });
  if (r.status !== 0) throw new Error('npm install failed');
}
const vitest = path.join(work, 'node_modules', '.bin', 'vitest');
if (!existsSync(vitest)) {
  throw new Error(`vitest is not installed in ${work} (run with --install)`);
}

// 3. vitest configuration: ../src/* -> shim, the package externalized
const config = `// generated by ${path.relative(REPO, fileURLToPath(import.meta.url))}
import { defineConfig } from 'vitest/config'

export default defineConfig({
  cacheDir: ${JSON.stringify(path.join(work, '.vite-cache'))},
  resolve: {
    alias: [
      { find: /^\\.\\.\\/src\\/(.*)$/, replacement: ${JSON.stringify(SHIM + '/')} + '$1.mjs' },
      { find: /^fontpixel\\/compat\\/bdfparser\\/cjk$/, replacement: ${JSON.stringify(path.join(pkg, 'compat_bdfparser_cjk.js'))} },
      { find: /^fontpixel\\/compat\\/bdfparser\\/cli$/, replacement: ${JSON.stringify(path.join(pkg, 'compat_bdfparser_cli.js'))} },
      { find: /^fontpixel\\/compat\\/bdfparser$/, replacement: ${JSON.stringify(path.join(pkg, 'compat_bdfparser.js'))} },
    ],
  },
  server: { fs: { strict: false } },
  test: {
    include: ['test/**/*.test.ts'],
    globals: true,
    testTimeout: 60000,
    hookTimeout: 120000,
    server: { deps: { external: [${JSON.stringify(pkg)}, /wasm-pkg/] } },
  },
})
`;
const configPath = path.join(work, 'vitest.compat.config.mjs');
writeFileSync(configPath, config);

// 4. run
const raw = path.join(work, 'vitest-results.json');
rmSync(raw, { force: true });
const env = { ...process.env };
if (args.slow) env.BDFPARSER_SLOW_TESTS = '1';
else delete env.BDFPARSER_SLOW_TESTS;
const t0 = Date.now();
const run = spawnSync(
  vitest,
  ['run', '--config', configPath, '--reporter=dot', '--reporter=json', `--outputFile.json=${raw}`, ...args.rest],
  { cwd: work, env, stdio: 'inherit' },
);
const seconds = Math.round((Date.now() - t0) / 100) / 10;

// 5. summary
const report = JSON.parse(readFileSync(raw, 'utf8'));
const tests = {};
const counts = {};
const add = (id, entry) => {
  tests[id] = entry;
  counts[entry.outcome] = (counts[entry.outcome] ?? 0) + 1;
};
for (const file of report.testResults) {
  const rel = path.relative(work, file.name);
  if (file.assertionResults.length === 0 && file.status === 'failed') {
    add(`${rel}::<suite>`, { outcome: 'error', message: (file.message ?? '').split('\n')[0].slice(0, 500) });
    continue;
  }
  for (const t of file.assertionResults) {
    const outcome = t.status === 'passed' ? 'passed' : t.status === 'failed' ? 'failed' : 'skipped';
    const entry = { outcome };
    if (outcome === 'failed') {
      entry.message = (t.failureMessages ?? [''])[0].split('\n')[0].slice(0, 500);
    }
    add(`${rel}::${t.fullName}`, entry);
  }
}
const pkgInfo = JSON.parse(readFileSync(path.join(pkg, 'package.json'), 'utf8'));
const summary = {
  reference: reference,
  fontpixel: pkgInfo.version,
  bdfparser_api: '3.0.0',
  node: process.version,
  vitest: DEPS.vitest,
  slow: args.slow,
  restored_fonts: restored,
  vitest_exit_code: run.status,
  seconds,
  counts,
  tests: Object.fromEntries(Object.entries(tests).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))),
};
writeFileSync(jsonOut, JSON.stringify(summary, null, 1) + '\n');
console.log(`summary: ${JSON.stringify(counts)} -> ${jsonOut}`);
process.exitCode = run.status ?? 1;
