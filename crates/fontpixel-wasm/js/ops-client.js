// Web Worker adapter for the W8.3 calls (`merge`, `info`, `render`).
//
// A merge of 50,000+ glyphs takes long enough to freeze a page if it runs on
// the main thread, because WebAssembly calls are synchronous. `createWorker()`
// compiles the module once (or takes a compiled `WebAssembly.Module`), starts
// a worker (an ES module worker in browsers, `worker_threads` in Node) that
// instantiates it with `initSync(module)`, and returns a `FontpixelWorker`
// whose methods return Promises. Each worker has its own instance and
// memory: only plain data and bytes cross (input bytes are copied, output
// buffers are transferred back).
//
// Progress: `onProgress(progress)` receives every `Progress` record the merge
// reports (after each part is read, after the merge, after each output is
// encoded), posted from the worker.
//
// Cancellation: an `AbortSignal` cancels a call.
// - cooperative (default where `SharedArrayBuffer` is usable: Node, and
//   cross-origin isolated pages): the page sets a shared flag, the worker's
//   progress callback returns `false` at the next step boundary and the call
//   rejects with `CancelledError`; nothing is returned. `info`/`render` have
//   no step boundaries: they are cancelled only before they start (a result
//   that arrives after the abort is discarded and the call still rejects).
// - terminate (fallback where shared memory is unavailable, or
//   `cancellation: 'terminate'`): the worker is terminated at once, every
//   pending call rejects with `CancelledError`, and the next call starts a
//   fresh worker from the same compiled module.
// `worker.terminate()` is the hard stop in every mode; the worker cannot be
// used afterwards.
import * as errorClasses from './errors.js';

const { CancelledError, FontpixelError, InvalidInputError, UnsupportedError } = errorClasses;

const isNode = globalThis.process?.versions?.node !== undefined && typeof globalThis.Worker !== 'function';
const defaultWasmUrl = new URL('./fontpixel_wasm_bg.wasm', import.meta.url);
let defaultModule = null;

/**
 * Compile the WebAssembly module once (cached when `source` is omitted).
 * `source` may be bytes, a URL/string, a Response or a compiled module.
 */
export function compileModule(source) {
  if (source instanceof WebAssembly.Module) return Promise.resolve(source);
  if (source === undefined) {
    defaultModule ??= compileFrom(defaultWasmUrl);
    defaultModule.catch(() => {
      defaultModule = null;
    });
    return defaultModule;
  }
  return compileFrom(source);
}

async function compileFrom(source) {
  if (source instanceof URL || typeof source === 'string') {
    const url = new URL(source, import.meta.url);
    if (url.protocol === 'file:') {
      const fs = await import(/* webpackIgnore: true */ /* @vite-ignore */ 'node:fs/promises');
      return WebAssembly.compile(await fs.readFile(url));
    }
    source = await fetch(url);
  }
  if (typeof Response === 'function' && source instanceof Response) {
    if (typeof WebAssembly.compileStreaming === 'function') {
      try {
        return await WebAssembly.compileStreaming(source.clone());
      } catch {
        // Wrong MIME type: compile from the bytes instead.
      }
    }
    return WebAssembly.compile(await source.arrayBuffer());
  }
  return WebAssembly.compile(source);
}

function sharedMemoryUsable() {
  if (typeof SharedArrayBuffer !== 'function' || typeof Atomics !== 'object') return false;
  return isNode || globalThis.crossOriginIsolated === true;
}

function cancelled(message) {
  return new CancelledError(message, { kind: 'Cancelled', exitCode: 1 });
}

function rebuildError({ name, message, fields }) {
  const Class = errorClasses[name];
  if (typeof Class === 'function' && (Class === FontpixelError || Class.prototype instanceof FontpixelError)) {
    return new Class(message, fields);
  }
  const Builtin = globalThis[name];
  const builtin = typeof Builtin === 'function' && (Builtin === Error || Builtin.prototype instanceof Error);
  const error = builtin ? new Builtin(message) : new Error(message);
  if (!builtin) error.name = name;
  return Object.assign(error, fields);
}

