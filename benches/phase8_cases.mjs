// Identical workloads in Node and Chromium. Setup and verification are untimed.
export async function benchmark(engine, api, inputs, gc = () => {}) {
  const native = engine === 'native';
  const decode = new TextDecoder();
  const scText = decode.decode(inputs.sc);
  const cps = [...scText.matchAll(/^ENCODING (\d+)$/gm)].map(m => +m[1]);
  const mixed = String.fromCodePoint(...cps.slice(0, 2000));
  const held = [];
  const own = x => { held.push(x); return x; };
  const parse = bytes => native ? api.read(bytes).fonts : new api.Font().load_bytes(bytes);
  const scSet = own(parse(inputs.sc)), latinSet = own(parse(inputs.latin));
  const sc = native ? own(scSet.font(0)) : scSet;
  const latin = native ? own(latinSet.font(0)) : latinSet;
  const rows = ['10000001', '01000010', '00100100', '00011000', '00011000', '00100100', '01000010', '10000001'];
  const bm = own(native ? api.Bitmap.fromText(rows.join('\n'), { palette: { 0: 0, 1: 1 } }) : new api.Bitmap(rows));
  const nativeRows = value => value.toText({ 0: '0', 1: '1' });
  const raster = value => {
    if (!native) return value.bindata.join('\n');
    if (value instanceof api.Rendered) { const b = value.bitmap; try { return nativeRows(b); } finally { b.free(); } }
    return nativeRows(value);
  };
  const count = value => native ? (() => { const f = value.font(0); try { return f.length; } finally { f.free(); } })() : value.length;
  const dispose = value => { if (native && value?.free) value.free(); };
  const canonical = x => Array.isArray(x) ? x.map(canonical) : x && typeof x === 'object' ? Object.fromEntries(Object.keys(x).sort().map(k => [k, canonical(x[k])])) : x;
  const hash = async value => {
    const bytes = value instanceof Uint8Array ? value : new TextEncoder().encode(JSON.stringify(canonical(value)));
    return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), b => b.toString(16).padStart(2, '0')).join('');
  };
  const cases = [];
  const add = (id, category, n, fn, inspect = x => x, compare = true) => cases.push({ id, category, iterations: n, fn, inspect, compare });
  add('parse_sc', 'whole', 1, () => parse(inputs.sc), count);
  add('parse_latin', 'whole', 4, () => parse(inputs.latin), count);
  for (const [id, format, font, set] of [['write_bdf_sc', 'bdf', sc, scSet], ['write_pcf_sc', 'pcf', sc, scSet], ['write_ttf_latin', 'ttf', latin, latinSet]]) {
    const write = () => {
      if (!native) return format === 'bdf' ? font.tobdf() : format === 'pcf' ? font.topcf() : font.tottf({ timestamp: 0 });
      const written = api.write(set, format);
      const output = written.output;
      try { return output.intoSingle(); } finally { output.free(); written.free(); }
    };
    // Native serialization has a different metadata/layout contract. Keep its
    // byte digest as evidence, but never label it identical to the legacy writer.
    add(id, 'whole', 1, write, x => typeof x === 'string' ? new TextEncoder().encode(x) : x, !native);
  }
  add('draw_2000_repeat', 'whole', 4, () => native ? sc.draw('永'.repeat(2000), { mode: 'cell', line_limit: 80 * sc.metrics.bounds.width }) : sc.draw('永'.repeat(2000), { mode: 0, linelimit: 80 * sc.headers.fbbx }), raster);
  add('draw_2000_mixed', 'whole', 4, () => native ? sc.draw(mixed, { mode: 'advance', line_limit: 512 }) : sc.draw(mixed, { mode: 1, linelimit: 512 }), raster);
  // Latin draw-all stays within every engine's unchanged default pixel limits.
  add('drawall_latin', 'whole', 2, () => native ? latin.drawAll(undefined, undefined, { mode: 'advance', line_limit: 512 }) : latin.drawall({ mode: 1, linelimit: 512 }), raster);
  add('lookup_A', 'small', 2000, () => native ? latin.glyphForCodepoint(65) : latin.glyphbycp(65), x => native ? x.codepoints[0].Unicode : x.meta.codepoint);
  add('font_length', 'small', 20000, () => sc.length);
  add('bitmap_getpixel', 'small', 20000, () => native ? bm.getPixel(0, 0) : bm.getpixel(0, 0));
  add('bitmap_clone', 'small', 2000, () => native ? bm.copy() : bm.clone(), raster);
  if (!native) {
    add('glyph_metadata', 'small', 500, () => latin.glyphbycp(65).meta);
    add('font_headers', 'small', 1000, () => ({ ...latin.headers }));
    add('font_property', 'small', 2000, () => latin.getprop('family_name'));
    add('iterate_codepoints_sc', 'whole', 2, () => sc.itercps());
    add('iterate_glyphs_latin', 'whole', 2, () => [...latin.iterglyphs()].map(g => g.meta.codepoint));
    add('glyph_draw', 'small', 500, () => latin.glyphbycp(65).draw(), raster);
    add('bitmap_construct', 'small', 2000, () => new api.Bitmap(rows), raster);
    add('bitmap_crop', 'small', 1000, () => bm.clone().crop(4, 4, 1, 1), raster);
    add('bitmap_concat', 'small', 1000, () => bm.clone().concat(bm), raster);
    add('bitmap_shift', 'small', 1000, () => bm.clone().shift(1, -1), raster);
    add('bitmap_glow', 'small', 1000, () => bm.clone().glow(), raster);
    for (let mode = 0; mode <= 5; mode++) add(`bitmap_todata_${mode}`, 'small', 1000, () => bm.todata(mode));
  }
  add('js_noop_baseline', 'baseline', 20000, () => 1);
  const results = [];
  try {
    for (const c of cases) {
      let value = c.fn();
      const signature = await hash(c.inspect(value));
      dispose(value); value = undefined;
      // One whole-workload warmup, 20 small-call warmups; same policy for all.
      for (let i = 0; i < (c.category === 'whole' ? 1 : 20); i++) dispose(c.fn());
      await gc();
      let sink = 0;
      const start = performance.now();
      for (let i = 0; i < c.iterations; i++) {
        value = c.fn();
        sink = ((sink * 33) ^ (typeof value === 'number' ? value : value ? 1 : 0)) | 0;
        dispose(value);
      }
      const ms = (performance.now() - start) / c.iterations;
      globalThis.__fontpixelBenchSink = sink;
      value = undefined;
      results.push({ id: c.id, category: c.category, iterations: c.iterations, ms, signature, compare: c.compare, sink });
    }
    return results;
  } finally { for (const value of held.reverse()) dispose(value); }
}
