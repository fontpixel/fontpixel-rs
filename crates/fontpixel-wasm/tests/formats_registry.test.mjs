// Registry, module functions, errors, strict mode, limits, resources and JS
// codecs (a port of crates/fontpixel-py/tests/test_formats_registry.py),
// compared with the golden CLI hashes and live CLI runs.
import assert from 'node:assert/strict';
import test from 'node:test';
import { gunzipSync, gzipSync } from 'node:zlib';
import { HAS_CLI, cliFiles, load } from './helpers.mjs';
import { GOLDEN, PROFILE, assetsOf, caseOf, enabled, hashes, sha, sourceFile } from './formats_support.mjs';

const fp = await load();
const UNI3 = sourceFile('uni3');
const UNI5 = sourceFile('uni5');
const LEG3 = sourceFile('leg3');
const HAS_YAFF = enabled(fp, 'yaff');

/** Skip reason when any of `ids` is compiled out of this build. */
function needs(...ids) {
  const off = ids.filter((f) => !enabled(fp, f));
  return off.length ? `${off.join(', ')} compiled out of the ${PROFILE} build` : false;
}

function single(written) {
  return written.output.intoSingle();
}

/** A deterministic serialization of a loaded model (YAFF when compiled, else BDF). */
function model(loaded) {
  return single(fp.write(loaded.fonts, HAS_YAFF ? 'yaff' : 'bdf', { report: loaded.report }));
}

// --- catalogue queries ------------------------------------------------------

test('registry formats status and names', () => {
  const reg = new fp.Registry();
  assert.deepEqual(reg.formats(), fp.listFormats());
  if (PROFILE === 'full') assert.deepEqual(reg.formats(), GOLDEN.listing);
  assert.deepEqual(reg.status('bdf'), GOLDEN.listing[0]);
  assert.throws(
    () => reg.status('nope'),
    (e) => e instanceof fp.UnknownFormatError && e.kind === 'UnknownFormat' && e.format === 'nope',
  );
  // Longest suffix wins; collisions stay visible (the catalogue is complete in every build).
  assert.deepEqual(reg.extensionCandidates('x.bdf.gz').map((f) => f.id), ['bdf']);
  const fnt = reg.extensionCandidates('X.FNT').map((f) => f.id);
  for (const id of ['raw', 'fnt', 'fontx2']) assert.ok(fnt.includes(id), id);
  assert.equal(reg.formatFromName('a.pcf').id, 'pcf');
  assert.throws(
    () => reg.formatFromName('a.fnt'),
    (e) => e instanceof fp.AmbiguousFormatError && new Set(e.candidates).size === new Set(fnt).size,
  );
  assert.throws(() => reg.formatFromName('a.unknown-ext'), fp.UnrecognizedFormatError);
  assert.match(String(reg), /^Registry\(99 formats, \d+ with codecs\)$/);
  if (PROFILE === 'full') assert.equal(String(reg), 'Registry(99 formats, 99 with codecs)');
  assert.ok(fp.registry() instanceof fp.Registry);
  assert.throws(() => new fp.Registry({ bulitin: false }), /unknown option `bulitin`/);
});

test('planned registry has no codecs', () => {
  const reg = new fp.Registry({ builtin: false });
  const statuses = reg.formats();
  assert.equal(statuses.length, 99);
  assert.ok(!statuses.some((s) => s.read || s.write || s.detect));
  assert.throws(
    () => reg.ensureWritable('bdf'),
    // error::Operation travels as the `operation` attribute.
    (e) => e instanceof fp.NotImplementedFormatError && e.kind === 'NotImplemented' && e.operation === 'Write' && e.format === 'bdf',
  );
  assert.throws(
    () => reg.read(UNI3, { format: 'bdf' }),
    (e) => e instanceof fp.NotImplementedFormatError && e.operation === 'Read',
  );
  assert.throws(() => reg.detect(UNI3), fp.UnrecognizedFormatError);
  new fp.Registry().ensureWritable('bdf');
});

