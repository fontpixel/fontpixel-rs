// Shim for bdfparser-js `src/Bitmap.ts`: re-exports the fontpixel compatibility
// API (`fontpixel/compat/bdfparser`, resolved to the built package by
// run_reference.mjs's vitest config).
export { Bitmap, fmtnum } from 'fontpixel/compat/bdfparser';
