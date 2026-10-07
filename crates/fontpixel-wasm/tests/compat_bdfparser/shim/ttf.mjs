// Shim for bdfparser-js `src/ttf.ts`: re-exports the fontpixel compatibility
// API (`fontpixel/compat/bdfparser`, resolved to the built package by
// run_reference.mjs's vitest config).
export { readTtf, writeTtf } from 'fontpixel/compat/bdfparser';
