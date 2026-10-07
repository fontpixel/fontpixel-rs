// Shared data of the OpenType / WOFF / variants / merge suites
// (sfnt.test.mjs, woff.test.mjs, variants.test.mjs, merge.test.mjs).
//
// `crates/fontpixel-py/tests/data/p3/expected.json` holds the results of the
// same Rust calls (a throwaway Rust program on the fontpixel library with
// all-formats + serde + compat-bdfparser + cjk-charsets) and, under `cli.*`,
// outputs of the frozen phase-6 CLI; the Python binding tests use the same
// file, so both bindings are compared with the same native results. Fonts
// (`font.*`) are the Rust BDF reader's serde dicts of
// tests/data/compat/mini.bdf and data/p3/{alt,latin}.bdf.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { DATA, REPO, has } from './helpers.mjs';

export const P3 = path.join(REPO, 'crates', 'fontpixel-py', 'tests', 'data', 'p3');
export const E = JSON.parse(readFileSync(path.join(P3, 'expected.json'), 'utf8'));
export const GOLDEN = new Uint8Array(readFileSync(path.join(DATA, 'compat', 'mini-square-1.0-0.ttf')));
export const CIRCLE = new Uint8Array(readFileSync(path.join(DATA, 'compat', 'mini-circle-1.0-0.ttf')));

/** Bytes from hex text. */
export function fromHex(text) {
  return new Uint8Array(Buffer.from(text, 'hex'));
}

/** Hex text of bytes. */
export function toHex(bytes) {
  return Buffer.from(bytes).toString('hex');
}

/** Expected bytes stored as hex under `key`. */
export function h(key) {
  return fromHex(E[key]);
}

/** A serde font dict (`mini`, `alt` or `latin`), as a fresh copy. */
export function font(name) {
  return structuredClone(E[`font.${name}`]);
}

/**
 * JSON-normalize a binding result so it compares with the serde_json
 * expectation: Uint8Arrays become number arrays, Maps objects, bigints
 * numbers when exact.
 */
export function norm(value) {
  return JSON.parse(
    JSON.stringify(value, (_key, v) => {
      if (v instanceof Uint8Array) return Array.from(v);
      if (v instanceof Map) return Object.fromEntries(v);
      if (typeof v === 'bigint') return Number(v);
      return v;
    }),
  );
}

/** Whether two byte arrays are equal (assert with a short message). */
export function assertBytes(actual, expected, what = 'bytes') {
  assert.ok(actual instanceof Uint8Array, `${what}: expected a Uint8Array`);
  assert.equal(actual.length, expected.length, `${what}: length`);
  assert.ok(Buffer.from(actual).equals(Buffer.from(expected)), `${what}: content differs`);
}

/**
 * `assert.throws` for a fontpixel error class; `fields` are compared with the
 * error's own properties.
 */
export function throwsError(fn, Class, fields = {}) {
  assert.throws(fn, (e) => {
    assert.ok(e instanceof Class, `expected ${Class.name}, got ${e?.constructor?.name}: ${e?.message}`);
    for (const [k, v] of Object.entries(fields)) {
      if (v instanceof RegExp) assert.match(String(e[k]), v, k);
      else assert.deepEqual(e[k], v, k);
    }
    return true;
  });
}

/** `skip` option: a reason when the build lacks capability `name`. */
export function needs(fp, name) {
  return has(fp, name) ? false : `capability ${name} is compiled out of this build profile`;
}
