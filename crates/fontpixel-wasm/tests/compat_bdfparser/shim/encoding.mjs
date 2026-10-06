// Shim for bdfparser-js `src/encoding.ts`: re-exports the fontpixel compatibility
// API (`fontpixel/compat/bdfparser`, resolved to the built package by
// run_reference.mjs's vitest config).
export {
  sanitizeCharsetName,
  sanitizeCharset,
  canonicalCharset,
  charsetProps,
  char2cp,
  cp2char,
  getCodec,
  isUnicodeCharset,
  detectGL,
  charsetKeys,
  registerCharsetTables,
} from 'fontpixel/compat/bdfparser';
