// Shim for bdfparser-js `src/constants.ts`: re-exports the fontpixel compatibility
// API (`fontpixel/compat/bdfparser`, resolved to the built package by
// run_reference.mjs's vitest config).
export { VERSION, PATTERN_VVECTOR_DELIMITER, EMPTY_GLYPH, META_TITLES, DIRE_SHORTCUT_MAP, DIRE_MAP } from 'fontpixel/compat/bdfparser';