// --- detection --------------------------------------------------------------

test('detect content then name hint', () => {
  const d = fp.detect(UNI3);
  assert.equal(d.format.id, 'bdf');
  assert.equal(d.confidence, 'certain');
  assert.deepEqual(d, new fp.Registry().detect(UNI3));
  // No detector matches arbitrary bytes; the name hint decides.
  const junk = new TextEncoder().encode('\0\x01\x02 not a font');
  assert.throws(() => fp.detect(junk), fp.UnrecognizedFormatError);
  const hinted = fp.detect(junk, { name: 'font.pcf' });
  assert.equal(hinted.format.id, 'pcf');
  assert.equal(hinted.confidence, 'extension');
  assert.throws(() => fp.detect(junk, { name: 'font.fnt' }), fp.AmbiguousFormatError);
  // Content beats the name.
  assert.equal(fp.detect(UNI3, { name: 'font.pcf' }).format.id, 'bdf');
  // gzip transport is decoded before detection.
  assert.equal(fp.detect(gzipSync(UNI3)).format.id, 'bdf');
  assert.throws(() => new fp.Registry().detect(undefined, { directory: 'dir', resources: { 'dir/a': new Uint8Array() } }), fp.InvalidInputError);
  for (const buffer of [UNI3.slice().buffer, new DataView(UNI3.slice().buffer)]) {
    assert.equal(fp.detect(buffer).format.id, 'bdf');
  }
  assert.throws(() => fp.detect('not bytes'), (e) => e instanceof fp.InvalidInputError && /Uint8Array/.test(e.message));
  assert.throws(() => fp.detect(UNI3, { nmae: 'x' }), /unknown option `nmae`/);
});

// --- reading ----------------------------------------------------------------

test('read matches cli and reports', () => {
  const loaded = fp.read(UNI3);
  assert.equal(loaded.format, 'bdf');
  assert.ok(loaded.fonts instanceof fp.FontSet);
  assert.deepEqual(loaded.report, { entries: [] });
  const spleen = fp.read(UNI5);
  if (HAS_YAFF) assert.equal(sha(model(spleen)), GOLDEN.fixtures['bdf-spleen'].yaff);
  const one = spleen.intoSingle();
  assert.ok(one.fonts instanceof fp.Font);
  assert.equal(one.format, 'bdf');
  assert.equal(one.fonts.length, 472);
  if (HAS_YAFF) {
    // Several strikes cannot become one Font.
    const two = spleen.fonts.copy();
    two.append(two.font(0).copy());
    const many = fp.read(single(fp.write(two, 'yaff')));
    assert.equal(many.fonts.length, 2);
    assert.throws(() => many.intoSingle(), fp.InvalidInputError);
  }
  assert.match(String(loaded), /^Loaded\(format="bdf", report_entries=0\)$/);
});

test('read with options and explicit format', { skip: needs('hex') }, () => {
  const w = fp.write(fp.read(UNI3).fonts, 'hex', { options: { geometry: { height: 6, descent: 1 } } });
  const data = single(w);
  // Wrong geometry is an error, the matching one reads.
  assert.throws(() => fp.read(data, { format: 'hex' }), fp.FontpixelError);
  assert.equal(fp.read(data, { format: 'hex', options: { height: 6, descent: 1 } }).format, 'hex');
  // Options without a format apply to the detected format.
  assert.equal(fp.read(data, { options: { height: 6, descent: 1 } }).format, 'hex');
  assert.throws(() => fp.read(data, { format: 'hex', options: { height: 6, bogus: 1 } }), (e) => e instanceof fp.InvalidInputError && /unknown field/.test(e.message));
  assert.throws(() => fp.read(data, { format: 'no-such-format' }), fp.UnknownFormatError);
});

