// Sidecar resources: named collections, synchronous resolvers and the
// asynchronous re-run protocol (withAsyncResources).
import assert from 'node:assert/strict';
import { test } from 'node:test';
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
