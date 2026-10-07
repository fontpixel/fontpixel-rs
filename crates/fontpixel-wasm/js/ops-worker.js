// Worker module of the fontpixel package: runs `merge`, `info` and `render`
// off the page's main thread (W8.3). Started by `createWorker()` in
// ops-client.js as a browser module worker or a Node `worker_threads`
// worker; not meant to be imported directly.
//
// Protocol (structured-clone messages):
//   page -> worker  {type: 'init', module}                 WebAssembly.Module compiled once by the page
//   worker -> page  {type: 'ready'} | {type: 'init-error', error}
//   page -> worker  {type: 'call', id, op, args, cancel, progress}
//                   op: 'merge' | 'info' | 'render'; cancel: SharedArrayBuffer(4) or null;
//                   progress: whether to post progress events
//   worker -> page  {type: 'progress', id, progress}       one `Progress` record per merge step
//                   {type: 'result', id, result}           output buffers transferred
//                   {type: 'error', id, error, fatal}      {name, message, fields}; fatal: the
//                                                          instance trapped and must be discarded
//
// Cooperative cancellation: the page sets cancel[0] = 1 (Atomics.store); the
// merge progress callback reads it at the next step boundary and returns
// false, so the merge fails with CancelledError and returns nothing. A busy
// synchronous call cannot receive messages, which is why the flag lives in
// shared memory.
import * as fp from './index.js';

const parentPort =
  globalThis.process?.versions?.node !== undefined && typeof globalThis.WorkerGlobalScope === 'undefined'
    ? (await import(/* webpackIgnore: true */ /* @vite-ignore */ 'node:worker_threads')).parentPort
    : null;

const send = parentPort
  ? (message, transfer) => parentPort.postMessage(message, transfer)
  : (message, transfer) => globalThis.postMessage(message, transfer);

function serializeError(error) {
  if (error instanceof Error) {
    const fields = {};
    for (const [key, value] of Object.entries(error)) {
      if (key !== 'name' && key !== 'message' && key !== 'stack') fields[key] = value;
    }
    return { name: error.name, message: error.message, fields };
  }
  return { name: 'Error', message: String(error), fields: {} };
}

function run(op, args, flag, wantProgress, id) {
  switch (op) {
    case 'merge': {
      const [parts, options = {}] = args;
      const progress = (record) => {
        if (wantProgress) send({ type: 'progress', id, progress: record });
        if (flag && Atomics.load(flag, 0) !== 0) return false;
        return undefined;
      };
      const result = fp.merge(parts, { ...options, progress });
      return [result, Object.values(result.files).map((bytes) => bytes.buffer)];
    }
    case 'info':
      return [fp.info(...args), []];
    case 'render': {
      const result = fp.render(...args);
      return [result, [result.data.buffer]];
    }
    default:
      throw new fp.InvalidInputError(`unknown worker operation ${JSON.stringify(op)}`, { kind: 'InvalidInput' });
  }
}

function handle(message) {
  if (message?.type === 'init') {
    try {
      fp.initSync(message.module);
      send({ type: 'ready' });
    } catch (error) {
      send({ type: 'init-error', error: serializeError(error) });
    }
    return;
  }
  if (message?.type !== 'call') return;
  const { id, op, args, cancel, progress } = message;
  const flag = cancel ? new Int32Array(cancel) : null;
  try {
    if (flag && Atomics.load(flag, 0) !== 0) {
      throw new fp.CancelledError('cancelled before the operation started', { kind: 'Cancelled', exitCode: 1 });
    }
    const [result, transfer] = run(op, args, flag, progress, id);
    send({ type: 'result', id, result }, transfer);
  } catch (error) {
    // A trap (Rust panic, `unreachable`) leaves the instance unusable.
    const fatal = error instanceof WebAssembly.RuntimeError;
    send({ type: 'error', id, error: serializeError(error), fatal });
  }
}

if (parentPort) parentPort.on('message', handle);
else globalThis.addEventListener('message', (event) => handle(event.data));
