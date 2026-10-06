// Foundation conventions: loading, version, capabilities, errors, plain data
// (numbers, maps, bytes), Worker use. Area suites live in <area>.test.mjs.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { pathToFileURL } from 'node:url';
import { Worker } from 'node:worker_threads';
import { BLOCK, PKG, REPO, buildInfo, has, load, sample } from './helpers.mjs';

const fp = await load();

test('version matches the workspace', () => {
  const manifest = readFileSync(path.join(REPO, 'Cargo.toml'), 'utf8');
  const version = /^version = "(.*)"/m.exec(manifest)[1];
  assert.equal(fp.version(), version);
  assert.equal(buildInfo().wasm_bindgen, '0.2.129');
});

test('init is idempotent and initSync after init is a no-op', async () => {
  await fp.init();
  fp.initSync();
  assert.equal(typeof fp.wasmUrl.href, 'string');
});

test('capabilities are explicit records', () => {
  const caps = fp.capabilities();
  const names = caps.map((c) => c.name);
  for (const name of ['filesystem', 'threads', 'toml_batch', 'compat_bdfparser', 'cjk_charsets', 'sync_resolvers']) {
    assert.ok(names.includes(name), name);
  }
  for (const cap of caps) {
    assert.deepEqual(Object.keys(cap).sort(), ['alternative', 'available', 'description', 'evidence', 'name', 'reason']);
    if (!cap.available) {
      assert.ok(cap.reason && cap.evidence && cap.alternative, cap.name);
    }
  }
  const byName = Object.fromEntries(caps.map((c) => [c.name, c]));
  assert.equal(byName.filesystem.available, false);
  assert.equal(byName.threads.available, false);
  assert.equal(byName.sync_resolvers.available, true);
  const info = buildInfo();
  assert.equal(byName.toml_batch.available, info.profile === 'full');
  assert.equal(byName.compat_bdfparser.available, info.profile === 'full');
});

test('requireCapability throws UnsupportedError with reason and alternative', () => {
  assert.throws(
    () => fp.requireCapability('filesystem'),
    (e) => {
      assert.ok(e instanceof fp.UnsupportedError);
      assert.ok(e instanceof fp.FontpixelError);
      assert.ok(e instanceof Error);
      assert.equal(e.name, 'UnsupportedError');
      assert.equal(e.kind, 'Unsupported');
      assert.equal(e.exitCode, 1);
      assert.equal(e.capability, 'filesystem');
      assert.ok(e.reason.length > 0 && e.evidence.length > 0 && e.alternative.length > 0);
      assert.match(e.message, /Alternative:/);
      return true;
    },
  );
  fp.requireCapability('sync_resolvers');
  assert.throws(() => fp.requireCapability('teleport'), fp.InvalidInputError);
});

test('core errors map to their classes with structured fields', () => {
  assert.throws(
    () => fp.read(sample(BLOCK), { format: 'no-such-format' }),
    (e) => e instanceof fp.UnknownFormatError && e.kind === 'UnknownFormat' && e.format === 'no-such-format',
  );
  const bad = new TextEncoder().encode('STARTFONT 2.1\nFONT x\nSIZE nope\n');
  assert.throws(
    () => fp.read(bad, { format: 'bdf' }),
    (e) => e instanceof fp.BdfParseError && e.kind === 'BdfParse' && e.line === 3 && e.exitCode === 1,
  );
  assert.throws(
    () => fp.read(sample(BLOCK), { limits: { max_input_bytes: 10 } }),
    (e) => e instanceof fp.ResourceLimitError && typeof e.resource === 'string' && e.limit === 10,
  );
  assert.throws(
    () => fp.read(new Uint8Array([1, 2, 3])),
    (e) => e instanceof fp.UnrecognizedFormatError && e.kind === 'UnrecognizedFormat',
  );
});

test('plain-data errors are InvalidInputError naming the problem', () => {
  assert.throws(
    () => fp.normalize('resources::ResourceLimits', { max_glyphs: 1, bogus: 2 }),
    (e) => e instanceof fp.InvalidInputError && /unknown field `bogus`/.test(e.message) && !/ at line /.test(e.message),
  );
  assert.throws(
    () => fp.normalize('resources::ResourceLimits', { max_glyphs: 1.5 }),
    (e) => e instanceof fp.InvalidInputError && /floating point/.test(e.message),
  );
  assert.throws(() => fp.normalize('resources::ResourceLimits', { max_glyphs: NaN }), /non-finite/);
  assert.throws(() => fp.normalize('resources::ResourceLimits', { max_glyphs: new Date() }), /not plain data/);
  assert.throws(() => fp.read(sample(BLOCK), { formt: 'bdf' }), /unknown option `formt`/);
  assert.throws(() => fp.normalize('no::Such', {}), fp.InvalidInputError);
});

