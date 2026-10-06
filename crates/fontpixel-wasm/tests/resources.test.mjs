// Sidecar resources: named collections, synchronous resolvers and the
// asynchronous re-run protocol (withAsyncResources).
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { gzipSync } from 'node:zlib';
import { HAS_CLI, SPLEEN, buildInfo, cliFiles, load, sample } from './helpers.mjs';

const fp = await load();
const FULL = buildInfo().profile === 'full';

// An HBF header plus its font.bin sidecar, written by the native CLI.
function hbfFiles() {
  const out = cliFiles({ 'in.bdf': sample(SPLEEN) }, ['convert', 'in.bdf', '-o', 'font.hbf']);
  assert.equal(out.code, 0, out.stderr);
  assert.deepEqual(Object.keys(out.files).sort(), ['font.bin', 'font.hbf']);
  return out.files;
}

const skip = (!FULL && 'needs the full build (hbf)') || (!HAS_CLI && 'native CLI not found');

test('sidecars from objects, Maps, functions and resolver objects', { skip }, () => {
  const files = hbfFiles();
  const header = files['font.hbf'];
  const expected = fp.read(sample(SPLEEN)).fonts.font(0).glyphIds().length;
  const glyphs = (loaded) => loaded.fonts.font(0).glyphIds().length;
  assert.equal(glyphs(fp.read(header, { format: 'hbf', resources: { 'font.bin': files['font.bin'] } })), expected);
  assert.equal(glyphs(fp.read(header, { name: 'font.hbf', resources: new Map([['font.bin', files['font.bin'].buffer]]) })), expected);
  const calls = [];
  const resolve = (path, maxBytes) => {
    calls.push([path, maxBytes]);
    return files[path];
  };
  assert.equal(glyphs(fp.read(header, { format: 'hbf', resources: resolve })), expected);
  assert.equal(calls[0][0], 'font.bin');
  assert.equal(typeof calls[0][1], 'number');
  const object = { resolve: (path) => files[path] };
  assert.equal(glyphs(fp.read(header, { format: 'hbf', resources: object })), expected);
});

test('missing sidecars, resolver exceptions and resource limits', { skip }, () => {
  const files = hbfFiles();
  const header = files['font.hbf'];
  assert.throws(
    () => fp.read(header, { format: 'hbf' }),
    (e) => e instanceof fp.MissingResourceError && e.path === 'font.bin',
  );
  assert.throws(() => fp.read(header, { format: 'hbf', resources: () => null }), fp.MissingResourceError);
  assert.throws(
    () =>
      fp.read(header, {
        format: 'hbf',
        resources: () => {
          throw new fp.MissingResourceError('not here', { kind: 'MissingResource' });
        },
      }),
    fp.MissingResourceError,
  );
  // Any other exception is rethrown unchanged once the call returns.
  const boom = new RangeError('resolver failed');
  assert.throws(
    () =>
      fp.read(header, {
        format: 'hbf',
        resources: () => {
          throw boom;
        },
      }),
    (e) => e === boom,
  );
  assert.throws(
    () => fp.read(header, { format: 'hbf', resources: () => 'not bytes' }),
    (e) => e instanceof fp.InvalidInputError && /resolve\(\) result/.test(e.message),
  );
  assert.throws(
    () => fp.read(header, { format: 'hbf', resources: files, limits: { max_resources: 0 } }),
    (e) => e instanceof fp.ResourceLimitError && e.resource === 'resource count' && e.limit === 0,
  );
  assert.throws(
    () => fp.read(header, { format: 'hbf', resources: files, limits: { max_resource_bytes: 16 } }),
    fp.ResourceLimitError,
  );
  assert.throws(() => fp.read(header, { format: 'hbf', resources: { '../x': new Uint8Array() } }), fp.InvalidInputError);
});

