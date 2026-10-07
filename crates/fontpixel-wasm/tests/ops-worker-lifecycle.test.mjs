import assert from 'node:assert/strict';
import test from 'node:test';
import { load } from './helpers.mjs';
const fp = await load();
const module = await WebAssembly.compile(new Uint8Array([0,97,115,109,1,0,0,0]));
const workerUrl = new URL('./fixtures/protocol-worker.mjs', import.meta.url);
const make = (cancellation = 'cooperative') => fp.createWorker({ module, workerUrl, cancellation });

test('worker clone failures release pending calls and allow reuse', { timeout: 5000 }, async () => {
  const w = await make();
  try {
    await assert.rejects(w.info(new Uint8Array([0]), { callback: () => {} }), { name: 'DataCloneError' });
    assert.equal(w.pending, 0);
    assert.deepEqual(await w.info(new Uint8Array([0])), { ok: true });
  } finally { w.terminate(); }
});

test('worker fatal traps reject every queued request and restart cleanly', { timeout: 5000 }, async () => {
  const w = await make();
  try {
    const results = await Promise.allSettled([w.info(new Uint8Array([9])), w.info(new Uint8Array([13]))]);
    assert.equal(results.length, 2);
    for (const r of results) {
      assert.equal(r.status, 'rejected');
      assert.match(r.reason.message, /simulated wasm trap/);
    }
    assert.equal(w.pending, 0);
    assert.deepEqual(await w.info(new Uint8Array([0])), { ok: true });
  } finally { w.terminate(); }
});

test('terminating during startup rejects startup and never leaves a usable worker', { timeout: 5000 }, async () => {
  const w = new fp.FontpixelWorker(module, { workerUrl, cooperative: false });
  const started = w.start();
  w.terminate();
  await assert.rejects(started, fp.CancelledError);
  await assert.rejects(w.info(new Uint8Array([0])), fp.InvalidInputError);
  assert.equal(w.pending, 0);
});

for (const cancellation of ['cooperative', 'terminate']) {
  test(`worker progress preserves thrown undefined (${cancellation})`, { timeout: 5000 }, async () => {
    const w = await make(cancellation);
    try {
      const [result] = await Promise.allSettled([w.merge([], {}, { onProgress() { throw undefined; } })]);
      assert.equal(result.status, 'rejected');
      assert.equal(result.reason, undefined);
      assert.equal(w.pending, 0);
    } finally { w.terminate(); }
  });
}