test('read errors and resource limits', () => {
  assert.throws(() => fp.read(new Uint8Array(7)), fp.UnrecognizedFormatError);
  assert.throws(
    () => fp.read(UNI3, { limits: { max_input_bytes: 100 } }),
    (e) => e instanceof fp.ResourceLimitError && e.resource === 'primary input bytes' && e.limit === 100,
  );
  assert.throws(() => fp.read(UNI5, { limits: { max_glyphs: 10 } }), (e) => e instanceof fp.ResourceLimitError && e.limit === 10);
  assert.throws(() => fp.read(gzipSync(UNI3), { limits: { max_decompressed_bytes: 1000 } }), fp.ResourceLimitError);
  assert.throws(() => fp.read(UNI3, { limits: { max_glyphs: -1 } }), fp.InvalidInputError);
  assert.throws(() => fp.read(UNI3, { limits: { max_gylphs: 1 } }), /unknown field/);
  assert.throws(
    () => fp.read(new TextEncoder().encode('STARTFONT 2.1\nSIZE x\n'), { format: 'bdf' }),
    (e) => e instanceof fp.BdfParseError && e.kind === 'BdfParse' && e.line === 2,
  );
  // The same failure from the native CLI.
  if (HAS_CLI) {
    const cli = cliFiles({ 'a.bdf': UNI3 }, ['convert', 'a.bdf', '-o', 'a.pcf', '--max-input-bytes', '100']);
    assert.notEqual(cli.code, 0);
    // The command line budgets the whole input asset (its message wording differs).
    assert.match(cli.stderr, /exceeds configured limit 100\b/);
  }
});

// --- writing ----------------------------------------------------------------

const LOSSY_READ = {
  entries: [
    {
      level: 'warning',
      impact: 'loss',
      code: 'test.loss',
      location: { font_index: null, glyph_index: null, codepoint: null, asset: null },
      message: 'earlier loss',
    },
  ],
};

test('write strict mode and reports', { skip: needs('psf', 'fzx') }, () => {
  const loaded = fp.read(UNI3);
  const w = fp.write(loaded.fonts, 'psf');
  const report = w.report;
  assert.ok(report.entries.some((e) => e.impact === 'loss'));
  assert.throws(
    () => fp.write(loaded.fonts, 'psf', { strict: true }),
    (e) => e instanceof fp.StrictLossError && e.kind === 'StrictLoss' && e.exitCode === 3 && JSON.stringify(e.report.entries) === JSON.stringify(report.entries),
  );
  // Loss-free output passes strict mode.
  assert.ok(single(fp.write(loaded.fonts, 'bdf', { strict: true })).length > 0);
  // Read diagnostics carried into the write make strict mode fail early.
  const reg = new fp.Registry();
  assert.throws(
    () => reg.writeWithReport(loaded.fonts, 'bdf', LOSSY_READ, { strict: true }),
    (e) => e instanceof fp.StrictLossError && JSON.stringify(e.report) === JSON.stringify(LOSSY_READ),
  );
  assert.equal(reg.writeWithReport(loaded.fonts, 'bdf', LOSSY_READ).report.entries[0].code, 'test.loss');
  const errorRead = { entries: [{ ...LOSSY_READ.entries[0], level: 'error' }] };
  assert.throws(
    () => reg.write(loaded.fonts, 'bdf', { report: errorRead }),
    (e) => e instanceof fp.ReportedErrorsError && JSON.stringify(e.report) === JSON.stringify(errorRead),
  );
  assert.throws(
    () => fp.write(loaded.fonts, 'fzx'),
    (e) => e instanceof fp.ReportedErrorsError && e.report.entries[0].code === 'fzx.write.unrepresentable_mapping',
  );
  assert.throws(() => fp.write(loaded.fonts, 'bdf', { stritc: true }), /unknown option `stritc`/);
  assert.throws(() => fp.write(loaded.fonts, 'bdf', { strict: 'yes' }), /must be a boolean/);
});

