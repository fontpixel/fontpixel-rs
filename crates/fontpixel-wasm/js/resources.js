// Asynchronous resource acquisition for synchronous fontpixel calls.
//
// fontpixel calls are synchronous and deterministic: given the same inputs
// and the same available files they produce the same result. An async source
// (fetch, node:fs/promises, IndexedDB, a user picking files) therefore works
// by re-running: run the call with a recording resolver over the files
// fetched so far; collect every name it asked for and did not have; await
// those; run again. The loop ends when a run asks for nothing new, so the
// result equals a synchronous call that had every file from the start. No
// I/O happens inside the synchronous call.
import { InvalidInputError, ResourceLimitError } from './errors.js';

function toBytes(value, name) {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  throw new InvalidInputError(`resolver result for ${JSON.stringify(name)}: expected a Uint8Array, ArrayBuffer or DataView`, {
    kind: 'InvalidInput',
  });
}

/**
 * Run `run(resources)` until it no longer asks for unknown files.
 *
 * `resolve(path)` is async and returns the file's bytes, or `null`/`undefined`
 * when it does not exist. `run` receives a synchronous resolver object to pass
 * as the `resources` argument of any fontpixel call; it must be a pure
 * function of what it reads (no side effects outside its return value),
 * since it may run several times.
 *
 * Options: `files` preloads a Map/object of known files; `list(directory)`
 * (async, returns names) enables directory enumeration; `maxRounds`
 * (default 1024) bounds the number of runs.
 */
export async function withAsyncResources(run, resolve, options = {}) {
  const { files, list, maxRounds = 1024 } = options;
  const known = new Map(files instanceof Map ? files : Object.entries(files ?? {}));
  for (const [name, bytes] of known) known.set(name, toBytes(bytes, name));
  const absent = new Set();
  const listings = new Map();
  for (let round = 1; ; round++) {
    const wanted = new Set();
    const wantedDirs = new Set();
    const resolver = {
      resolve(path) {
        if (known.has(path)) return known.get(path);
        if (!absent.has(path)) wanted.add(path);
        return undefined;
      },
    };
    if (list) {
      resolver.list = (directory) => {
        const key = directory ?? '';
        if (listings.has(key)) return listings.get(key);
        wantedDirs.add(key);
        return [];
      };
    }
    let result;
    let error;
    let failed = false;
    try {
      result = run(resolver);
    } catch (e) {
      error = e;
      failed = true;
    }
    if (wanted.size === 0 && wantedDirs.size === 0) {
      if (failed) throw error;
      return result;
    }
    if (round >= maxRounds) {
      throw new ResourceLimitError(`withAsyncResources: still missing resources after ${round} runs`, {
        kind: 'ResourceLimit',
        resource: 'async resource rounds',
        limit: maxRounds,
      });
    }
    await Promise.all([
      ...[...wanted].map(async (path) => {
        const bytes = await resolve(path);
        if (bytes == null) absent.add(path);
        else known.set(path, toBytes(bytes, path));
      }),
      ...[...wantedDirs].map(async (dir) => {
        listings.set(dir, [...(await list(dir === '' ? null : dir))]);
      }),
    ]);
  }
}
