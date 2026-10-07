// Node-only convenience around the `ops` memory storage (import
// `fontpixel/node-files`; it uses node:fs and is not part of the browser
// entry point).
//
// `readFiles` reads named files into a `files` object for an `ops` call or
// `merge` part bytes; `writeOutputs` writes an operation's returned outputs
// (`result.outputs`, `merge(...).files`) to disk.
//
// This is NOT the native filesystem mode, which the WebAssembly package does
// not have (capability `filesystem`):
// - nothing is transactional: files are written one after another, and a
//   failure part-way leaves the earlier ones written;
// - there is no source-alias protection: an output may overwrite one of the
//   inputs (or another output of the same call) if the names say so;
// - sidecars and directory members are not discovered: list every file the
//   operation needs (`readDirectory` reads a whole directory).
// Use the native CLI or the Python binding's filesystem mode where those
// guarantees matter.
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { InvalidInputError } from './errors.js';

function relativeKey(name) {
  const key = String(name).split(/[\\/]+/).filter((part) => part !== '' && part !== '.');
  if (path.isAbsolute(String(name)) || key.includes('..') || key.length === 0) {
    throw new InvalidInputError(`output name ${JSON.stringify(name)} must be a relative path inside the output root`, {
      kind: 'InvalidInput',
    });
  }
  return key;
}

/**
 * Read `names` (paths relative to `root`, default the current directory) as
 * `{name: Uint8Array}`; the keys are the names exactly as given.
 */
export async function readFiles(names, { root = '.' } = {}) {
  const files = {};
  for (const name of names) {
    files[name] = new Uint8Array(await readFile(path.resolve(root, name)));
  }
  return files;
}

/**
 * Read every file below `directory` (relative to `root`) as
 * `{"<directory>/<relative path>": Uint8Array}` (`/` separators), for
 * directory inputs and sidecar sets.
 */
export async function readDirectory(directory, { root = '.' } = {}) {
  const files = {};
  const base = path.resolve(root, directory);
  const prefix = String(directory).split(path.sep).join('/').replace(/\/+$/, '');
  for (const entry of await readdir(base, { recursive: true, withFileTypes: true })) {
    if (!entry.isFile()) continue;
    const full = path.join(entry.parentPath ?? entry.path, entry.name);
    const relative = path.relative(base, full).split(path.sep).join('/');
    files[prefix ? `${prefix}/${relative}` : relative] = new Uint8Array(await readFile(full));
  }
  return files;
}

/**
 * Write `outputs` (`{storage key: bytes}`) below `root` (default the current
 * directory), creating parent directories, one file after another (not
 * transactional; no source protection). Keys must be relative and stay
 * inside `root`. `overwrite: false` refuses existing files. Returns the
 * absolute paths written, in order.
 */
export async function writeOutputs(outputs, { root = '.', overwrite = true } = {}) {
  const entries = outputs instanceof Map ? [...outputs] : Object.entries(outputs);
  const targets = entries.map(([name, bytes]) => [path.resolve(root, ...relativeKey(name)), bytes]);
  const written = [];
  for (const [target, bytes] of targets) {
    await mkdir(path.dirname(target), { recursive: true });
    await writeFile(target, bytes, { flag: overwrite ? 'w' : 'wx' });
    written.push(target);
  }
  return written;
}
