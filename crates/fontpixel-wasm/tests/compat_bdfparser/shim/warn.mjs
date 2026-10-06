// Shim for bdfparser-js `src/warn.ts`: re-exports the fontpixel compatibility
// API (`fontpixel/compat/bdfparser`, resolved to the built package by
// run_reference.mjs's vitest config).
export { warn, setWarningHandler, defaultWarningHandler } from 'fontpixel/compat/bdfparser';
