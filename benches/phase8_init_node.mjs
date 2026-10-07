import { readFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';
import path from 'node:path';
import { measure } from './phase8_init.mjs';
const [pkg, font] = process.argv.slice(2);
console.log(JSON.stringify(await measure(pathToFileURL(path.join(pkg, 'index.js')).href, readFileSync(path.join(pkg, 'fontpixel_wasm_bg.wasm')), readFileSync(font))));