test('withAsyncResources re-runs until nothing new is requested', { skip }, async () => {
  const files = hbfFiles();
  const header = files['font.hbf'];
  const fetched = [];
  const resolveAsync = async (path) => {
    fetched.push(path);
    await new Promise((r) => setTimeout(r, 1));
    return files[path] ?? null;
  };
  let runs = 0;
  const loaded = await fp.withAsyncResources((resources) => {
    runs += 1;
    return fp.read(header, { format: 'hbf', resources });
  }, resolveAsync);
  assert.equal(runs, 2);
  assert.deepEqual(fetched, ['font.bin']);
  assert.deepEqual(loaded.fonts.toDict(), fp.read(header, { format: 'hbf', resources: files }).fonts.toDict());
  // A name the async source does not have: the final run's error surfaces.
  await assert.rejects(
    fp.withAsyncResources((resources) => fp.read(header, { format: 'hbf', resources }), async () => null),
    fp.MissingResourceError,
  );
  // Preloaded files need no fetch.
  let calls = 0;
  await fp.withAsyncResources(
    (resources) => fp.read(header, { format: 'hbf', resources }),
    async () => {
      calls += 1;
      return null;
    },
    { files: { 'font.bin': files['font.bin'] } },
  );
  assert.equal(calls, 0);
  await assert.rejects(
    fp.withAsyncResources((resources) => fp.read(header, { format: 'hbf', resources }), resolveAsync, { maxRounds: 1 }),
    fp.ResourceLimitError,
  );
});

// ---------------------------------------------------------------------------
// Inputs, resource sets, read contexts and output bundles (mirrors
// crates/fontpixel-py/tests/test_resources.py).

const bytes = (text) => new TextEncoder().encode(text);
const text = (data) => new TextDecoder().decode(data);
const invalid = (pattern) => (e) => e instanceof fp.InvalidInputError && e.kind === 'InvalidInput' && pattern.test(e.message);

test('resource paths and default limits', () => {
  assert.equal(fp.resourcePath('fonts/a.bdf'), 'fonts/a.bdf');
  for (const bad of ['', '/abs', 'a//b', 'a/./b', '../x', 'a\\b', 'c:x', 'a\nb', 'dir/']) {
    assert.throws(() => fp.resourcePath(bad), fp.InvalidInputError, JSON.stringify(bad));
  }
  const limits = fp.defaultResourceLimits();
  assert.deepEqual(Object.keys(limits).sort(), [
    'max_decompressed_bytes',
    'max_glyphs',
    'max_input_bytes',
    'max_pixels_per_glyph',
    'max_resource_bytes',
    'max_resources',
    'max_table_bytes',
    'max_total_bytes',
  ]);
  assert.equal(limits.max_resources, 1024);
  assert.equal(limits.max_input_bytes, 64 * 1024 * 1024);
  const transport = fp.defaultTransportLimits();
  assert.deepEqual(transport.resources, limits);
  assert.equal(transport.max_output_bytes, limits.max_input_bytes);
});