async function spawn(workerUrl) {
  if (!isNode) {
    // Written inline so that bundlers recognize the worker entry point.
    const worker = workerUrl
      ? new Worker(workerUrl, { type: 'module' })
      : new Worker(new URL('./ops-worker.js', import.meta.url), { type: 'module' });
    return {
      post: (message, transfer) => worker.postMessage(message, transfer ?? []),
      listen(onMessage, onFailure) {
        worker.addEventListener('message', (event) => onMessage(event.data));
        worker.addEventListener('error', (event) => {
          event.preventDefault?.();
          onFailure(new Error(`fontpixel worker error: ${event.message ?? 'failed to load'}`));
        });
        worker.addEventListener('messageerror', () => onFailure(new Error('fontpixel worker: message could not be deserialized')));
      },
      terminate: () => worker.terminate(),
    };
  }
  const { Worker: NodeWorker } = await import(/* webpackIgnore: true */ /* @vite-ignore */ 'node:worker_threads');
  const worker = new NodeWorker(workerUrl ?? new URL('./ops-worker.js', import.meta.url));
  return {
    post: (message, transfer) => worker.postMessage(message, transfer ?? []),
    listen(onMessage, onFailure) {
      worker.on('message', onMessage);
      worker.on('error', onFailure);
      worker.on('exit', (code) => onFailure(new Error(`fontpixel worker exited (code ${code})`)));
    },
    terminate: () => worker.terminate(),
  };
}

/** A worker running fontpixel calls; create it with {@link createWorker}. */
export class FontpixelWorker {
  #module;
  #workerUrl;
  #cooperative;
  #port = null;
  #starting = null;
  #bootPort = null;
  #cancelStart = null;
  #pending = new Map();
  #nextId = 1;
  #closed = false;

  /** @internal Use `createWorker()`. */
  constructor(module, { workerUrl, cooperative }) {
    this.#module = module;
    this.#workerUrl = workerUrl;
    this.#cooperative = cooperative;
  }

  /** `"cooperative"` (shared cancellation flag) or `"terminate"`. */
  get cancellation() {
    return this.#cooperative ? 'cooperative' : 'terminate';
  }

  /** Number of calls sent and not yet settled. */
  get pending() {
    return this.#pending.size;
  }