test('write variants agree and owned moves the model', () => {
  const reg = new fp.Registry();
  const loaded = fp.read(UNI3);
  const plain = reg.write(loaded.fonts, 'pcf');
  const reserved = reg.writeReservedWithReport(loaded.fonts, 'pcf', { report: loaded.report, transportLimits: {} });
  assert.deepEqual(assetsOf(plain), assetsOf(reserved));
  assert.deepEqual(plain.report, reserved.report);
  const moved = loaded.fonts.copy();
  const owned = reg.writeOwned(moved, 'pcf', { transportLimits: {} });
  assert.deepEqual(assetsOf(owned), assetsOf(plain));
  assert.equal(moved.length, 0);
  assert.equal(loaded.fonts.length, 1);
  if (enabled(fp, 'ttf')) {
    // An owned outline write takes the dedicated owned route.
    const out = reg.writeOwned(loaded.fonts.copy(), 'ttf');
    assert.deepEqual(assetsOf(out), assetsOf(reg.write(loaded.fonts, 'ttf')));
  }
  // A Font argument is accepted as a one-strike set; a font view too.
  const font = loaded.fonts.font(0);
  assert.deepEqual(assetsOf(reg.write(font, 'pcf')), assetsOf(plain));
  assert.deepEqual(assetsOf(reg.write(font.copy(), 'pcf')), assetsOf(plain));
  assert.throws(() => reg.write(new Uint8Array(3), 'pcf'), (e) => e instanceof fp.InvalidInputError && /expected a FontSet or Font, got Uint8Array/.test(e.message));
  assert.throws(() => reg.write(fp.Bitmap.fromText('#'), 'pcf'), /expected a FontSet or Font, got Bitmap/);
  if (HAS_CLI) {
    // The bytes equal the native CLI's conversion.
    const cli = cliFiles({ 'a.bdf': UNI3 }, ['convert', 'a.bdf', '-o', 'font.pcf', '--to', 'pcf']);
    assert.deepEqual(assetsOf(plain)['font.pcf'], cli.files['font.pcf']);
  }
});

test('transport limits and gzip envelope', { skip: needs('c') }, () => {
  const loaded = fp.read(UNI3);
  assert.throws(
    () => fp.write(loaded.fonts, 'bdf', { transportLimits: { max_output_bytes: 64 } }),
    // The BDF writer's own cap is intersected with the common limit.
    (e) => e instanceof fp.ResourceLimitError && e.resource === 'BDF output bytes' && e.limit === 64,
  );
  // A writer without a native cap hits the registry's aggregate check.
  assert.throws(() => fp.write(loaded.fonts, 'c', { transportLimits: { max_output_bytes: 64 } }), (e) => e instanceof fp.ResourceLimitError && e.limit === 64);
  assert.throws(() => fp.write(loaded.fonts, 'bdf', { transportLimits: { max_output: 1 } }), /unknown field/);
  const gz = fp.write(loaded.fonts, 'bdf', { primaryName: 'x.bdf.gz' });
  const data = assetsOf(gz)['x.bdf.gz'];
  assert.deepEqual([...data.slice(0, 2)], [0x1f, 0x8b]);
  assert.deepEqual(new Uint8Array(gunzipSync(data)), single(fp.write(loaded.fonts, 'bdf')));
  const back = fp.read(gz);
  assert.equal(back.format, 'bdf');
  assert.deepEqual(model(back), model(loaded));
  assert.throws(() => fp.write(loaded.fonts, 'bdf', { primaryName: 'x.bdf.gz', transportLimits: { max_output_bytes: 32 } }), fp.ResourceLimitError);
  assert.throws(() => fp.write(loaded.fonts, 'bdf', { primaryName: '../escape.bdf' }), fp.InvalidInputError);
  if (HAS_CLI) {
    const cli = cliFiles({ 'a.bdf': UNI3 }, ['convert', 'a.bdf', '-o', 'x.bdf.gz', '--to', 'bdf']);
    assert.deepEqual(data, cli.files['x.bdf.gz']);
  }
});

// --- sidecars, multi-asset and directory output -----------------------------