test('ResourceSet resolver: copies, listing, budgets, shared with readers', () => {
  const rs = new fp.ResourceSet({ 'a.txt': bytes('A'), 'dir/b.txt': bytes('BB').buffer, 'dir/sub/c.txt': new DataView(bytes('C').buffer) });
  assert.equal(rs.length, 3);
  assert.equal(rs.has('dir/b.txt'), true);
  assert.equal(rs.has('nope'), false);
  assert.equal(rs.has('../x'), false);
  assert.deepEqual(rs.paths(), ['a.txt', 'dir/b.txt', 'dir/sub/c.txt']);
  assert.equal(text(rs.get('dir/b.txt')), 'BB');
  assert.equal(rs.get('x'), undefined);
  assert.equal(text(rs.resolve('a.txt')), 'A');
  assert.throws(() => rs.resolve('missing.txt'), (e) => e instanceof fp.MissingResourceError && e.kind === 'MissingResource' && e.path === 'missing.txt');
  assert.throws(() => rs.resolve('dir/b.txt', 1), (e) => e instanceof fp.ResourceLimitError && e.resource === 'resource bytes' && e.limit === 1);
  assert.deepEqual(rs.list(), ['a.txt']);
  assert.deepEqual(rs.list('dir'), ['dir/b.txt']);
  assert.throws(() => rs.list(null, 0), (e) => e instanceof fp.ResourceLimitError && e.resource === 'directory entries' && e.limit === 0);
  assert.throws(() => rs.list(null, 10, 8), (e) => e instanceof fp.ResourceLimitError && e.resource === 'directory listing bytes');
  // The result is a copy: editing it does not change the set.
  const copy = rs.get('a.txt');
  copy[0] = 0;
  assert.equal(text(rs.get('a.txt')), 'A');
  rs.insert('z.txt', bytes('Z'));
  assert.equal(rs.remove('a.txt'), true);
  assert.equal(rs.remove('a.txt'), false);
  assert.throws(() => rs.insert('../escape', new Uint8Array()), fp.InvalidInputError);
  assert.throws(() => rs.insert('ok', [1, 2]), invalid(/Uint8Array/));
  assert.match(rs.toString(), /z\.txt/);
  assert.deepEqual(new fp.ResourceSet(new Map([['m.bin', new Uint8Array([1])]])).paths(), ['m.bin']);
  assert.throws(() => new fp.ResourceSet({ '/abs': new Uint8Array() }), fp.InvalidInputError);
  assert.throws(() => new fp.ResourceSet(() => null), invalid(/object or Map/));
});

test('Input handles: name, sidecars, directory entry points', () => {
  const i = new fp.Input(bytes('abc'), { name: 'font.bdf' });
  assert.equal(i.name, 'font.bdf');
  assert.equal(i.isDirectory, false);
  assert.equal(text(i.data), 'abc');
  assert.equal(i.length, 3);
  const j = i.withName('other.pcf').withResources({ x: bytes('1') });
  assert.equal(j.name, 'other.pcf');
  assert.equal(i.name, 'font.bdf');
  const d = fp.Input.directory('fontdir', new fp.ResourceSet({ a: bytes('1') }));
  assert.equal(d.isDirectory, true);
  assert.equal(d.length, 0);
  assert.equal(d.name, 'fontdir');
  assert.throws(() => new fp.Input(new Uint8Array(), { resources: 42 }), invalid(/resources: expected/));
  assert.throws(() => new fp.Input(new Uint8Array(), { resources: new fp.Font() }), invalid(/expected a ResourceSet or its plain data, got a Font/));
  assert.throws(() => new fp.Input(new Uint8Array(), { names: 'x' }), invalid(/unknown option `names`/));
  assert.equal(i.toString(), 'Input(bytes=3, name="font.bdf", directory=false)');
});

