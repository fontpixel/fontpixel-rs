// Iteration protocol of the list-like classes (added by js/model.js).
import type { Font, Glyph, OutputAsset, Property } from "./fontpixel_wasm.js";

declare module "./fontpixel_wasm.js" {
  interface FontSet {
    /** Live views of every strike. */
    [Symbol.iterator](): IterableIterator<Font>;
  }
  interface Font {
    /** Live views of every glyph, in ID order. */
    [Symbol.iterator](): IterableIterator<Glyph>;
  }
  interface Properties {
    /** The property records (plain data). */
    [Symbol.iterator](): IterableIterator<Property>;
  }
  interface OutputBundle {
    /** Copies of every asset, sorted by path. */
    [Symbol.iterator](): IterableIterator<OutputAsset>;
  }
}

export {};