test('sidecar formats with every resource kind', { skip: needs('hbf') }, async () => {
  const loaded = fp.read(UNI5);
  const w = fp.write(loaded.fonts, 'hbf', { primaryName: 'font.hbf' });
  const files = assetsOf(w);
  assert.deepEqual(Object.keys(files).sort(), ['font.bin', 'font.hbf']);
  assert.deepEqual(hashes(w), GOLDEN.formats.hbf.write.assets);
  assert.throws(() => w.output.intoSingle(), (e) => e instanceof fp.MultipleAssetsError && e.count === 2);
  const expected = model(fp.read(w, { format: 'hbf' }));
  const header = files['font.hbf'];
  // object, Map, function and resolver object
  assert.deepEqual(model(fp.read(header, { format: 'hbf', resources: { 'font.bin': files['font.bin'] } })), expected);
  assert.deepEqual(model(fp.read(header, { name: 'font.hbf', resources: new Map([['font.bin', files['font.bin']]]) })), expected);
  const calls = [];
  const resolve = (p, maxBytes) => {
    calls.push([p, maxBytes]);
    return files[p];
  };
  assert.deepEqual(model(fp.read(header, { format: 'hbf', resources: resolve })), expected);
  assert.equal(calls[0][0], 'font.bin');
  assert.deepEqual(model(new fp.Registry().read(header, { format: 'hbf', resources: { resolve: (p) => files[p] } })), expected);
  // An OutputBundle reads back with its own name and sidecars.
  assert.deepEqual(model(fp.read(w.output, { format: 'hbf' })), expected);
  // Asynchronous sources: re-run until nothing new is requested.
  let fetched = 0;
  const viaAsync = await fp.withAsyncResources(
    (resources) => fp.read(header, { format: 'hbf', resources }),
    async (p) => {
      fetched++;
      return files[p] ?? null;
    },
  );
  assert.deepEqual(model(viaAsync), expected);
  assert.equal(fetched, 1);
  // Missing sidecar: an object without it, and a resolver throwing MissingResourceError.
  assert.throws(() => fp.read(header, { format: 'hbf' }), (e) => e instanceof fp.MissingResourceError && e.path === 'font.bin');
  assert.throws(
    () =>
      fp.read(header, {
        format: 'hbf',
        resources: (p) => {
          throw new fp.MissingResourceError(`no ${p}`, { kind: 'MissingResource', path: p });
        },
      }),
    fp.MissingResourceError,
  );
  // A resolver's own exception propagates unchanged.
  const boom = new RangeError('resolver failure');
  assert.throws(
    () =>
      fp.read(header, {
        format: 'hbf',
        resources: () => {
          throw boom;
        },
      }),
    (e) => e === boom,
  );
  // Per-resource budget and resource count.
  assert.throws(() => fp.read(header, { format: 'hbf', resources: files, limits: { max_resource_bytes: 16 } }), fp.ResourceLimitError);
  assert.throws(
    () => fp.read(header, { format: 'hbf', resources: files, limits: { max_resources: 0 } }),
    (e) => e instanceof fp.ResourceLimitError && e.resource === 'resource count',
  );
  assert.throws(() => fp.read(w, { format: 'hbf', name: 'x' }), fp.InvalidInputError);
  if (HAS_CLI) {
    const cli = cliFiles({ 'a.bdf': UNI5 }, ['convert', 'a.bdf', '-o', 'font.hbf', '--to', 'hbf']);
    assert.deepEqual(cli.files, files);
  }
});

