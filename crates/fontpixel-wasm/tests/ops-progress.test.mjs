// Progress callbacks and cancellation of `ops.merge`/`ops.convert`
// (fontpixel_ops::merge_with_progress / convert_with_progress), the JS
// counterpart of crates/fontpixel-py/tests/test_progress.py: a `Progress`
// record per completed step, exactly `false` cancels with CancelledError, a
// throw cancels and is rethrown unchanged, results never depend on the
// callback.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { BLOCK, SPLEEN, buildInfo, load, sample } from './helpers.mjs';

const fp = await load();
const { ops } = fp;
const MINIMAL = buildInfo().profile === 'minimal';
const FILES = { 'fonts/s.bdf': sample(SPLEEN), 'fonts/b.bdf': sample(BLOCK) };
const KEYS = ['bytes', 'done', 'glyphs', 'item', 'step', 'total'];
// The minimal profile writes BDF/PCF/YAFF only.
const SECOND = MINIMAL ? 'out/m.pcf' : 'out/m.woff2';

function mergeRequest() {
  return {
    family: 'Mix',
    parts: [
      { input: 'fonts/s.bdf', take: 'western' },
      { input: 'fonts/b.bdf', take: 'all', dy: -1 },
    ],
    outputs: [{ destination: 'out/m.bdf' }, { destination: SECOND }],
  };
}

function convertRequest() {
  return { inputs: ['fonts/s.bdf', 'fonts/b.bdf'], output: 'out', to: 'pcf', each: true };
}

test('merge progress reports each step and equals merge', () => {
  const plain = ops.merge(mergeRequest(), { files: FILES });
  const events = [];
  const observed = ops.merge(mergeRequest(), { files: FILES, progress: (e) => events.push(e) });
  assert.deepEqual(observed.outputs, plain.outputs);
  assert.equal(observed.json(true), plain.json(true));
  assert.equal(observed.text(), plain.text());
  for (const event of events) assert.deepEqual(Object.keys(event).sort(), KEYS);
  assert.deepEqual(
    events.map((e) => [e.step, e.done, e.total]),
    [['read', 1, 2], ['read', 2, 2], ['merge', 1, 1], ['encode', 1, 2], ['encode', 2, 2]],
  );
  assert.deepEqual(events.map((e) => e.item), ['fonts/s.bdf', 'fonts/b.bdf', null, 'out/m.bdf', SECOND]);
  assert.equal(events[2].glyphs, plain.summary.glyphs);
  assert.deepEqual(events.slice(3).map((e) => e.bytes), plain.summary.summaries.map((s) => s.bytes));
  assert.ok(events.slice(0, 2).every((e) => e.glyphs > 0 && e.bytes === null));
  // progress: undefined/null is the plain operation.
  assert.deepEqual(ops.merge(mergeRequest(), { files: FILES, progress: null }).outputs, plain.outputs);
  assert.deepEqual(ops.merge(mergeRequest(), { files: FILES, progress: undefined }).outputs, plain.outputs);
});

test('convert progress alternates reads and encodes', () => {
  const plain = ops.convert(convertRequest(), { files: FILES });
  const events = [];
  const observed = ops.convert(convertRequest(), { files: FILES, progress: (e) => events.push(e) });
  assert.deepEqual(observed.outputs, plain.outputs);
  assert.equal(observed.json(true), plain.json(true));
  assert.deepEqual(
    events.map((e) => [e.step, e.done, e.total, e.item]),
    [
      ['read', 1, 2, 'fonts/s.bdf'],
      ['encode', 1, 2, 'out/s.pcf'],
      ['read', 2, 2, 'fonts/b.bdf'],
      ['encode', 2, 2, 'out/b.pcf'],
    ],
  );
  assert.deepEqual(
    [events[1].bytes, events[3].bytes],
    [plain.outputs['out/s.pcf'].length, plain.outputs['out/b.pcf'].length],
  );
  // vectorize is convert: the same callback contract.
  const seen = [];
  ops.vectorize(convertRequest(), { files: FILES, progress: (e) => seen.push(e) });
  assert.deepEqual(seen, events);
});

test('returning false cancels at every step', () => {
  for (let stop = 0; stop < 5; stop++) {
    const calls = [];
    assert.throws(
      () =>
        ops.merge(mergeRequest(), {
          files: FILES,
          progress: (event) => {
            calls.push(event);
            return calls.length <= stop ? undefined : false;
          },
        }),
      (e) => {
        assert.ok(e instanceof fp.CancelledError, `stop ${stop}: ${e}`);
        assert.ok(e instanceof fp.FontpixelError);
        assert.equal(e.kind, 'Cancelled');
        assert.equal(e.exitCode, 1);
        return true;
      },
    );
    assert.equal(calls.length, stop + 1);
  }
});

test('only false cancels', () => {
  for (const value of [undefined, null, true, 0, '', [], 'stop', new Boolean(false)]) {
    const result = ops.convert(convertRequest(), { files: FILES, progress: () => value });
    assert.equal(Object.keys(result.outputs).length, 2);
  }
});

test('callback exceptions cancel and propagate', () => {
  class Stop extends Error {}
  let thrown;
  assert.throws(
    () =>
      ops.merge(mergeRequest(), {
        files: FILES,
        progress: (event) => {
          if (event.step === 'encode') {
            thrown = new Stop(event.item);
            throw thrown;
          }
        },
      }),
    (e) => e === thrown && e.message === 'out/m.bdf',
  );
  assert.throws(() => ops.merge(mergeRequest(), { files: FILES, progress: 42 }), (e) => e instanceof fp.InvalidInputError && /progress must be a function/.test(e.message));
  // A throw in a later step does not leak into the next call.
  assert.equal(Object.keys(ops.merge(mergeRequest(), { files: FILES, progress: () => {} }).outputs).length, 2);
  // Non-Error values are rethrown as they are.
  assert.throws(
    () =>
      ops.convert(convertRequest(), {
        files: FILES,
        progress: () => {
          throw 'plain string';
        },
      }),
    (e) => e === 'plain string',
  );
});

test('provider and progress exceptions are rethrown unchanged', () => {
  const failure = new Error('provider failed');
  const provider = {
    entry: (path) => (path in FILES ? FILES[path].length : path === 'fonts' || path === '' ? 'directory' : null),
    read: (path) => {
      if (path === 'fonts/b.bdf') throw failure;
      return FILES[path];
    },
    list: () => [],
  };
  assert.throws(
    () =>
      ops.merge(mergeRequest(), {
        files: provider,
        progress: () => {
          throw new Error('progress failed');
        },
      }),
    (e) => e.message === 'progress failed',
  );
  assert.throws(() => ops.merge(mergeRequest(), { files: provider, progress: () => undefined }), (e) => e === failure);
});
