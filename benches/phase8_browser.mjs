// Chromium, Firefox and WebKit integration evidence (not the phase 9 page).
// Usage: source scripts/local-env.sh; node benches/phase8_browser.mjs
// FONTPIXEL_CLI: freshly rebuilt portable-float CLI; FONTPIXEL_WEBKIT_EXECUTABLE:
// optional host-specific MiniBrowser wrapper. Playwright lives in FONTPIXEL_JS_TOOLS.
import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync, readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { ROOT, WORK, PKG, sha256, metadata, save, browserFor, serve } from './phase8_support.mjs';

const fixture = path.join(WORK, 'browser-fixtures');
mkdirSync(fixture, { recursive: true });
const cli = process.env.FONTPIXEL_CLI ?? path.join(WORK, 'reference/fontpixel');
const samples = process.env.FONTPIXEL_TONG_SAMPLES ?? path.join(ROOT, '../samples/tong-pixel-sans');
const sources = {
  's.bdf': path.join(ROOT, 'tests/data/bdf/spleen-5x8.bdf'),
  'sc.bdf': path.join(samples, 'TongPixelSans14Large-SC.bdf'),
  'latin.bdf': path.join(samples, 'TongPixelSans14Large-Latin.bdf'),
  'mono-latin.bdf': path.join(samples, 'TongPixelSansMono14Large-Latin.bdf'),
  'unifont.bdf': process.env.FONTPIXEL_UNIFONT ?? path.join(ROOT, '../reference/bdfparser/tests/fonts/unifont-13.0.04.bdf'),
};
for (const [name, file] of Object.entries(sources)) copyFileSync(file, path.join(fixture, name));
const native = args => execFileSync(cli, args, { cwd: fixture, encoding: 'utf8', maxBuffer: 32 << 20 });
const info = JSON.parse(native(['info', 's.bdf', '--json']));
const preview = native(['preview', 's.bdf', 'Hello']);
native(['convert', 's.bdf', '-o', 'font.hbf']);
const merges = [
  { name: 'unifont', parts: [{ name: 'latin.bdf', take: 'western' }, { name: 'unifont.bdf', take: 'all' }], options: { family: 'Unifont Western Mix', spacing: 'prop', outputs: ['merged.bdf'], report: 'json' } },
  { name: 'tong', parts: [{ name: 'sc.bdf', take: 'cjk' }, { name: 'mono-latin.bdf', take: 'western' }], options: { family: 'Tong Mono Mix', spacing: 'mono', allow_misaligned: true, outputs: ['mono.bdf', 'mono.woff2'], report: 'json' } },
];
for (const ref of merges) {
  const dir = path.join(fixture, ref.name);
  mkdirSync(dir, { recursive: true });
  for (const part of ref.parts) copyFileSync(sources[part.name], path.join(dir, part.name));
  const args = ['merge', '--family', ref.options.family, ...ref.parts.flatMap(p => ['--part', p.name, '--take', p.take]), '--spacing', ref.options.spacing,
    ...(ref.options.allow_misaligned ? ['--allow-misaligned'] : []), ...ref.options.outputs.flatMap(o => ['-o', o]), '--report', 'json'];
  ref.json = execFileSync(cli, args, { cwd: dir, encoding: 'utf8', maxBuffer: 32 << 20 });
  ref.progress = ['read', 'read', 'merge', ...ref.options.outputs.map(() => 'encode')];
  ref.outputs = Object.fromEntries(ref.options.outputs.map(name => {
    const bytes = readFileSync(path.join(dir, name));
    return [name, { bytes: bytes.length, sha256: sha256(bytes) }];
  }));
}
// Count through info's machine-readable font list; no binding-produced references.
const references = { info, preview, glyphs: info.fonts[0].glyph_count, merges };
save(path.join(fixture, 'references.json'), references);
const evidence = { ...metadata(), cli: { path: cli, sha256: sha256(readFileSync(cli)) }, inputs: Object.fromEntries(Object.entries(sources).map(([name, file]) => [name, { path: file, bytes: readFileSync(file).length, sha256: sha256(readFileSync(file)) }])), references, browsers: [] };
const server = await serve({ '/pkg/': PKG, '/cases/': path.join(ROOT, 'crates/fontpixel-wasm/tests'), '/examples/': path.join(ROOT, 'examples/js'), '/fixtures/': fixture });
try {
  for (const engine of (process.env.FONTPIXEL_BROWSER_ENGINES ?? 'chromium,firefox,webkit').split(',')) {
    const browser = await browserFor(engine);
    const record = { engine, version: browser.version(), cases: [], errors: [] };
    evidence.browsers.push(record);
    try {
      for (const isolated of [true, false]) {
        const context = await browser.newContext();
        try {
          const page = await context.newPage();
          page.on('pageerror', error => record.errors.push(String(error)));
          page.on('console', message => { if (message.type() === 'error') record.errors.push(message.text()); });
          await page.goto(server.url + (isolated ? '/' : '/plain'));
          const result = await page.evaluate(async large => (await import('/cases/browser-cases.mjs')).runBrowserCases(large), isolated);
          assert.equal(result.isolated, isolated);
          if (isolated) result.recipes = await page.evaluate(async () => {
            const fp = await import('/pkg/index.js');
            const compat = await import('/pkg/compat_bdfparser.js');
            const bytes = new Uint8Array(await (await fetch('/fixtures/s.bdf')).arrayBuffer());
            return (await import('/examples/recipes.mjs')).recipes(fp, bytes, compat);
          });
          record.cases.push(result);
          console.log(`${engine} ${browser.version()} isolated=${isolated}: ${result.checks.length} checks`);
        } finally { await context.close(); }
      }
      assert.deepEqual(record.errors, []);
    } catch (error) { record.failure = String(error.stack ?? error); throw error; }
    finally { await browser.close(); }
  }
  assert.deepEqual(server.errors, []);
} finally {
  evidence.serverErrors = server.errors;
  save(process.env.FONTPIXEL_BROWSER_RESULTS ?? path.join(ROOT, 'benches/results/phase8-browser.json'), evidence);
  await server.close();
}