test('ReadContext accounting matches the core: retained bytes, counts, budgets', () => {
  const rs = new fp.ResourceSet({ 'side.bin': bytes('x'.repeat(10)), 'd/one': bytes('1'), 'd/two': bytes('22') });
  const ctx = new fp.ReadContext(new fp.Input(bytes('primary'), { name: 'p.fnt', resources: rs }));
  assert.equal(ctx.name, 'p.fnt');
  assert.equal(ctx.isDirectory, false);
  assert.equal(text(ctx.bytes()), 'primary');
  assert.deepEqual(ctx.limits, fp.defaultResourceLimits());
  assert.equal(ctx.retainedInputBytes(), 7);
  assert.equal(text(ctx.resource('side.bin')), 'x'.repeat(10));
  assert.equal(ctx.retainedInputBytes(), 17);
  assert.throws(() => ctx.resourceLimited('side.bin', 5), (e) => e instanceof fp.ResourceLimitError && e.resource === 'resource bytes' && e.limit === 5);
  assert.deepEqual(ctx.listResourcesLimited('d', 10, 10_000), ['d/one', 'd/two']);
  const listed = ctx.retainedInputBytes();
  assert.ok(listed > 17);
  assert.throws(() => ctx.resource('absent'), fp.MissingResourceError);
  assert.equal(ctx.retainedInputBytes(), listed);
  // Later changes to a shared ResourceSet are seen by later operations only.
  rs.insert('late.bin', bytes('LL'));
  assert.equal(text(ctx.resource('late.bin')), 'LL');
  assert.equal(ctx.retainedInputBytes(), listed + 2);
  // max_resources counts every resource operation, including failures.
  const small = new fp.ReadContext(new fp.Input(bytes('p'), { resources: rs }), { max_resources: 2 });
  small.resource('d/one');
  assert.throws(() => small.resource('absent'), fp.MissingResourceError);
  assert.throws(() => small.resource('d/one'), (e) => e instanceof fp.ResourceLimitError && e.resource === 'resource count' && e.limit === 2);
  assert.equal(small.retainedInputBytes(), 2);
  // Aggregate budget: the primary plus every resolved sidecar.
  const tight = new fp.ReadContext(new fp.Input(bytes('pp'), { resources: rs }), { max_total_bytes: 13 });
  tight.resource('side.bin');
  assert.throws(() => tight.resource('d/two'), (e) => e instanceof fp.ResourceLimitError && e.resource === 'resource bytes' && e.limit === 1);
  assert.equal(text(tight.resource('d/one')), '1');
  assert.equal(tight.retainedInputBytes(), 13);
  assert.throws(() => new fp.ReadContext(bytes('12345'), { max_input_bytes: 4 }), (e) => e instanceof fp.ResourceLimitError && e.resource === 'primary input bytes' && e.limit === 4);
  assert.throws(() => new fp.ReadContext(bytes('x')).resource('side.bin'), fp.MissingResourceError);
  assert.throws(() => ctx.resource('../x'), fp.InvalidInputError);
  assert.throws(() => ctx.resourceLimited('side.bin', 2 ** 32), invalid(/out of range/));
  assert.equal(new fp.ReadContext(bytes('x')).toString(), 'ReadContext(name=null, directory=false)');
  const dir = new fp.ReadContext(fp.Input.directory('fontdir', rs));
  assert.equal(dir.isDirectory, true);
  assert.equal(dir.retainedInputBytes(), 0);
  assert.deepEqual(dir.listResourcesLimited(null, 10, 10_000), ['late.bin', 'side.bin']);
});

test('ReadContext decodes a gzip transport envelope under its budgets', () => {
  const raw = bytes('STARTFONT 2.1\n'.repeat(20));
  const packed = new Uint8Array(gzipSync(raw));
  let ctx;
  try {
    ctx = new fp.ReadContext(packed);
  } catch (e) {
    // Builds without the gzip transport say so explicitly.
    assert.ok(e instanceof fp.FeatureDisabledError && e.format === 'gzip', String(e));
    return;
  }
  assert.deepEqual(ctx.bytes(), raw);
  assert.ok(ctx.retainedInputBytes() > raw.length);
  assert.throws(() => new fp.ReadContext(packed, { max_decompressed_bytes: 10 }), fp.ResourceLimitError);
});