test('directory output round trip', { skip: needs('consoleet') }, () => {
  const loaded = fp.read(LEG3);
  const w = fp.write(loaded.fonts, 'consoleet', { primaryName: 'font.txt' });
  assert.ok(w.output.isDirectory);
  assert.equal(w.output.primary, 'font.txt');
  const files = assetsOf(w);
  assert.ok(Object.keys(files).every((p) => p.startsWith('font.txt/')));
  const back = fp.read(w, { format: 'consoleet' });
  const relative = Object.fromEntries(Object.entries(files).map(([p, b]) => [p.slice('font.txt/'.length), b]));
  const same = fp.read(undefined, { format: 'consoleet', directory: 'font.txt', resources: relative });
  assert.deepEqual(model(same), model(back));
  assert.throws(() => fp.read(undefined, { directory: 'font.txt', resources: relative }), fp.InvalidInputError);
  assert.throws(() => fp.read(new Uint8Array([120]), { format: 'consoleet', directory: 'font.txt' }), fp.InvalidInputError);
  assert.throws(() => w.output.intoSingle(), fp.InvalidInputError);
  assert.deepEqual(hashes(w), GOLDEN.formats.consoleet.write.assets);
});

// --- JS codecs --------------------------------------------------------------

const MAGIC = new TextEncoder().encode('JSFONT\n');
const startsWithMagic = (bytes) => MAGIC.every((b, i) => bytes[i] === b);

test('js codec registration', () => {
  const reg = new fp.Registry();
  const seen = {};
  const detector = (ctx) => {
    assert.ok(ctx instanceof fp.HookContext);
    return startsWithMagic(ctx.bytes()) ? 'certain' : 'no_match';
  };
  const reader = (ctx, options) => {
    seen.options = options;
    seen.name = ctx.name;
    seen.directory = ctx.isDirectory;
    seen.side = ctx.resource('side.txt');
    seen.retained = ctx.retainedInputBytes();
    seen.limits = ctx.limits;
    seen.ctx = ctx;
    const inner = fp.read(ctx.bytes().slice(MAGIC.length));
    return [inner.fonts, { entries: [] }];
  };
  const writer = {
    write(fonts, options) {
      seen.writeOptions = options;
      const bdf = single(fp.write(fonts, 'bdf'));
      const body = new Uint8Array(MAGIC.length + bdf.length);
      body.set(MAGIC);
      body.set(bdf, MAGIC.length);
      return new fp.OutputBundle(options.primary_name, [
        { path: options.primary_name, data: body },
        ['side.txt', new TextEncoder().encode('hi')],
      ]);
    },
    transportLimits() {
      return { max_output_bytes: 1 << 20 };
    },
  };
  reg.register({ id: 'js-test', name: 'JS test', extensions: ['jsf'] }, { detector, reader, writer });
  assert.deepEqual(reg.status('js-test'), {
    format: { id: 'js-test', name: 'JS test', extensions: ['jsf'], feature_enabled: true },
    detect: true,
    read: true,
    write: true,
  });
  const loaded = fp.read(UNI3);
  const w = reg.write(loaded.fonts, 'js-test');
  assert.deepEqual(seen.writeOptions, { strict: false, primary_name: 'font.jsf', transport_limits: null });
  assert.deepEqual(Object.keys(assetsOf(w)).sort(), ['font.jsf', 'side.txt']);
  assert.equal(reg.detect(assetsOf(w)['font.jsf']).format.id, 'js-test');
  const back = reg.read(w);
  assert.equal(back.format, 'js-test');
  assert.deepEqual([...seen.side], [104, 105]);
  assert.equal(seen.name, 'font.jsf');
  assert.equal(seen.directory, false);
  assert.equal(seen.options.format, null);
  // The hook saw the registry's own context: the sidecar counts against the budget.
  assert.equal(seen.retained, assetsOf(w)['font.jsf'].length + 2);
  assert.equal(seen.limits.max_resources, 1024);
  assert.deepEqual(model(back), model(loaded));
  // The context is only valid during the hook.
  assert.throws(() => seen.ctx.bytes(), (e) => e instanceof fp.InvalidInputError && /has returned/.test(e.message));
  // A hook's resource reads are budgeted like a built-in codec's.
  assert.throws(() => reg.read(w, { limits: { max_resources: 0 } }), (e) => e instanceof fp.ResourceLimitError && e.resource === 'resource count');
  // The exception a hook throws propagates unchanged.
  const failure = new RangeError('custom failure');
  const reg2 = new fp.Registry();
  reg2.register({ id: 'js-broken', name: 'Broken' }, {
    reader() {
      throw failure;
    },
  });
  assert.throws(() => reg2.read(new Uint8Array([1]), { format: 'js-broken' }), (e) => e === failure);
  // A malformed hook result is a typed error.
  const reg4 = new fp.Registry();
  reg4.register({ id: 'js-bad', name: 'Bad' }, { reader: () => 42, writer: () => 'nope' });
  assert.throws(() => reg4.read(new Uint8Array([1]), { format: 'js-bad' }), /returns \[fonts, report\]/);
  assert.throws(() => reg4.write(loaded.fonts, 'js-bad'), /returns a Written, an OutputBundle/);
  // Planned registration, live entry, invalid IDs and hooks.
  reg2.register({ id: 'js-planned', name: 'Planned' });
  assert.equal(reg2.status('js-planned').read, false);
  assert.throws(() => reg2.register('bdf', { reader }), fp.InvalidInputError);
  assert.throws(() => reg2.register({ id: 'Bad_ID', name: 'x' }), fp.InvalidInputError);
  assert.throws(() => reg2.register({ id: 'js-x', name: 'x' }, { reader: 3 }), /must be a function/);
  assert.throws(() => reg2.register({ id: 'js-x', name: 'x' }, { rdr: reader }), /unknown option `rdr`/);
  // A catalogue ID on the planned registry, returning a Written or [bundle, report].
  const planned = new fp.Registry({ builtin: false });
  planned.register('bdf', { writer: (fonts, o) => fp.OutputBundle.single(o.primary_name, new Uint8Array([120])) });
  assert.deepEqual(assetsOf(planned.write(loaded.fonts, 'bdf')), { 'font.bdf': new Uint8Array([120]) });
  // gzip output of a JS writer uses the registry envelope.
  const gz = planned.write(loaded.fonts, 'bdf', { primaryName: 'a.bdf.gz' });
  assert.deepEqual([...gunzipSync(assetsOf(gz)['a.bdf.gz'])], [120]);
  const planned2 = new fp.Registry({ builtin: false });
  planned2.register('pcf', {
    writer: (fonts, o) => [fp.OutputBundle.single(o.primary_name, new Uint8Array([1])), LOSSY_READ],
  });
  assert.equal(planned2.write(loaded.fonts, 'pcf').report.entries[0].code, 'test.loss');
  assert.throws(() => planned2.write(loaded.fonts, 'pcf', { strict: true }), fp.StrictLossError);
  const planned3 = new fp.Registry({ builtin: false });
  planned3.register('pcf', { writer: (fonts, o) => new fp.Written(fp.OutputBundle.single(o.primary_name, new Uint8Array([2]))) });
  assert.deepEqual(single(planned3.write(loaded.fonts, 'pcf')), new Uint8Array([2]));
});

