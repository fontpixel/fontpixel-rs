import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import path from 'node:path';
import { benchmark } from './phase8_cases.mjs';
const [engine, pkg, reference, sc, latin] = process.argv.slice(2);
let api;
if (engine === 'pure') api = await import(pathToFileURL(reference));
else {
  api = await import(pathToFileURL(path.join(pkg, 'index.js')));
  await api.init();
  if (engine === 'compat') api = await import(pathToFileURL(path.join(pkg, 'compat_bdfparser.js')));
}
const inputs = { sc: new Uint8Array(readFileSync(sc)), latin: new Uint8Array(readFileSync(latin)) };
console.log(JSON.stringify(await benchmark(engine, api, inputs, globalThis.gc)));
