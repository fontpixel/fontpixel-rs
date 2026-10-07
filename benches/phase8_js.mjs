// Five paired samples, alternating engine order; fresh process/context per engine.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import os from 'node:os';
import { ROOT, WORK, PKG, requireTool, sha256, metadata, save, browserFor, serve } from './phase8_support.mjs';
const source = process.env.BDFPARSER_JS_REFERENCE ?? path.join(ROOT, '../reference/bdfparser-js');
const version = JSON.parse(readFileSync(path.join(source, 'package.json'))).version;
let referenceSources;
assert.equal(version, '3.0.0');
const bundle = path.join(WORK, 'reference-js.mjs');
await requireTool('esbuild').build({ entryPoints: [path.join(source, 'src/index.ts')], outfile: bundle, bundle: true, format: 'esm', platform: 'browser', target: 'es2022', metafile: true }).then(result => {
  save(path.join(WORK, 'reference-js-build.json'), result.metafile);
  referenceSources = Object.keys(result.metafile.inputs).sort().map(file => ({ file, sha256: sha256(readFileSync(path.resolve(ROOT, file))) }));
});
const samples = process.env.FONTPIXEL_TONG_SAMPLES ?? path.join(ROOT, '../samples/tong-pixel-sans');
const sc = path.join(samples, 'TongPixelSans14Large-SC.bdf');
const latin = path.join(samples, 'TongPixelSans14Large-Latin.bdf');
const evidence = { ...metadata(), reference: { source, version, sources: referenceSources, bundle_sha256: sha256(readFileSync(bundle)) }, inputs: [sc, latin].map(file => ({ file, bytes: readFileSync(file).length, sha256: sha256(readFileSync(file)) })), methodology: '5 paired repetitions; alternating pure/compat/native order; fresh process or context for each engine; preloaded bytes; one whole/20 small warmups; fixed iterations; output verification outside timings; native handle free included; explicit GC outside timing in Node and Chromium; no raised resource caps; Latin draw-all and TTF, SC BDF/PCF/parse/text.', samples: [] };
const server = await serve({ '/pkg/': PKG, '/bench/': path.join(ROOT, 'benches'), '/reference/': WORK, '/inputs/': samples });
let browser;
try {
  for (const host of (process.env.FONTPIXEL_BENCH_HOSTS ?? 'node,chromium').split(',')) {
    if (host === 'chromium') { browser = await browserFor('chromium'); evidence.chromium = browser.version(); }
    for (let repeat = 0; repeat < +(process.env.FONTPIXEL_BENCH_REPEATS ?? 5); repeat++) {
      for (const engine of (repeat % 2 ? ['native', 'compat', 'pure'] : ['pure', 'compat', 'native'])) {
        const before = os.loadavg();
        let cases;
        if (host === 'node') {
          cases = JSON.parse(execFileSync(process.execPath, ['--expose-gc', path.join(ROOT, 'benches/phase8_node_child.mjs'), engine, PKG, bundle, sc, latin], { encoding: 'utf8', maxBuffer: 8 << 20 }));
        } else {
          const context = await browser.newContext();
          try {
            const page = await context.newPage();
            const cdp = await context.newCDPSession(page);
            await page.exposeFunction('collectGarbage', () => cdp.send('HeapProfiler.collectGarbage'));
            await page.goto(server.url);
            cases = await page.evaluate(async engine => {
              let api;
              if (engine === 'pure') api = await import('/reference/reference-js.mjs');
              else { api = await import('/pkg/index.js'); await api.init(); if (engine === 'compat') api = await import('/pkg/compat_bdfparser.js'); }
              const bytes = async name => new Uint8Array(await (await fetch('/inputs/' + name)).arrayBuffer());
              const inputs = { sc: await bytes('TongPixelSans14Large-SC.bdf'), latin: await bytes('TongPixelSans14Large-Latin.bdf') };
              return (await import('/bench/phase8_cases.mjs')).benchmark(engine, api, inputs, globalThis.collectGarbage);
            }, engine);
          } finally { await context.close(); }
        }
        evidence.samples.push({ host, repeat, engine, loadBefore: before, loadAfter: os.loadavg(), cases });
        console.log(`${host} pair ${repeat + 1} ${engine}: ${cases.length} cases`);
      }
      const pair = evidence.samples.filter(s => s.host === host && s.repeat === repeat);
      const baseline = pair.find(s => s.engine === 'pure');
      for (const sample of pair) for (const c of sample.cases) {
        const ref = baseline.cases.find(r => r.id === c.id);
        if (c.compare) assert.equal(c.signature, ref.signature, `${host}/${sample.engine}/${c.id} correctness`);
      }
    }
    if (browser) { await browser.close(); browser = undefined; }
  }
  assert.deepEqual(server.errors, []);
} catch (error) { evidence.failure = String(error.stack ?? error); throw error; }
finally {
  if (browser) await browser.close();
  evidence.serverErrors = server.errors;
  save(process.env.FONTPIXEL_BENCH_RESULTS ?? path.join(ROOT, 'benches/results/phase8-js.json'), evidence);
  await server.close();
}