test('re-entrant access from a codec hook is a typed error end to end', () => {
  const loaded = fp.read(UNI3);
  const set = loaded.fonts.copy();
  const reg = new fp.Registry();
  const inside = {};
  reg.register({ id: 'js-reentrant', name: 'Re-entrant' }, {
    writer(fonts, options) {
      // The registry call holds `set` (shared) and the registry: mutating
      // either from the hook is refused with InvalidInputError, never a
      // panic or a wasm-bindgen aliasing error.
      try {
        set.append(fonts.font(0).copy());
      } catch (e) {
        inside.append = e;
      }
      try {
        reg.setWriteOptions('fzx', {});
      } catch (e) {
        inside.configure = e;
      }
      try {
        reg.register({ id: 'js-nested', name: 'Nested' });
      } catch (e) {
        inside.register = e;
      }
      // Reading the held values and nested reads are fine.
      inside.length = set.length;
      inside.nested = reg.read(UNI3).format;
      if (options.primary_name === 'throw.bin') set.pop();
      return fp.OutputBundle.single(options.primary_name, new Uint8Array([1]));
    },
  });
  reg.write(set, 'js-reentrant');
  const busy = (e, what) => e instanceof fp.InvalidInputError && e.kind === 'InvalidInput' && new RegExp(`${what} is in use by the running fontpixel call`).test(e.message);
  assert.ok(busy(inside.append, 'FontSet'), String(inside.append));
  assert.ok(busy(inside.configure, 'Registry'), String(inside.configure));
  assert.ok(busy(inside.register, 'Registry'), String(inside.register));
  assert.equal(inside.length, 1);
  assert.equal(inside.nested, 'bdf');
  assert.equal(set.length, 1);
  // Uncaught, the same typed error is rethrown unchanged by the outer call.
  assert.throws(
    () => reg.write(set, 'js-reentrant', { primaryName: 'throw.bin' }),
    (e) => busy(e, 'FontSet'),
  );
  // The registry and the set are usable again afterwards.
  set.append(set.font(0).copy());
  reg.setWriteOptions('pcf', {});
  assert.equal(set.length, 2);
});

