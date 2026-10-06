// Plain-data codec between JS values and the JSON text the Rust side parses
// with serde (internal; imported by the wasm glue).
//
// Number policy (docs/bindings-design.md, "JS/WASM"):
// - a JS number that is an integer with |v| <= Number.MAX_SAFE_INTEGER is an
//   exact integer; a bigint is an exact integer of any size;
// - other finite numbers are floats: an integer-typed Rust field rejects them
//   (so a non-integral value, or an integral number beyond 2^53 whose exact
//   value is ambiguous, never reaches an integer field), a float field
//   accepts them; NaN and infinities are rejected;
// - results carry integers as `number` when |v| <= Number.MAX_SAFE_INTEGER,
//   else as `bigint`, decided per value.
// Maps: plain objects (string keys) or `Map`s whose keys are strings,
// integral numbers or bigints; integer keys of Rust maps are decimal strings.
// Bytes inside plain data: `Uint8Array`/`ArrayBuffer`/`DataView` (or arrays
// of numbers) on input; arrays of numbers on output.
import { InvalidInputError } from './errors.js';

const MARK = '\u0000fp:n';

function fail(path, message) {
  return new InvalidInputError(`${path || 'value'}: ${message}`, { kind: 'InvalidInput', exitCode: 1 });
}

function bytesOf(value) {
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (value instanceof Uint8Array || value instanceof Uint8ClampedArray) return value;
  if (value instanceof DataView) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  return null;
}

// Floats use the shortest round-trip digits with a `+`-less exponent
// (`1e21`, `1e-7`), the spelling serde_json's arbitrary-precision numbers
// treat as an exact float when a request buffers its fields.
function encodeNumber(value, path) {
  if (!Number.isFinite(value)) throw fail(path, `non-finite number ${value} is not plain data`);
  if (Number.isInteger(value)) {
    if (Math.abs(value) <= Number.MAX_SAFE_INTEGER) return Object.is(value, -0) ? '0' : String(value);
    // Integral but beyond 2^53, so not an exact integer: written as a float,
    // which integer fields reject (pass a bigint) and float fields accept.
    return value.toExponential().replace('e+', 'e');
  }
  return String(value).replace('e+', 'e');
}

function encodeKey(key, path) {
  if (typeof key === 'string') return key;
  if (typeof key === 'bigint') return key.toString();
  if (typeof key === 'number' && Number.isSafeInteger(key)) return String(key);
  throw fail(path, `map key ${String(key)} must be a string, an integer or a bigint`);
}

function encode(value, path, depth, out) {
  if (depth > 256) throw fail(path, 'nesting deeper than 256 levels');
  switch (typeof value) {
    case 'undefined':
      out.push('null');
      return;
    case 'boolean':
      out.push(value ? 'true' : 'false');
      return;
    case 'number':
      out.push(encodeNumber(value, path));
      return;
    case 'bigint':
      out.push(value.toString());
      return;
    case 'string':
      out.push(JSON.stringify(value));
      return;
    case 'object':
      break;
    default:
      throw fail(path, `a ${typeof value} is not plain data`);
  }
  if (value === null) {
    out.push('null');
    return;
  }
  const bytes = bytesOf(value);
  if (bytes) {
    out.push('[', bytes.join(','), ']');
    return;
  }
  if (Array.isArray(value)) {
    out.push('[');
    for (let i = 0; i < value.length; i++) {
      if (i) out.push(',');
      encode(value[i], `${path}[${i}]`, depth + 1, out);
    }
    out.push(']');
    return;
  }
  if (value instanceof Map) {
    out.push('{');
    let first = true;
    for (const [k, v] of value) {
      if (v === undefined) continue;
      const key = encodeKey(k, path);
      if (!first) out.push(',');
      first = false;
      out.push(JSON.stringify(key), ':');
      encode(v, `${path}.${key}`, depth + 1, out);
    }
    out.push('}');
    return;
  }
  const proto = Object.getPrototypeOf(value);
  if (proto !== Object.prototype && proto !== null) {
    const name = value?.constructor?.name ?? 'object';
    throw fail(path, `a ${name} is not plain data (expected an object, array, Map, string, number, bigint, boolean or null)`);
  }
  out.push('{');
  let first = true;
  for (const key of Object.keys(value)) {
    const v = value[key];
    if (v === undefined) continue;
    if (!first) out.push(',');
    first = false;
    out.push(JSON.stringify(key), ':');
    encode(v, path ? `${path}.${key}` : key, depth + 1, out);
  }
  out.push('}');
}

/** JSON text of a plain JS value with exact integers (internal). */
export function encodePlain(value) {
  const out = [];
  encode(value, '', 0, out);
  return out.join('');
}

function revive(_key, value) {
  return typeof value === 'string' && value.startsWith(MARK) ? BigInt(value.slice(MARK.length)) : value;
}

/**
 * Parse JSON text written by the Rust side (internal). With `markers`,
 * integers beyond 2^53 arrive as marked strings and become bigints.
 */
export function decodePlain(text, markers) {
  return markers ? JSON.parse(text, revive) : JSON.parse(text);
}
