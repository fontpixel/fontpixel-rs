// Foundation measurements: module start-up, the plain-data bridge on a large
// font, and the released-handle guard's per-call cost.
//
// Usage: node crates/fontpixel-wasm/bench/foundation.mjs [FONT.bdf]
// (FONTPIXEL_WASM_PKG as for the tests; default font: Tong Pixel Sans SC from
// the repository fixtures).
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { performance } from 'node:perf_hooks';
import { pathToFileURL } from 'node:url';
import { PKG, buildInfo, fixture } from '../tests/helpers.mjs';

const fontPath = process.argv[2] ?? 'fixtures/tong-pixel-sans/TongPixelSans14Large-SC.bdf.gz';
const bytes = process.argv[2]
  ? new Uint8Array(readFileSync(process.argv[2]))
  : fixture('tong-pixel-sans/TongPixelSans14Large-SC.bdf.gz');
const wasmBytes = readFileSync(path.join(PKG, 'fontpixel_wasm_bg.wasm'));

function time(label, fn, runs = 5) {
  let best = Infinity;
  let result;
  for (let i = 0; i < runs; i++) {
    const t = performance.now();
    result = fn();
    best = Math.min(best, performance.now() - t);
  }
  console.log(`${label.padEnd(44)} ${best.toFixed(2).padStart(10)} ms`);
  return result;
}

const t0 = performance.now();
const module = await WebAssembly.compile(wasmBytes);
const t1 = performance.now();
const fp = await import(pathToFileURL(path.join(PKG, 'index.js')).href);
fp.initSync(module);
const t2 = performance.now();
const info = buildInfo();
console.log(`profile ${info.profile}: ${info.wasm_bytes} bytes (${info.wasm_gzip_bytes} gzip)`);
console.log(`${'compile (WebAssembly.compile)'.padEnd(44)} ${(t1 - t0).toFixed(2).padStart(10)} ms`);
console.log(`${'instantiate + start (initSync)'.padEnd(44)} ${(t2 - t1).toFixed(2).padStart(10)} ms`);

console.log(`font: ${fontPath} (${bytes.length} bytes)`);
const loaded = time('read', () => fp.read(bytes), 3);
const set = loaded.fonts;
const glyphs = set.font(0).length;
const dict = time('FontSet.toDict (plain data out)', () => set.toDict(), 3);
const json = JSON.stringify(dict, (_k, v) => (typeof v === 'bigint' ? v.toString() : v));
console.log(`  ${glyphs} glyphs, plain data ${json.length} JSON chars`);
time('JSON.parse of the same text (baseline)', () => JSON.parse(json), 3);
time('FontSet.fromDict (plain data in)', () => fp.FontSet.fromDict(dict), 3);
time('FontSet.copy (no plain data)', () => set.copy(), 3);

// Guard cost: the guarded package classes vs. a second, unguarded instance
// of the glue (a separate module instance via a distinct URL).
const raw = await import(`${pathToFileURL(path.join(PKG, 'fontpixel_wasm.js')).href}?unguarded`);
raw.initSync({ module });
const guarded = fp.Bitmap.fromText('#.\n.#');
const plain = raw.Bitmap.fromText('#.\n.#');
const N = 1_000_000;
const loop = (b) => () => {
  let s = 0;
  for (let i = 0; i < N; i++) s += b.getPixel(i & 1, 0);
  return s;
};
const g = time(`getPixel x ${N} (guarded)`, loop(guarded));
const u = time(`getPixel x ${N} (unguarded glue)`, loop(plain));
if (g !== u) throw new Error('mismatch');
