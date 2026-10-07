// Model, metadata and resource helpers (internal; imported by the wasm glue
// and by index.js).
//
// `stashHandle(value)` lets Rust read a handle that arrives inside a plain
// `JsValue` (unions such as `Glyph | GlyphDict`, arrays of fonts) without
// taking ownership: it checks `value` against the exported classes and calls
// the class's hidden `__fpStash()` method, which borrows the receiver and
// leaves a reference to its storage for the Rust caller
// (src/model_args.rs). `installModelProtocols` adds the JS iteration
// protocol to the list-like classes.
import * as glue from './fontpixel_wasm.js';
import { InvalidInputError } from './errors.js';

const STASHABLE = [
  'FontSet',
  'Font',
  'Glyph',
  'Bitmap',
  'Codepoint',
  'Properties',
  'ResourceSet',
  'Input',
  'OutputAsset',
  'OutputBundle',
  'Written',
  'BitmapProjection',
];

/**
 * Called from Rust: `true` after `value.__fpStash()` ran for an exported
 * class instance, `false` for anything else. Throws `InvalidInputError` for
 * a released handle.
 */
export function stashHandle(value) {
  for (const name of STASHABLE) {
    const cls = glue[name];
    if (typeof cls === 'function' && value instanceof cls) {
      if (value.__wbg_ptr === 0) {
        throw new InvalidInputError(`this ${name} handle was released (free() was called); use copy() to keep a value`, {
          kind: 'InvalidInput',
        });
      }
      cls.prototype.__fpStash.call(value);
      return true;
    }
  }
  return false;
}

/** Add `[Symbol.iterator]` to the list-like classes of `exports` (once). */
export function installModelProtocols(exports) {
  const iterate = {
    // Live views of every strike.
    FontSet() {
      return this.fonts[Symbol.iterator]();
    },
    // Live views of every glyph, in ID order.
    Font() {
      return this.glyphs()
        .map(([, glyph]) => glyph)
        [Symbol.iterator]();
    },
    // Property records (plain data).
    Properties() {
      return this.toList()[Symbol.iterator]();
    },
    // Copies of every asset, sorted by path.
    OutputBundle() {
      return this.assets[Symbol.iterator]();
    },
  };
  for (const [name, fn] of Object.entries(iterate)) {
    const cls = exports[name];
    if (typeof cls !== 'function' || Object.prototype.hasOwnProperty.call(cls.prototype, Symbol.iterator)) continue;
    Object.defineProperty(cls.prototype, Symbol.iterator, { value: fn, writable: true, configurable: true });
  }
}