test('JS resolvers behind a ReadContext: missing, exceptions, listing, byte budgets', () => {
  const calls = [];
  const boom = new RangeError('boom');
  const resolver = {
    resolve(path, maxBytes) {
      calls.push([path, maxBytes]);
      if (path === 'boom') throw boom;
      return path === 'none' ? null : bytes(`data:${path}`);
    },
    list(directory, maxEntries, maxBytes) {
      calls.push(['list', directory, maxEntries, typeof maxBytes]);
      return directory === null ? ['a', 'b'] : [`${directory}/c`];
    },
  };
  const ctx = new fp.ReadContext(new fp.Input(bytes('p'), { resources: resolver }));
  assert.equal(text(ctx.resource('x/y')), 'data:x/y');
  assert.equal(calls[0][0], 'x/y');
  assert.throws(() => ctx.resource('none'), fp.MissingResourceError);
  // Any other exception aborts the call and is rethrown unchanged.
  assert.throws(() => ctx.resource('boom'), (e) => e === boom);
  assert.deepEqual(ctx.listResourcesLimited(null, 5, 10_000), ['a', 'b']);
  assert.deepEqual(ctx.listResourcesLimited('q', 5, 10_000), ['q/c']);
  // Earlier operations are replayed from their recorded results: the resolver
  // ran exactly once per operation.
  assert.equal(calls.length, 5);
  const before = ctx.retainedInputBytes();
  assert.equal(calls.length, 5);
  assert.ok(before > 1);
  const missingThrow = () => {
    throw new fp.MissingResourceError('not here', { kind: 'MissingResource' });
  };
  assert.throws(() => new fp.ReadContext(new fp.Input(bytes('p'), { resources: missingThrow })).resource('k'), fp.MissingResourceError);
  const noList = new fp.ReadContext(new fp.Input(bytes('p'), { resources: () => bytes('1') }));
  assert.equal(text(noList.resource('k')), '1');
  assert.throws(() => noList.listResourcesLimited(null, 5, 100), invalid(/enumeration is unavailable/));
  const tooBig = new fp.ReadContext(new fp.Input(bytes('p'), { resources: () => bytes('x'.repeat(100)) }), { max_resource_bytes: 8 });
  assert.throws(() => tooBig.resource('k'), (e) => e instanceof fp.ResourceLimitError && e.limit === 8);
  const badList = new fp.ReadContext(new fp.Input(bytes('p'), { resources: { resolve: () => null, list: () => ['../up'] } }));
  assert.throws(() => badList.listResourcesLimited(null, 5, 1000), fp.InvalidInputError);
  const escaping = new fp.ReadContext(new fp.Input(bytes('p'), { resources: { resolve: () => null, list: () => ['other/x'] } }));
  assert.throws(() => escaping.listResourcesLimited('d', 5, 1000), invalid(/escapes its immediate directory/));
});

test('a resolver re-entering its running ReadContext gets InvalidInputError (re-entrant borrow)', () => {
  let ctx;
  let inner;
  const resolver = (path) => {
    if (path === 'outer') {
      try {
        ctx.retainedInputBytes();
      } catch (e) {
        inner = e;
        throw e;
      }
    }
    return bytes('ok');
  };
  ctx = new fp.ReadContext(new fp.Input(bytes('p'), { resources: resolver }));
  assert.throws(
    () => ctx.resource('outer'),
    (e) => e === inner && e instanceof fp.InvalidInputError && /ReadContext is in use by the running fontpixel call/.test(e.message),
  );
  // The failed operation still counted; the context stays usable.
  assert.equal(text(ctx.resource('next')), 'ok');
  assert.equal(ctx.retainedInputBytes(), 1 + 2);
});

test('read() accepts a ResourceSet as sidecar resources (shared, not consumed)', { skip }, () => {
  const files = hbfFiles();
  const set = new fp.ResourceSet({ 'font.bin': files['font.bin'] });
  const expected = fp.read(files['font.hbf'], { format: 'hbf', resources: files }).fonts.toDict();
  assert.deepEqual(fp.read(files['font.hbf'], { format: 'hbf', resources: set }).fonts.toDict(), expected);
  assert.equal(set.length, 1);
});

