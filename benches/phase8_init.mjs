// Preloaded-byte initialization measurement, with API import measured separately.
export async function measure(url, bytes, fontBytes) {
  const t0 = performance.now();
  const fp = await import(url);
  const t1 = performance.now();
  const module = await WebAssembly.compile(bytes);
  const t2 = performance.now();
  fp.initSync(module);
  const t3 = performance.now();
  const catalogue = fp.formats.catalogue();
  const enabled = Object.fromEntries(['bdf', 'pcf', 'yaff', 'ttf', 'otf', 'woff', 'woff2', 'vector-raster'].map(id => [id, catalogue.find(f => f.id === id).feature_enabled]));
  const set = fp.read(fontBytes).fonts;
  const outputs = {};
  try {
    for (const id of Object.keys(enabled).filter(id => enabled[id] && id !== 'vector-raster')) {
      const w = fp.write(set, id);
      const out = w.output;
      try { outputs[id] = out.intoSingle().length; } finally { out.free(); w.free(); }
    }
  } finally { set.free(); }
  return { importMs: t1 - t0, compileMs: t2 - t1, instantiateMs: t3 - t2, totalMs: t3 - t0, enabled, cjk: fp.capabilities().find(c => c.name === 'cjk_charsets').available, specializedOpenType: typeof fp.Sfnt === 'function', outputs };
}
