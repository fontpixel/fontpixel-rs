// Shim for bdfparser-js `src/Glyph.ts`: re-exports the fontpixel compatibility
// API (`fontpixel/compat/bdfparser`, resolved to the built package by
// run_reference.mjs's vitest config).
export { Glyph, defaultglyphname } from 'fontpixel/compat/bdfparser';
