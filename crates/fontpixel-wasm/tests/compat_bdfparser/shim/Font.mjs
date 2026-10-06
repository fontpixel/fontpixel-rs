// Shim for bdfparser-js `src/Font.ts`: re-exports the fontpixel compatibility
// API (`fontpixel/compat/bdfparser`, resolved to the built package by
// run_reference.mjs's vitest config).
// `gunzip` is bdfparser-js's internal DecompressionStream helper (no
// fontpixel counterpart: the Rust loader decodes gzip itself); it is ported
// here so the test module resolves, see KNOWN_DIFFERENCES.md.
export { Font } from 'fontpixel/compat/bdfparser';

export const gunzip = async (data) => {
  if (typeof DecompressionStream === 'undefined') {
    throw new Error('Cannot decompress gzip data: DecompressionStream is not available');
  }
  const ds = new DecompressionStream('gzip');
  const writer = ds.writable.getWriter();
  const written = writer.write(data).then(() => writer.close());
  written.catch(() => undefined);
  const reader = ds.readable.getReader();
  const chunks = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    total += value.length;
  }
  await written;
  const out = new Uint8Array(total);
  let o = 0;
  for (const c of chunks) {
    out.set(c, o);
    o += c.length;
  }
  return out;
};