  /** @internal Start (or restart) the worker and wait until it is ready. */
  start() {
    if (this.#closed) return Promise.reject(new InvalidInputError('this fontpixel worker was terminated', { kind: 'InvalidInput' }));
    if (this.#port) return Promise.resolve();
    this.#starting ??= (async () => {
      const port = await spawn(this.#workerUrl);
      if (this.#closed) {
        port.terminate();
        throw cancelled('the fontpixel worker was terminated during startup');
      }
      this.#bootPort = port;
      await new Promise((resolve, reject) => {
        this.#cancelStart = reject;
        let ready = false;
        port.listen(
          (message) => {
            if (!ready) {
              if (message?.type === 'ready') {
                ready = true;
                resolve();
              } else if (message?.type === 'init-error') {
                reject(rebuildError(message.error));
              }
              return;
            }
            this.#receive(port, message);
          },
          (error) => {
            if (!ready) reject(error);
            else if (this.#port === port) this.#stop(error);
          },
        );
        port.post({ type: 'init', module: this.#module });
      }).catch((error) => {
        port.terminate();
        throw error;
      });
      if (this.#closed) {
        port.terminate();
        throw cancelled('the fontpixel worker was terminated during startup');
      }
      this.#port = port;
    })().finally(() => {
      this.#starting = null;
      this.#bootPort = null;
      this.#cancelStart = null;
    });
    return this.#starting;
  }

  #receive(port, message) {
    if (port !== this.#port) return;
    const entry = this.#pending.get(message?.id);
    if (!entry) return;
    if (message.type === 'progress') {
      if (!entry.hasProgressError && entry.onProgress) {
        try {
          entry.onProgress(message.progress);
        } catch (error) {
          // Like a throwing progress callback: cancel and rethrow it.
          entry.hasProgressError = true;
          entry.progressError = error;
          this.#cancel(entry);
        }
      }
      return;
    }
    this.#pending.delete(message.id);
    entry.cleanup();
    if (message.type === 'error' && message.fatal) {
      // The instance trapped: discard this worker; the next call starts a new one.
      this.#stop(rebuildError(message.error));
    }
    if (entry.hasProgressError) entry.reject(entry.progressError);
    else if (entry.signal?.aborted) entry.reject(cancelled('the operation was aborted'));
    else if (message.type === 'error') entry.reject(rebuildError(message.error));
    else entry.resolve(message.result);
  }

  #cancel(entry) {
    if (entry.flag) Atomics.store(entry.flag, 0, 1);
    else this.#stop(cancelled('the operation was aborted; the worker was terminated'));
  }

  /** Terminate the running worker and reject every pending call with `error`. */
  #stop(error) {
    const port = this.#port;
    this.#port = null;
    port?.terminate();
    this.#bootPort?.terminate();
    this.#cancelStart?.(error);
    for (const [id, entry] of this.#pending) {
      this.#pending.delete(id);
      entry.cleanup();
      entry.reject(entry.hasProgressError ? entry.progressError : error);
    }
  }

  async #call(op, args, { onProgress, signal } = {}) {
    if (signal?.aborted) throw cancelled('the operation was aborted before it started');
    await this.start();
    // terminate() or a terminating abort may have stopped the worker meanwhile.
    while (!this.#port) {
      if (this.#closed) throw cancelled('the fontpixel worker was terminated');
      await this.start();
    }
    if (signal?.aborted) throw cancelled('the operation was aborted before it started');
    const id = this.#nextId++;
    const cancel = this.#cooperative ? new SharedArrayBuffer(4) : null;
    return new Promise((resolve, reject) => {
      const entry = {
        resolve,
        reject,
        onProgress,
        signal,
        flag: cancel ? new Int32Array(cancel) : null,
        hasProgressError: false,
        progressError: undefined,
        cleanup: () => {},
      };
      if (signal) {
        const onAbort = () => this.#cancel(entry);
        signal.addEventListener('abort', onAbort, { once: true });
        entry.cleanup = () => signal.removeEventListener('abort', onAbort);
      }
      this.#pending.set(id, entry);
      try {
        this.#port.post({ type: 'call', id, op, args, cancel, progress: typeof onProgress === 'function' });
      } catch (error) {
        this.#pending.delete(id);
        entry.cleanup();
        reject(error);
      }
    });
  }

  /**
   * `merge(parts, options)` in the worker; `options` as for the
   * synchronous `merge` except `progress` (use `onProgress`).
   */
  merge(parts, options = {}, { onProgress, signal } = {}) {
    const { progress, ...plain } = options ?? {};
    if (progress !== undefined) {
      return Promise.reject(
        new InvalidInputError('options.progress cannot cross into a worker; pass {onProgress} as the third argument', {
          kind: 'InvalidInput',
        }),
      );
    }
    return this.#call('merge', [parts, plain], { onProgress, signal });
  }

  /** `info(bytes, options)` in the worker. */
  info(bytes, options, { signal } = {}) {
    return this.#call('info', options === undefined ? [bytes] : [bytes, options], { signal });
  }

  /** `render(fontBytes, text, options)` in the worker. */
  render(fontBytes, text, options, { signal } = {}) {
    return this.#call('render', options === undefined ? [fontBytes, text] : [fontBytes, text, options], { signal });
  }

  /** Hard stop: terminate the worker; pending calls reject with `CancelledError`. */
  terminate() {
    this.#closed = true;
    this.#stop(cancelled('the fontpixel worker was terminated'));
  }
}

/**
 * Start a worker. Options: `module` (a compiled `WebAssembly.Module`;
 * default: the package's `.wasm`, compiled once and cached), `workerUrl`
 * (override the worker script), `cancellation` (`"auto"` (default),
 * `"cooperative"` (needs `SharedArrayBuffer`) or `"terminate"`).
 */
export async function createWorker(options = {}) {
  const { module, workerUrl, cancellation = 'auto' } = options;
  if (!['auto', 'cooperative', 'terminate'].includes(cancellation)) {
    throw new InvalidInputError(`cancellation must be "auto", "cooperative" or "terminate", not ${JSON.stringify(cancellation)}`, {
      kind: 'InvalidInput',
    });
  }
  const usable = sharedMemoryUsable();
  if (cancellation === 'cooperative' && !usable) {
    throw new UnsupportedError(
      'cooperative worker cancellation needs SharedArrayBuffer, which this page cannot use (it is not cross-origin isolated)',
      {
        kind: 'Unsupported',
        capability: 'shared_memory',
        reason: 'SharedArrayBuffer is only available to cross-origin isolated pages.',
        evidence: 'globalThis.crossOriginIsolated is not true or SharedArrayBuffer is undefined.',
        alternative: 'Serve the page with Cross-Origin-Opener-Policy: same-origin and Cross-Origin-Embedder-Policy: require-corp, or use cancellation "terminate".',
      },
    );
  }
  const compiled = await compileModule(module);
  const worker = new FontpixelWorker(compiled, {
    workerUrl,
    cooperative: cancellation === 'cooperative' || (cancellation === 'auto' && usable),
  });
  await worker.start();
  return worker;
}

/**
 * Run one `merge(parts, options)` in a worker and resolve to its result
 * (`{files, report, json, text}`). Pass `worker` to reuse one; otherwise a
 * worker is created for this call and terminated afterwards.
 */
export async function mergeInWorker(parts, options, { onProgress, signal, worker, module } = {}) {
  const own = worker ?? (await createWorker({ module }));
  try {
    return await own.merge(parts, options, { onProgress, signal });
  } finally {
    if (!worker) own.terminate();
  }
}
