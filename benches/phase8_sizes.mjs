// Build five experimental combinations, then measure all eight modules.
// --measure-only reuses existing release artifacts after feature/hash checks.
// The three supported packages must already have passed check_wasm.sh.
import assert from 'node:assert/strict';
import { readFileSync, mkdirSync, openSync, closeSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { promisify } from 'node:util';
import { execFile } from 'node:child_process';
import path from 'node:path';
import os from 'node:os';
import { ROOT, WORK, PKG, sha256, metadata, save, browserFor, serve } from './phase8_support.mjs';
const exec = promisify(execFile);
const target = process.env.CARGO_TARGET_DIR ?? path.join(ROOT, 'target');
assert.ok(process.argv.slice(2).every(arg => arg === '--measure-only'), 'only --measure-only is supported');
const measureOnly = process.argv.includes('--measure-only');
const vector = 'minimal,fontpixel/ttf,fontpixel/otf,fontpixel/woff';
const combinations = [
  ['full', 'full', true, true, true], ['merge', 'merge', true, true, true], ['minimal', 'minimal', false, false, false],
  ['minimal-cjk', 'minimal,cjk-charsets', false, false, true],
  ['registry-vector', vector, true, false, false], ['registry-vector-cjk', vector + ',cjk-charsets', true, false, true],
  ['registry-vector-woff2', vector + ',fontpixel/woff2', true, true, false],
  ['registry-vector-woff2-cjk', vector + ',fontpixel/woff2,cjk-charsets', true, true, true],
];
const evidence = { ...metadata(), buildPolicy: measureOnly ? 'verify existing release artifacts; no rebuild' : 'build all experimental artifacts before measuring', method: '5 fresh Node processes and 5 fresh Chromium processes per module; bytes fetched/read before timer; JS import, WebAssembly.compile and asynchronous init(module) measured separately; warm OS disk cache, no reuse of an engine process; WASM/input transfer excluded; browser import includes loopback JS fetch; five rounds rotate/reverse profile order after all builds finish; per-sample load recorded; runtime writers exercised after timing.', combinations: [] };
const mounts = { '/bench/': path.join(ROOT, 'benches'), '/input/': path.join(ROOT, 'tests/data/bdf') };
for (const [name] of combinations) mounts[`/${name}/`] = path.join(target, 'wasm-pkg', name);
const server = await serve(mounts);
try {
  for (const [name, features] of combinations) {
    const pkg = mounts[`/${name}/`];
    if (!measureOnly && !['full', 'merge', 'minimal'].includes(name)) {
      mkdirSync(path.join(WORK, 'sizes'), { recursive: true });
      const fd = openSync(path.join(WORK, 'sizes', `${name}.log`), 'w');
      try { execFileSync(path.join(ROOT, 'scripts/build_wasm.sh'), ['custom', pkg], { cwd: ROOT, env: { ...process.env, WASM_BUILD_MODE: 'release', WASM_FEATURES: features }, stdio: ['ignore', fd, fd] }); }
      finally { closeSync(fd); }
    }
    const build = JSON.parse(readFileSync(path.join(pkg, 'build-info.json')));
    assert.equal(build.build_mode, 'release'); assert.notEqual(build.wasm_opt, 'off'); assert.equal(build.source_dirty, false);
    assert.equal(build.features, features);
    assert.equal(build.wasm_sha256, sha256(readFileSync(path.join(pkg, 'fontpixel_wasm_bg.wasm'))));
    evidence.combinations.push({ name, build, node: [], chromium: [] });
    console.log(`${name}: ${build.wasm_bytes} bytes, ${build.wasm_gzip_bytes} gzip`);
  }
  // All compilation/optimization is finished before the first timed sample.
  // Rotate and reverse the profile order so no profile owns one time window.
  for (let n = 0; n < 5; n++) {
    const offset = n * 3 % combinations.length;
    const order = [...combinations.slice(offset), ...combinations.slice(0, offset)];
    if (n % 2) order.reverse();
    for (const [name, , vectorEnabled, woff2, cjk] of order) {
      const row = evidence.combinations.find(row => row.name === name);
      const pkg = mounts[`/${name}/`];
      let loadBefore = os.loadavg();
      const { stdout } = await exec(process.execPath, [path.join(ROOT, 'benches/phase8_init_node.mjs'), pkg, path.join(ROOT, 'tests/data/bdf/3x5-block.bdf')]);
      row.node.push({ ...JSON.parse(stdout), repeat: n, loadBefore, loadAfter: os.loadavg() });
      loadBefore = os.loadavg();
      const browser = await browserFor('chromium');
      try {
        evidence.chromium = browser.version();
        const page = await browser.newPage();
        await page.goto(server.url);
        const result = await page.evaluate(async name => {
          const bytes = await (await fetch(`/${name}/fontpixel_wasm_bg.wasm`)).arrayBuffer();
          const font = new Uint8Array(await (await fetch('/input/3x5-block.bdf')).arrayBuffer());
          return (await import('/bench/phase8_init.mjs')).measure(`/${name}/index.js`, bytes, font);
        }, name);
        row.chromium.push({ ...result, repeat: n, loadBefore, loadAfter: os.loadavg() });
      } finally { await browser.close(); }
      for (const result of [row.node[n], row.chromium[n]]) {
        assert.equal(result.enabled.ttf, vectorEnabled, `${name}: ttf`);
        assert.equal(result.enabled['vector-raster'], vectorEnabled, `${name}: vector-raster`);
        assert.equal(result.enabled.woff2, woff2, `${name}: woff2`);
        assert.equal(result.cjk, cjk, `${name}: cjk`);
      }
      console.log(`round ${n + 1}: ${name}`);
      save(path.join(ROOT, 'benches/results/phase8-sizes.json'), evidence);
    }
  }
  assert.deepEqual(server.errors, []);
} catch (error) { evidence.failure = String(error.stack ?? error); throw error; }
finally { save(path.join(ROOT, 'benches/results/phase8-sizes.json'), evidence); await server.close(); }