test('released handles as arguments are typed errors', () => {
  const loaded = fp.read(UNI3);
  const set = loaded.fonts.copy();
  set.free();
  assert.throws(() => fp.write(set, 'bdf'), (e) => e instanceof fp.InvalidInputError && /FontSet handle was released/.test(e.message));
  const w = fp.write(loaded.fonts, 'bdf');
  const bundle = w.output;
  bundle.free();
  assert.throws(() => fp.read(bundle), /OutputBundle handle was released/);
  // The Written still owns the shared bundle.
  assert.equal(fp.read(w).format, 'bdf');
});

test('written and output bundle classes', () => {
  const loaded = fp.read(UNI3);
  const w = fp.write(loaded.fonts, 'bdf');
  assert.match(String(w), /^Written\(output=OutputBundle\(primary="font.bdf", assets=1, directory=false\), entries=0\)$/);
  const dict = w.toDict();
  assert.deepEqual(Object.keys(dict).sort(), ['output', 'report']);
  assert.equal(dict.output.primary, 'font.bdf');
  const again = fp.Written.fromDict(dict);
  assert.ok(again.output.equals(w.output));
  assert.deepEqual(fp.normalize('resources::Written', dict), dict);
  const b = new fp.OutputBundle('a.hbf', [['a.hbf', new Uint8Array([1, 2])], { path: 'a.bin', data: new Uint8Array([3]).buffer }]);
  assert.equal(b.length, 2);
  assert.equal(b.primary, 'a.hbf');
  assert.equal(b.isDirectory, false);
  assert.equal(b.retainedBytes(), b.retainedBytes(1 << 20));
  assert.throws(() => b.retainedBytes(1), fp.ResourceLimitError);
  // Assets are kept in path order.
  assert.deepEqual(b.intoAssets().map((a) => a.path), ['a.bin', 'a.hbf']);
  const copy = b.assets[1].data;
  copy[0] = 99;
  assert.equal(b.assets[1].data[0], 1);
  assert.ok(fp.OutputBundle.fromDict(b.toDict()).equals(b));
  assert.ok(!b.equals(w.output) && !b.equals({}));
  assert.throws(() => new fp.OutputBundle('missing', [['a', new Uint8Array()]]), fp.InvalidInputError);
  assert.throws(() => new fp.OutputBundle('a', [['../x', new Uint8Array()]]), fp.InvalidInputError);
  const dir = new fp.OutputBundle('d', [['d/x', new Uint8Array([1])]], { directory: true });
  assert.ok(dir.isDirectory);
  assert.throws(() => new fp.Written({}), fp.InvalidInputError);
  assert.deepEqual(new fp.Written(b.toDict()).output.toDict(), b.toDict());
  assert.deepEqual(new fp.Written(b, LOSSY_READ).report, LOSSY_READ);
});

test('case table covers option examples', () => {
  assert.deepEqual(caseOf(fp, 'hzk').write, { preset: 'asc16' });
});
