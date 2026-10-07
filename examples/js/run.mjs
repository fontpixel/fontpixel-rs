// Build full first. No package installation in this repository is necessary.
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { recipes } from './recipes.mjs';
const root = fileURLToPath(new URL('../..', import.meta.url));
const pkg = path.resolve(process.env.FONTPIXEL_WASM_PKG ?? path.join(process.env.CARGO_TARGET_DIR ?? path.join(root, 'target'), 'wasm-pkg/full'));
const fp = await import(pathToFileURL(path.join(pkg, 'index.js')));
await fp.init();
const compat = await import(pathToFileURL(path.join(pkg, 'compat_bdfparser.js')));
const bytes = new Uint8Array(await readFile(path.join(root, 'tests/data/bdf/spleen-5x8.bdf')));
console.log(await recipes(fp, bytes, compat));
