// Shim for bdfparser-js `src/errors.ts`: re-exports the fontpixel compatibility
// API (`fontpixel/compat/bdfparser`, resolved to the built package by
// run_reference.mjs's vitest config).
export { BDFParseError } from 'fontpixel/compat/bdfparser';