test('output bundles, assets and Written', () => {
  const a = new fp.OutputAsset('font.fnt', bytes('F'));
  assert.equal(a.path, 'font.fnt');
  assert.equal(text(a.data), 'F');
  assert.equal(a.length, 1);
  assert.ok(a.equals(new fp.OutputAsset('font.fnt', bytes('F').buffer)));
  assert.equal(a.equals('font.fnt'), false);
  const bundle = new fp.OutputBundle('font.fnt', [a, ['pages/0.png', bytes('PNG')]]);
  assert.equal(bundle.primary, 'font.fnt');
  assert.equal(bundle.isDirectory, false);
  assert.equal(bundle.length, 2);
  assert.deepEqual([...bundle].map((x) => x.path), ['font.fnt', 'pages/0.png']);
  assert.deepEqual(bundle.intoAssets().map((x) => x.path), bundle.assets.map((x) => x.path));
  assert.equal(a.length, 1); // the asset handle was borrowed, not consumed
  assert.ok(bundle.retainedBytes() > 4);
  assert.throws(() => bundle.retainedBytes(1), (e) => e instanceof fp.ResourceLimitError && e.resource === 'retained output bundle bytes' && e.limit === 1);
  assert.throws(() => bundle.intoSingle(), (e) => e instanceof fp.MultipleAssetsError && e.count === 2);
  assert.throws(() => new fp.OutputBundle('missing.fnt', [a]), invalid(/missing its primary asset/));
  assert.throws(() => new fp.OutputBundle('font.fnt', [a, a]), invalid(/duplicate output asset/));
  assert.throws(() => new fp.OutputBundle('a', [['a', bytes('1')], ['a/b', bytes('2')]]), invalid(/conflicts with an ancestor file/));
  assert.throws(() => new fp.OutputBundle('font.fnt', [['../x', bytes('1')]]), fp.InvalidInputError);
  assert.throws(() => new fp.OutputBundle('font.fnt', [42]), invalid(/OutputAsset or a \[path, bytes\] pair/));
  const d = new fp.OutputBundle('out', [['out/a', bytes('1')]], { directory: true });
  assert.equal(d.isDirectory, true);
  assert.equal(d.primary, 'out');
  assert.throws(() => d.intoSingle(), invalid(/directory output cannot be flattened/));
  assert.throws(() => new fp.OutputBundle('out', [['elsewhere/a', bytes('1')]], { directory: true }), invalid(/beneath its entry point/));
  const single = fp.OutputBundle.single('x.bdf', bytes('BDF'));
  assert.equal(text(single.intoSingle()), 'BDF');
  const data = bundle.toDict();
  assert.equal(data.primary, 'font.fnt');
  assert.deepEqual(data.assets[0], { path: 'font.fnt', bytes: [70] });
  assert.ok(fp.OutputBundle.fromDict(data).equals(bundle));
  assert.throws(() => fp.OutputBundle.fromDict({ ...data, primary: '../x' }), fp.InvalidInputError);
  const w = new fp.Written(single, { entries: [] });
  assert.ok(w.output.equals(single));
  assert.deepEqual(w.report, { entries: [] });
  assert.equal(w.toDict().output.primary, 'x.bdf');
  assert.deepEqual(new fp.Written(single.toDict()).toDict(), w.toDict());
  assert.throws(() => new fp.Written(a), invalid(/expected a OutputBundle or its plain data, got a OutputAsset/));
  assert.match(w.toString(), /^Written\(output=OutputBundle\(/);
  assert.match(a.toString(), /^OutputAsset\(path=/);
});

test('resource handles: release, Symbol.dispose and use after free', () => {
  const rs = new fp.ResourceSet({ 'a.bin': bytes('A') });
  const input = new fp.Input(bytes('p'), { resources: rs });
  const ctx = new fp.ReadContext(input);
  rs.free();
  // The input and context share the set's storage, which outlives the handle.
  assert.equal(text(ctx.resource('a.bin')), 'A');
  assert.throws(() => rs.paths(), invalid(/ResourceSet handle was released/));
  assert.throws(() => new fp.Input(bytes('p'), { resources: rs }), invalid(/ResourceSet handle was released/));
  input[Symbol.dispose]();
  assert.throws(() => new fp.ReadContext(input), invalid(/Input handle was released/));
  ctx.free();
  ctx.free();
  assert.throws(() => ctx.retainedInputBytes(), invalid(/ReadContext handle was released/));
});
