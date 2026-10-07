// Shim for bdfparser-js `src/cli.ts`: re-exports the fontpixel compatibility
// API (`fontpixel/compat/bdfparser`, resolved to the built package by
// run_reference.mjs's vitest config).
export { loadFontFile, main } from 'fontpixel/compat/bdfparser/cli';
