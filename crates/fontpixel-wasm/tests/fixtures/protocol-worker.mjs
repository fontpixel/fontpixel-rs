// Deliberately controlled Worker protocol peer for client failure tests.
import { parentPort } from 'node:worker_threads';
parentPort.on('message', (m) => {
  if (m.type === 'init') setTimeout(() => parentPort.postMessage({ type: 'ready' }), 40);
  else if (m.type === 'call') {
    const code = m.args[0]?.[0];
    if (code === 9) return; // Keep queued until another request crashes.
    if (code === 13) {
      parentPort.postMessage({ type: 'error', id: m.id, fatal: true,
        error: { name: 'RuntimeError', message: 'simulated wasm trap', fields: {} } });
    } else {
      parentPort.postMessage({ type: 'progress', id: m.id, progress: { step: 'read', done: 1, total: 1 } });
      parentPort.postMessage({ type: 'result', id: m.id, result: { ok: true } });
    }
  }
});
