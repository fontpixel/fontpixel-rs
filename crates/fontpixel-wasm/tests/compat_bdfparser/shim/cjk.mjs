// Shim for bdfparser-js `src/cjk.ts`: re-exports the fontpixel compatibility
// API (`fontpixel/compat/bdfparser`, resolved to the built package by
// run_reference.mjs's vitest config).
export { cjkTables } from 'fontpixel/compat/bdfparser/cjk';
