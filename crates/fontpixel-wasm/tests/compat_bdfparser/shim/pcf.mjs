// Shim for bdfparser-js `src/pcf.ts`: re-exports the fontpixel compatibility
// API (`fontpixel/compat/bdfparser`, resolved to the built package by
// run_reference.mjs's vitest config).
export { readPcf, readPcfAsync, writePcf } from 'fontpixel/compat/bdfparser';
