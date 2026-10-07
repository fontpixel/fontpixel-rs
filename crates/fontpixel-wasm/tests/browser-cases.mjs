// Runs inside each real browser. Assertions deliberately avoid Node shims.
export async function runBrowserCases(large = true) {
  const fp = await import('/pkg/index.js');
  await fp.init();
  await fp.init();
  fp.initSync(); // idempotent after asynchronous browser initialization
  const checks = [];
  const ok = (condition, label) => { if (!condition) throw new Error(label); checks.push(label); };
  const equal = (a, b, label) => ok(JSON.stringify(a) === JSON.stringify(b), label);
  const throws = (fn, type, label) => { let error; try { fn(); } catch (e) { error = e; } ok(error instanceof type, label); return error; };
  const rejects = async (fn, type, label) => { let error; try { await fn(); } catch (e) { error = e; } ok(error instanceof type, label); };
  const bytes = async (name) => new Uint8Array(await (await fetch(`/fixtures/${name}`)).arrayBuffer());
  const digest = async (data) => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', data)), b => b.toString(16).padStart(2, '0')).join('');
  const references = await (await fetch('/fixtures/references.json')).json();
  const s = await bytes('s.bdf');
  const set = fp.read(s).fonts;
  const font = set.font(0);
  const glyph = font.glyph(font.glyphIds()[0]);
  const copy = glyph.bitmap;
  const original = copy.getPixel(0, 0);
  copy.setPixel(0, 0, 1 - original);
  const before = glyph.bitmap;
  ok(before.getPixel(0, 0) === original, 'bitmap getter copies');
  before.free();
  glyph.bitmap = copy;
  const after = glyph.bitmap;
  ok(after.getPixel(0, 0) === 1 - original, 'glyph view mutation');
  after.free(); copy.free(); glyph.free(); set.free();
  ok(font.length > 0, 'view retains parent storage');
  font.free(); font.free();
  throws(() => font.length, fp.InvalidInputError, 'released handle rejected');
  const big = 2n ** 63n + 1n;
  ok(fp.normalize('model::PropertyValue', { Unsigned: big }).Unsigned === big, 'BigInt exact');
  throws(() => fp.normalize('model::PropertyValue', { Unsigned: 2 ** 60 }), fp.InvalidInputError, 'unsafe Number rejected');
  throws(() => fp.read(s, { formt: 'bdf' }), fp.InvalidInputError, 'unknown option rejected');
  const limit = throws(() => fp.info(s, { max_input_bytes: 10 }), fp.ResourceLimitError, 'input resource limit');
  ok(limit.limit === 10, 'structured limit detail');
  const loss = throws(() => fp.ops.convert({ inputs: ['s.bdf'], output: 'x.pcf', strict: true, report: 'json' }, { files: { 's.bdf': s } }), fp.StrictLossError, 'strict loss preserved');
  ok(loss.exitCode === 3 && loss.report != null, 'strict error contains report');
  equal(fp.info(s, { name: 's.bdf' }), references.info, 'info equals native CLI');
  const rendered = fp.render(s, 'Hello', { name: 's.bdf' });
  ok(rendered.art + '\n' === references.preview, 'render equals native CLI');
  const rgba = fp.render(s, 'Hello', { format: 'rgba' });
  ok(rgba.data instanceof Uint8ClampedArray && rgba.stride === rgba.width * 4, 'RGBA output');
  const header = await bytes('font.hbf');
  throws(() => fp.read(header, { format: 'hbf' }), fp.MissingResourceError, 'sidecar required');
  let rounds = 0;
  const loaded = await fp.withAsyncResources(resources => {
    rounds++;
    return fp.read(header, { format: 'hbf', resources });
  }, async name => bytes(name));
  ok(rounds === 2, 'async sidecar rerun');
  const hbfFont = loaded.fonts.font(0);
  ok(hbfFont.length === references.glyphs, 'async read result');
  hbfFont.free(); loaded.fonts.free();
  const bundle = new fp.OutputBundle('x.bin', [['x.bin', new Uint8Array([1, 2])]]);
  const [asset] = bundle.assets;
  const returned = asset.data;
  returned[0] = 9;
  ok(asset.data[0] === 1, 'output asset bytes copy');
  asset.free(); bundle.free();
  const compat = await import('/pkg/compat_bdfparser.js');
  const cf = new compat.Font();
  cf.load_bytes(s);
  ok(cf.length === references.glyphs, 'compat load in browser');
  const worker = await fp.createWorker();
  const isolated = globalThis.crossOriginIsolated;
  ok(worker.cancellation === (isolated ? 'cooperative' : 'terminate'), 'worker cancellation capability');
  const smallParts = [{ bytes: s, name: 's.bdf', take: 'all' }];
  const smallOptions = { family: 'Browser', spacing: 'prop', outputs: ['small.bdf'], report: 'json' };
  const evidence = { isolated, checks, merges: [] };
  try {
    equal(await worker.info(s, { name: 's.bdf' }), references.info, 'Worker info');
    const controller = new AbortController(); controller.abort();
    await rejects(() => worker.merge(smallParts, smallOptions, { signal: controller.signal }), fp.CancelledError, 'pre-aborted call');
    const cancelParts = large ? [{ bytes: await bytes('sc.bdf'), name: 'sc.bdf', take: 'all' }] : smallParts;
    const during = new AbortController();
    let cancelEvents = 0;
    await rejects(() => worker.merge(cancelParts, smallOptions, { signal: during.signal, onProgress() { cancelEvents++; during.abort(); } }), fp.CancelledError, 'abort from progress');
    ok(cancelEvents > 0, 'cancel received progress');
    equal(await worker.info(s, { name: 's.bdf' }), references.info, 'Worker reusable after abort');
    const active = worker.merge(cancelParts, smallOptions);
    worker.terminate();
    await rejects(() => active, fp.CancelledError, 'terminate pending call');
    await rejects(() => worker.info(s), fp.InvalidInputError, 'terminated Worker stays closed');
  } finally { worker.terminate(); }
  if (large) {
    const mergeWorker = await fp.createWorker();
    try {
      for (const ref of references.merges) {
        const parts = await Promise.all(ref.parts.map(async p => ({ ...p, bytes: await bytes(p.name) })));
        const progress = [];
        let beats = 0;
        const timer = setInterval(() => beats++, 10);
        const start = performance.now();
        let result;
        try { result = await mergeWorker.merge(parts, ref.options, { onProgress: p => progress.push(p.step) }); }
        finally { clearInterval(timer); }
        ok(beats > 0, `${ref.name}: main thread heartbeat`);
        equal(progress, ref.progress, `${ref.name}: progress steps`);
        ok(result.json === ref.json, `${ref.name}: native report identity`);
        const outputs = {};
        for (const [name, expected] of Object.entries(ref.outputs)) {
          const actual = result.files[name];
          const native = await bytes(`${ref.name}/${name}`);
          ok(actual.length === native.length && actual.every((b, i) => b === native[i]), `${ref.name}/${name}: byte identity`);
          outputs[name] = { bytes: actual.length, sha256: await digest(actual) };
          equal(outputs[name], expected, `${ref.name}/${name}: native SHA-256`);
        }
        evidence.merges.push({ name: ref.name, ms: performance.now() - start, beats, progress, outputs });
      }
    } finally { mergeWorker.terminate(); }
  }
  return evidence;
}
