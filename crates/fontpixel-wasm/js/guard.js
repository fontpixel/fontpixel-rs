// Released-handle guard (internal).
//
// After `free()` (or `[Symbol.dispose]()`) a wasm-bindgen object's pointer
// is 0 and the generated glue would throw a plain `Error("null pointer
// passed to rust")`. The guard wraps every method and accessor of the
// exported classes so that using a released handle, as `this` or as an
// argument, throws `InvalidInputError` naming the class instead, and makes
// `free()` / `[Symbol.dispose]()` idempotent (the glue would throw on a
// second call).
import { InvalidInputError } from './errors.js';

function released(name) {
  return new InvalidInputError(`this ${name} handle was released (free() was called); use copy() to keep a value`, {
    kind: 'InvalidInput',
  });
}

function isReleased(value) {
  return value !== null && typeof value === 'object' && value.__wbg_ptr === 0 && typeof value.free === 'function';
}

function wrap(fn, name) {
  return function guarded(...args) {
    if (isReleased(this)) throw released(name);
    for (const arg of args) {
      if (isReleased(arg)) throw released(arg.constructor?.name ?? 'object');
    }
    return fn.apply(this, args);
  };
}

function wrapStatic(fn) {
  return function guarded(...args) {
    for (const arg of args) {
      if (isReleased(arg)) throw released(arg.constructor?.name ?? 'object');
    }
    return fn.apply(this, args);
  };
}

/** Wrap the methods of every exported class in `exports` (once). */
export function guardHandles(exports) {
  for (const cls of Object.values(exports)) {
    if (typeof cls !== 'function' || typeof cls.prototype?.free !== 'function') continue;
    if (cls.__fontpixelGuarded) continue;
    Object.defineProperty(cls, '__fontpixelGuarded', { value: true });
    const proto = cls.prototype;
    const free = proto.free;
    const idempotentFree = function free_() {
      if (this.__wbg_ptr === 0) return;
      free.call(this);
    };
    Object.defineProperty(proto, 'free', { value: idempotentFree, writable: true, configurable: true });
    if (typeof Symbol.dispose === 'symbol') {
      Object.defineProperty(proto, Symbol.dispose, { value: idempotentFree, writable: true, configurable: true });
    }
    for (const key of Object.getOwnPropertyNames(proto)) {
      if (key === 'constructor' || key === 'free' || key.startsWith('__')) continue;
      const desc = Object.getOwnPropertyDescriptor(proto, key);
      if (typeof desc.value === 'function') desc.value = wrap(desc.value, cls.name);
      if (desc.get) desc.get = wrap(desc.get, cls.name);
      if (desc.set) desc.set = wrap(desc.set, cls.name);
      Object.defineProperty(proto, key, desc);
    }
    for (const key of Object.getOwnPropertyNames(cls)) {
      const desc = Object.getOwnPropertyDescriptor(cls, key);
      if (typeof desc.value !== 'function' || key.startsWith('__')) continue;
      desc.value = wrapStatic(desc.value);
      Object.defineProperty(cls, key, desc);
    }
  }
}
