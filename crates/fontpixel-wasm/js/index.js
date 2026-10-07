// Entry point of the `fontpixel` package (ESM; Node >= 18 and browsers,
// main thread or Web Worker).
//
// Call `await init()` once before anything else (or `initSync(...)`). The
// API is the wasm-bindgen output re-exported unchanged, plus the error
// classes and the asynchronous resource helper.
import initGlue, { initSync as initSyncGlue } from './fontpixel_wasm.js';
import { guardHandles } from './guard.js';
import { installModelProtocols } from './model.js';
import * as glue from './fontpixel_wasm.js';

export * from './fontpixel_wasm.js';
export {
  FontpixelError,
  UnknownFormatError,
  FeatureDisabledError,
  NotImplementedFormatError,
  AmbiguousFormatError,
  UnrecognizedFormatError,
  InvalidInputError,
  BdfParseError,
  ResourceLimitError,
  MissingResourceError,
  MultipleAssetsError,
  StrictLossError,
  ReportedErrorsError,
  FontpixelIoError,
  UnsupportedError,
  CancelledError,
} from './errors.js';
export { withAsyncResources } from './resources.js';
export { FontpixelWorker, compileModule, createWorker, mergeInWorker } from './ops-client.js';

/** URL of the WebAssembly module shipped next to this file. */
export const wasmUrl = new URL('./fontpixel_wasm_bg.wasm', import.meta.url);

guardHandles(glue);
installModelProtocols(glue);

let pending = null;
let initialized = false;

async function readFileUrl(url) {
  // Node: fetch() cannot read file: URLs. The specifier is hidden from
  // bundlers; browsers never take this branch (their URLs are http(s)).
  const fs = await import(/* webpackIgnore: true */ /* @vite-ignore */ 'node:fs/promises');
  return fs.readFile(url);
}

/**
 * Load and instantiate the WebAssembly module (idempotent).
 *
 * `source` defaults to the `.wasm` next to this module: fetched in browsers
 * and workers, read with node:fs in Node. It may also be a URL/string,
 * Response (or a promise of one), bytes, or a compiled WebAssembly.Module.
 */
export function init(source) {
  if (!pending) {
    pending = (async () => {
      let input = source;
      if (input === undefined) input = wasmUrl.protocol === 'file:' ? await readFileUrl(wasmUrl) : wasmUrl;
      await initGlue({ module_or_path: input });
      initialized = true;
    })();
    pending.catch(() => {
      pending = null;
    });
  }
  return pending;
}

/**
 * Instantiate synchronously from bytes or a compiled WebAssembly.Module
 * (idempotent). Browsers allow synchronous compilation of large modules only
 * off the main thread: on the main thread pass a module compiled with
 * `WebAssembly.compile`. Without an argument it reads the `.wasm` next to
 * this module where the runtime offers synchronous file access
 * (`process.getBuiltinModule`, Node >= 20.16 / 22.3).
 */
export function initSync(source) {
  if (initialized) return;
  let input = source;
  if (input === undefined) {
    const fs = globalThis.process?.getBuiltinModule?.('node:fs');
    if (!fs || wasmUrl.protocol !== 'file:') {
      throw new TypeError('initSync() needs the module bytes or a WebAssembly.Module here; use `await init()`');
    }
    input = fs.readFileSync(wasmUrl);
  }
  initSyncGlue({ module: input });
  initialized = true;
  pending ??= Promise.resolve();
}
