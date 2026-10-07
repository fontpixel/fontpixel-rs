// Executable recipes shared by Node and the browser harness. Full build required.
export async function recipes(fp, input, compat) {
  const held = [];
  const own = handle => { held.push(handle); return handle; };
  const results = [];
  const check = (name, condition) => { if (!condition) throw new Error(name); results.push(name); };
  try {
    // 1. Read, write and inspect the loss report before persisting a bundle.
    const loaded = fp.read(input, { name: 'font.bdf' });
    const fonts = own(loaded.fonts);
    const written = own(fp.write(fonts, 'pcf', { report: loaded.report }));
    const output = own(written.output);
    const pcf = output.intoSingle();
    check('read and write', pcf instanceof Uint8Array && Array.isArray(written.report.entries));

    // 2. A glyph is a live view; a bitmap property is an independent copy.
    const font = own(fonts.font(0));
    const glyph = own(font.glyph(font.glyphIds()[0]));
    const bitmap = own(glyph.bitmap);
    const old = bitmap.getPixel(0, 0);
    bitmap.setPixel(0, 0, 1 - old);
    check('copy before edit', own(glyph.bitmap).getPixel(0, 0) === old);
    glyph.bitmap = bitmap;
    check('edit a glyph view', own(glyph.bitmap).getPixel(0, 0) === 1 - old);

    // 3. Pixel operations remain in WASM; typed-array exports copy out.
    const pixels = own(fp.Bitmap.fromText('#.\n.#'));
    const raw = pixels.asBytes();
    raw[0] = 0;
    check('bitmap copy out', pixels.getPixel(0, 0) === 1);

    // 4. Native layout and PNG export with the complete rendering report.
    const rendered = own(font.draw('Hello'));
    const png = fp.toPng(own(rendered.bitmap));
    check('PNG render', png[0] === 137 && Array.isArray(rendered.report.entries));

    // 5. Exact wide integers use bigint; small integers return as number.
    check('BigInt', fp.normalize('model::PropertyValue', { Unsigned: 2n ** 63n }).Unsigned === 2n ** 63n);

    // 6. A caller may lower budgets; errors retain structured resource detail.
    let error;
    try { fp.read(input, { limits: { max_input_bytes: 8 } }); } catch (e) { error = e; }
    check('resource error', error instanceof fp.ResourceLimitError && error.limit === 8);

    // 7. Sidecars are explicit. An async resolver uses deterministic reruns.
    const hbf = own(fp.write(fonts, 'hbf'));
    const hbfBundle = own(hbf.output);
    const assets = hbfBundle.assets.map(own);
    const files = Object.fromEntries(assets.map(a => [a.path, a.data]));
    const result = await fp.withAsyncResources(
      resources => fp.read(files[hbfBundle.primary], { format: 'hbf', resources }),
      async name => files[name],
    );
    check('async sidecar', own(result.fonts).length === 1);

    // 8. OpenType and WOFF2 are in-process library operations.
    const vector = fp.variants.encodeSingle(font, { flavor: 'cff' });
    const woff2 = fp.woff.woff2.encode(vector.bytes);
    const sfnt = own(new fp.Sfnt(fp.woff.woff2.decode(woff2.bytes).sfnt));
    sfnt.validateChecksums();
    check('WOFF2', woff2.bytes.length > 0);

    // 9. Typed batch tasks use a flat `kind` discriminant.
    const batch = fp.ops.batch({ tasks: [{ id: 'pcf', kind: 'convert', inputs: ['font.bdf'], output: 'font.pcf' }] }, { files: { 'font.bdf': input } });
    check('typed batch', batch.exitCode === 0 && batch.outputs['font.pcf'].length > 0);

    // 10. Worker calls move CPU work off the caller's event loop.
    const worker = await fp.createWorker();
    try {
      const steps = [];
      const merged = await worker.merge([{ name: 'font.bdf', bytes: input, take: 'all' }],
        { family: 'Example', spacing: 'prop', outputs: ['example.bdf'] },
        { onProgress: event => steps.push(event.step) });
      check('Worker merge', merged.files['example.bdf'].length > 0 && steps.includes('encode'));
      const abort = new AbortController(); abort.abort();
      let cancelled;
      try { await worker.info(input, {}, { signal: abort.signal }); } catch (e) { cancelled = e; }
      check('Worker cancellation', cancelled instanceof fp.CancelledError);
    } finally { worker.terminate(); }

    // 11. The legacy API has its own lowercase names and data model.
    const legacy = new compat.Font().load_bytes(input);
    check('bdfparser compatibility', legacy.length === font.length && legacy.glyphbycp(65) !== null);

    // 12. Capability detection is explicit in every profile.
    check('capabilities', fp.capabilities().some(c => c.name === 'filesystem' && !c.available && c.alternative));
    return results;
  } finally {
    for (const handle of held.reverse()) handle.free();
  }
}