test('integers: number up to 2^53 - 1, bigint beyond, per value', () => {
  const safe = Number.MAX_SAFE_INTEGER;
  assert.deepEqual(fp.normalize('model::PropertyValue', { Unsigned: safe }), { Unsigned: safe });
  assert.deepEqual(fp.normalize('model::PropertyValue', { Unsigned: 2n ** 53n }), { Unsigned: 2n ** 53n });
  assert.deepEqual(fp.normalize('model::PropertyValue', { Unsigned: 2n ** 64n - 1n }), { Unsigned: 2n ** 64n - 1n });
  assert.deepEqual(fp.normalize('model::PropertyValue', { Integer: -(2n ** 63n) }), { Integer: -(2n ** 63n) });
  assert.deepEqual(fp.normalize('model::PropertyValue', { Integer: 7n }), { Integer: 7 });
  // An integral number beyond 2^53 is not exact: rejected for integer fields.
  assert.throws(() => fp.normalize('model::PropertyValue', { Unsigned: 2 ** 60 }), /floating point/);
  assert.throws(() => fp.normalize('model::PropertyValue', { Unsigned: 2n ** 64n }), fp.InvalidInputError);
  assert.throws(() => fp.normalize('model::PropertyValue', { Unsigned: -1 }), fp.InvalidInputError);
  // usize is 32-bit on wasm32: always a number; larger inputs fail.
  const limits = fp.normalize('resources::ResourceLimits');
  for (const value of Object.values(limits)) assert.equal(typeof value, 'number');
  assert.throws(() => fp.normalize('resources::ResourceLimits', { max_glyphs: 2 ** 32 }), fp.InvalidInputError);
  assert.equal(fp.normalize('resources::ResourceLimits', { max_glyphs: 2n ** 32n - 1n }).max_glyphs, 2 ** 32 - 1);
});

test('maps: string-keyed objects; integer keys as decimal strings; Map input', { skip: !fp.optionTypes().includes('sfnt::gsub::Gsub') && 'build without sfnt' }, () => {
  const gsub = { scripts: {}, features: [], lookups: [{ 3: 4, 1: 2 }] };
  const out = fp.normalize('sfnt::gsub::Gsub', gsub);
  assert.deepEqual(out.lookups, [{ 1: 2, 3: 4 }]);
  assert.deepEqual(Object.keys(out.lookups[0]), ['1', '3']);
  const viaMap = fp.normalize('sfnt::gsub::Gsub', { ...gsub, lookups: [new Map([[1, 2], [3n, 4], ['5', 6]])] });
  assert.deepEqual(viaMap.lookups, [{ 1: 2, 3: 4, 5: 6 }]);
  assert.deepEqual(fp.normalize('sfnt::gsub::Gsub', out), out);
  assert.throws(() => fp.normalize('sfnt::gsub::Gsub', { ...gsub, lookups: [{ x: 1 }] }), fp.InvalidInputError);
  assert.throws(() => fp.normalize('sfnt::gsub::Gsub', { ...gsub, lookups: [new Map([[1.5, 2]])] }), /map key/);
});

test('flattened and tagged requests keep exact numbers and strict keys', () => {
  const request = fp.normalize('ops::RenderRequest', { input: 'a.bdf', output: 'a.png', text: 'A', line_limit: -5 });
  assert.equal(request.line_limit, -5);
  assert.equal(request.scale, 1);
  assert.throws(
    () => fp.normalize('ops::RenderRequest', { input: 'a.bdf', output: 'a.png', text: 'A', bogus: 1 }),
    /unknown field `bogus`/,
  );
  const task = fp.normalize('ops::TaskRequest', {
    kind: 'convert',
    inputs: ['a.bdf'],
    output: 'a.pcf',
    max_total_bytes: 4096,
  });
  assert.equal(task.kind, 'convert');
  assert.equal(task.max_total_bytes, 4096);
  assert.equal(task.font_index, null);
  assert.throws(() => fp.normalize('ops::TaskRequest', { kind: 'teleport' }), fp.InvalidInputError);
});

test('bytes: Uint8Array, ArrayBuffer and DataView in; Uint8Array out', () => {
  const bytes = sample(BLOCK);
  const a = fp.read(bytes).fonts.toDict();
  const b = fp.read(bytes.slice().buffer).fonts.toDict();
  const c = fp.read(new DataView(bytes.slice().buffer)).fonts.toDict();
  assert.deepEqual(a, b);
  assert.deepEqual(a, c);
  assert.throws(() => fp.read([...bytes]), (e) => e instanceof fp.InvalidInputError && /Uint8Array/.test(e.message));
  const raster = fp.Bitmap.fromText('#.\n.#');
  const out = raster.asBytes();
  assert.ok(out instanceof Uint8Array);
  assert.deepEqual([...out], [0x80, 0x40]);
  // Byte lists inside plain data: number arrays out, buffers accepted in.
  const dict = raster.toDict();
  assert.deepEqual(dict, { mono: { width: 2, height: 2, data: [0x80, 0x40] } });
  const back = fp.Bitmap.fromDict({ mono: { width: 2, height: 2, data: new Uint8Array([0x80, 0x40]) } });
  assert.ok(back.equals(raster));
});

test('a compiled module is shared with a worker thread (initSync)', async () => {
  const module = await WebAssembly.compile(readFileSync(path.join(PKG, 'fontpixel_wasm_bg.wasm')));
  const url = pathToFileURL(path.join(PKG, 'index.js')).href;
  const source = `
    import { parentPort, workerData } from 'node:worker_threads';
    const fp = await import(workerData.url);
    fp.initSync(workerData.module);
    const loaded = fp.read(workerData.bytes);
    parentPort.postMessage({ version: fp.version(), glyphs: loaded.fonts.font(0).length });
  `;
  const worker = new Worker(new URL(`data:text/javascript,${encodeURIComponent(source)}`), {
    workerData: { url, module, bytes: sample(BLOCK) },
  });
  const message = await new Promise((resolve, reject) => {
    worker.once('message', resolve);
    worker.once('error', reject);
  });
  await worker.terminate();
  assert.equal(message.version, fp.version());
  assert.equal(message.glyphs, fp.read(sample(BLOCK)).fonts.font(0).length);
});
