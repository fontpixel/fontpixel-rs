// `ops`: every command-line operation in-process (fontpixel_ops), the JS
// counterpart of crates/fontpixel-py/tests/test_ops.py.
//
// Normal results are compared with the native CLI run on the same inputs:
// the JSON/text documents must equal its stdout byte for byte and every
// output its file bytes. Memory storage (named buffers, Maps, provider
// objects) is the only storage; filesystem storage throws UnsupportedError.
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { HAS_CLI, BLOCK, SPLEEN, buildInfo, cliFiles, has, load, runCli, sample, tempDir } from './helpers.mjs';

const fp = await load();
const { ops } = fp;
const PROFILE = buildInfo().profile;
const FULL = PROFILE === 'full';
const SFNT = has(fp, 'sfnt_operations');
const decoder = new TextDecoder();
const text = (bytes) => decoder.decode(bytes);

const S = sample(SPLEEN);
const B = sample(BLOCK);
const E = sample('embedded-bitmap/bdat-i1-f1-d1.ttf');
const INPUTS = { 's.bdf': S, 'b.bdf': B, 'e.ttf': E };
const needsCli = !HAS_CLI && 'native CLI not found (FONTPIXEL_CLI)';
const fullOnly = (reason = 'needs the full profile') => (FULL ? needsCli : reason);

/**
 * Like helpers.cliFiles, but also creates the (empty) directories `dirs`
 * the command line expects to exist; returns the files it left.
 */
function cliIn(inputs, args, dirs = []) {
  const { dir, cleanup } = tempDir();
  try {
    for (const sub of dirs) mkdirSync(path.join(dir, sub), { recursive: true });
    for (const [name, bytes] of Object.entries(inputs)) writeFileSync(path.join(dir, name), bytes);
    const result = runCli(args, dir);
    const files = {};
    const walk = (sub) => {
      for (const entry of readdirSync(path.join(dir, sub))) {
        const rel = sub ? `${sub}/${entry}` : entry;
        if (statSync(path.join(dir, rel)).isDirectory()) walk(rel);
        else if (!(rel in inputs)) files[rel] = new Uint8Array(readFileSync(path.join(dir, rel)));
      }
    };
    walk('');
    return { ...result, files, dir };
  } finally {
    cleanup();
  }
}

/** Plain `{name: bytes}` copy for deepEqual against CLI files. */
function plainFiles(files) {
  return Object.fromEntries(Object.entries(files).map(([k, v]) => [k, new Uint8Array(v)]));
}

/** CLI stdout/files vs the memory result of the parsed argv. */
function compareWriter(args, operation, { mkdirs = [] } = {}) {
  const parsed = ops.parseArgs(args);
  const cli = cliIn(INPUTS, args, mkdirs);
  assert.equal(cli.code, 0, cli.stderr);
  const result = operation(parsed.request, { files: INPUTS });
  const rendered = parsed.report_style === 'json' ? result.json() : result.text();
  assert.equal(rendered, text(cli.stdout), args.join(' '));
  assert.deepEqual(plainFiles(result.outputs), cli.files);
  return result;
}

// ------------------------------------------------------------ registry/queries

test('formats documents equal the command line', { skip: fullOnly() }, () => {
  const result = ops.formats();
  const json = cliFiles({}, ['formats', '--json']);
  assert.equal(json.code, 0);
  assert.equal(result.json(), text(json.stdout));
  assert.equal(ops.formats({ json: true }).json(), text(json.stdout));
  assert.equal(result.text(), text(cliFiles({}, ['formats']).stdout));
  assert.deepEqual(result.document, JSON.parse(text(json.stdout)));
  const status = result.formatStatus('bdf');
  assert.equal(status.format.id, 'bdf');
  assert.ok(status.read && status.write);
  assert.deepEqual(status, result.summary.formats.find((s) => s.format.id === 'bdf'));
  assert.equal(result.formatStatus('nope'), undefined);
  assert.equal(result.exitCode, 0);
  assert.deepEqual(result.outputs, {});
  assert.equal(result.report, null);
  assert.equal(result.operation, 'formats');
  const converted = ops.convert({ inputs: ['s.bdf'], output: 'o.pcf' }, { files: INPUTS });
  assert.throws(() => converted.formatStatus('bdf'), TypeError);
  assert.throws(() => converted.fontJson(0), TypeError);
});

test('formats list this build in every profile', () => {
  const formats = ops.formats({ json: true }).summary.formats;
  const bdf = formats.find((s) => s.format.id === 'bdf');
  assert.ok(bdf.read && bdf.write);
  const woff2 = formats.find((s) => s.format.id === 'woff2');
  assert.equal(Boolean(woff2?.write), PROFILE !== 'minimal');
});

test('capabilities defaults and keys', () => {
  assert.deepEqual(ops.capabilities(), { filesystem: false, threads: false, toml_batch: FULL });
  assert.deepEqual(ops.defaults(), {
    max_working_bytes: 2 ** 30,
    max_pending_bytes: 512 * 2 ** 20,
    max_report_bytes: 2 * 2 ** 30,
    max_task_report_bytes: 64 * 2 ** 20,
  });
  assert.equal(ops.memoryKey('./a/../b//c.bdf'), 'b/c.bdf');
  assert.equal(ops.memoryKey('/x/./y'), '/x/y');
  assert.equal(ops.outputFormatFor('a.pcf'), 'pcf');
  if (FULL) {
    assert.equal(ops.outputFormatFor('x/font.woff2'), 'woff2');
    assert.throws(
      () => ops.outputFormatFor('o.psf'),
      (e) =>
        e instanceof fp.AmbiguousFormatError &&
        e.candidates.includes('psf') &&
        e.candidates.includes('printshop') &&
        e.exitCode === 1,
    );
  }
  const names = fp.capabilities().map((c) => c.name);
  assert.ok(names.includes('cli_parser') && names.includes('sfnt_operations'));
});

test('parseArgs and inspectRequest share the command-line definition', { skip: !FULL && 'parseArgs needs the full profile (cli_parser)' }, () => {
  const parsed = ops.parseArgs(['convert', 'a.bdf', '-o', 'b.pcf', '--report', 'json', '--max-total-bytes', '1000000']);
  assert.equal(parsed.command, 'convert');
  assert.equal(parsed.report_style, 'json');
  assert.deepEqual(parsed.request.inputs, ['a.bdf']);
  assert.equal(parsed.request.max_total_bytes, 1000000);
  const inspected = ops.inspectRequest('vectorize', parsed.request);
  assert.deepEqual(inspected.request, parsed.request);
  assert.deepEqual(inspected.declared_inputs, ['a.bdf']);
  assert.equal(inspected.weight, inspected.source_identity_total_bytes);
  assert.equal(inspected.source_identity_total_bytes, parsed.source_identity_total_bytes);
  assert.deepEqual(ops.parseArgs(['batch', 'jobs.toml', '--jobs', '2']).request, { file: 'jobs.toml', jobs: 2 });
  assert.equal(ops.inspectRequest('info', { input: 'a.bdf', json: true }).report_style, 'json');
  assert.throws(
    () => ops.parseArgs(['convert', 'a.bdf', '--each', '-o', 'd']),
    (e) =>
      e instanceof fp.InvalidInputError &&
      e.exitCode === 2 &&
      e.message.includes('the following required arguments were not provided:\n  --to <TO>'),
  );
  assert.throws(() => ops.parseArgs(['--help']), (e) => e instanceof fp.InvalidInputError && e.exitCode === 0);
  assert.throws(() => ops.inspectRequest('nope', {}), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2);
  // Every operation's parsed request is accepted back unchanged.
  for (const argv of [
    ['merge', '--family', 'F', '--part', 'a.bdf', '--take', 'all', '-o', 'm.bdf'],
    ['diff', 'a.bdf', 'b.bdf'],
    ['validate', 'a.bdf', '--json'],
    ['charset', 'a.bdf', '-o', 'c.bdf', '--to', 'cp437'],
    ['variants', '--base', 'a.bdf', '--lang', 'ja=b.bdf', '-o', 'v.ttf'],
    ['subset', 'a.ttf', '--text', 'A', '-o', 's.ttf'],
    ['formats'],
    ['info', 'a.bdf'],
    ['preview', 'a.bdf', 'Hi'],
    ['render', 'a.bdf', 'Hi', '-o', 'h.png'],
  ]) {
    const described = ops.parseArgs(argv);
    const again = ops.inspectRequest(described.command, described.request);
    assert.deepEqual(again.request, described.request, argv.join(' '));
    assert.equal(again.report_style, described.report_style);
    assert.equal(again.source_identity_total_bytes, described.source_identity_total_bytes);
  }
});

test('inspectRequest validates requests in every profile', () => {
  const inspected = ops.inspectRequest('convert', { inputs: ['a.bdf'], output: 'b.pcf' });
  assert.equal(inspected.command, 'convert');
  assert.equal(inspected.report_style, 'text');
  assert.equal(inspected.request.to, null);
  assert.deepEqual(inspected.declared_inputs, ['a.bdf']);
  assert.equal(ops.inspectRequest('formats').report_style, 'text');
  assert.equal(ops.inspectRequest('preview', { input: 'a.bdf', text: 'A' }).declared_inputs, undefined);
  assert.throws(
    () => ops.inspectRequest('convert', { inputs: ['a.bdf'], output: 'b.pcf', stirct: true }),
    (e) => e instanceof fp.InvalidInputError && e.exitCode === 2 && /unknown field `stirct`/.test(e.message),
  );
});

// ------------------------------------------------------------ requests

const RULES = [
  [['convert', 's.bdf', '--each', '-o', 'd'], { inputs: ['s.bdf'], each: true, output: 'd' }],
  [['preview', 's.bdf', 'Hi', '--all'], { input: 's.bdf', text: 'Hi', all: true }],
  [['preview', 's.bdf', '--range', 'U+20-7E'], { input: 's.bdf', range: 'U+20-7E' }],
  [['preview', 's.bdf', 'Hi', '--missing', '?', '--fail-missing'], { input: 's.bdf', text: 'Hi', missing: '?', fail_missing: true }],
  [['subset', 'e.ttf', '--text', 'A', '--unicodes', 'U+41', '-o', 'x.ttf'], { input: 'e.ttf', text: 'A', unicodes: 'U+41', output: 'x.ttf' }],
  [['subset', 'e.ttf', '-o', 'x.ttf'], { input: 'e.ttf', output: 'x.ttf' }],
  [['convert', 's.bdf', '-o', 'x.bdf', '--c-width', '3'], { inputs: ['s.bdf'], output: 'x.bdf', c_width: 3 }],
  [['convert', 's.bdf', '-o', 'x.bdf', '--swidth-ppem', '0'], { inputs: ['s.bdf'], output: 'x.bdf', swidth_ppem: 0 }],
];

test('request rules reject like the command line', { skip: fullOnly() }, () => {
  for (const [args, request] of RULES) {
    const cli = cliFiles(INPUTS, args);
    assert.equal(cli.code, 2, args.join(' '));
    const message = cli.stderr.replace(/^error: /, '').split('\n\nUsage')[0].split('\n\nFor more')[0].trim();
    assert.throws(
      () => ops[args[0]](request, { files: INPUTS }),
      (e) => {
        assert.ok(e instanceof fp.InvalidInputError, args.join(' '));
        assert.equal(e.message, message, args.join(' '));
        assert.equal(e.exitCode, 2);
        assert.equal(e.kind, 'InvalidInput');
        return true;
      },
    );
  }
});

test('requests are strict and typed', () => {
  assert.throws(
    () => ops.convert({ inputs: ['s.bdf'], output: 'o.pcf', stirct: true }, { files: INPUTS }),
    (e) => e instanceof fp.InvalidInputError && /unknown field `stirct`/.test(e.message) && e.exitCode === 2,
  );
  assert.throws(
    () => ops.convert({ inputs: ['s.bdf'], output: 'o.pcf', glyph_pad: '3' }, { files: INPUTS }),
    (e) => e instanceof fp.InvalidInputError && /possible values/.test(e.message) && e.exitCode === 2,
  );
  assert.throws(() => ops.convert({ inputs: ['s.bdf'], output: 'o.pcf' }, { files: INPUTS, bogus: 1 }), fp.InvalidInputError);
  assert.throws(() => ops.convert(['s.bdf'], { files: INPUTS }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2);
  assert.throws(() => ops.convert({ inputs: [S], output: 'o.pcf' }, { files: INPUTS }), fp.InvalidInputError);
  // Numbers, typed enums and core shapes deserialize.
  if (FULL) {
    const r = ops.render(
      {
        input: 's.bdf',
        text: 'Hi',
        output: 'h.svg',
        shape: 'dot',
        color: { red: 1, green: 2, blue: 3, alpha: 255 },
        direction: { glyphs: 'right', lines: 'down' },
        scale: 2,
      },
      { files: INPUTS },
    );
    assert.ok(r.summary.width > 0);
    assert.deepEqual(Object.keys(r.outputs), ['h.svg']);
  }
});

// ------------------------------------------------------------ convert

const CONVERSIONS = {
  full: ['o.pcf', 'o.yaff', 'o.otb', 'o.ttf', 'o.woff2', 'o.svg', 'o.bdf', 'o.kbitx'],
  minimal: ['o.pcf', 'o.yaff', 'o.bdf'],
  merge: ['o.pcf', 'o.otb', 'o.ttf', 'o.woff2', 'o.bdf'],
  web: ['o.pcf', 'o.yaff', 'o.otb', 'o.ttf', 'o.woff2', 'o.bdf'],
}[PROFILE];

test('convert in memory equals the command line', { skip: needsCli }, () => {
  for (const output of CONVERSIONS) {
    const json = cliFiles(INPUTS, ['convert', 's.bdf', '-o', output, '--report', 'json']);
    assert.equal(json.code, 0, json.stderr);
    const plain = cliFiles(INPUTS, ['convert', 's.bdf', '-o', output]);
    const memory = ops.convert({ inputs: ['s.bdf'], output, report: 'json' }, { files: INPUTS });
    assert.equal(memory.json(), text(json.stdout), output);
    assert.equal(memory.text(), text(plain.stdout), output);
    assert.deepEqual(plainFiles(memory.outputs), json.files, output);
    assert.deepEqual(memory.destinations, [output]);
    const bundle = memory.bundles[output];
    assert.equal(bundle.primary, output);
    assert.deepEqual(new Uint8Array(bundle.assets[0].bytes), json.files[output]);
    assert.equal(memory.operation, 'convert');
    assert.equal(memory.report, null);
    assert.equal(memory.fonts, undefined);
  }
});

test('convert each names and options equal the command line', { skip: fullOnly() }, () => {
  const args = ['convert', 's.bdf', 'b.bdf', '--each', '--to', 'pcf', '-o', 'out', '--family', 'Fam', '--glyphpad', '1', '--report', 'json'];
  const cli = cliIn(INPUTS, args, ['out']);
  assert.equal(cli.code, 0, cli.stderr);
  const { request } = ops.parseArgs(args);
  const memory = ops.convert(request, { files: INPUTS });
  assert.equal(memory.json(), text(cli.stdout));
  assert.deepEqual(plainFiles(memory.outputs), cli.files);
  assert.equal(memory.summary.each, true);
  assert.equal(memory.summary.outputs.length, 2);
  const vectorized = ops.vectorize(request, { files: INPUTS });
  assert.equal(vectorized.json(), text(cli.stdout));
  assert.equal(vectorized.operation, 'vectorize');
});

// ------------------------------------------------------------ other writers

test('merge equals the command line', { skip: fullOnly() }, () => {
  const args = ['merge', '--family', 'Mix', '--part', 's.bdf', '--take', 'western', '--part', 'b.bdf', '--take', 'all', '--fallback',
    '-o', 'm.bdf', '-o', 'm.woff2', '-o', 'm.dat', '--to', 'pcf', '--report', 'json'];
  const result = compareWriter(args, ops.merge);
  assert.deepEqual(Object.keys(result.outputs).sort(), ['m.bdf', 'm.dat', 'm.woff2']);
  assert.equal(result.summary.family, 'Mix');
  assert.equal(result.summary.summaries.length, 3);
  assert.equal(result.report, null);
  compareWriter(args.filter((a) => a !== '--report' && a !== 'json'), ops.merge);
});

test('variants and subset equal the command line', { skip: fullOnly() }, () => {
  const variants = compareWriter(['variants', '--base', 's.bdf', '--lang', 'ja=s.bdf', '-o', 'v.ttf', '--report', 'json'], ops.variants);
  assert.equal(variants.summary.output, 'v.ttf');
  assert.ok(variants.report);
  compareWriter(['subset', 'e.ttf', '--text', 'AB', '-o', 's.woff2', '--report', 'json'], ops.subset);
  compareWriter(['subset', 'e.ttf', '--unicodes', 'U+41', '-o', 'u.ttf'], ops.subset);
});

test('charset and render equal the command line', { skip: fullOnly() }, () => {
  const charset = compareWriter(['charset', 's.bdf', '-o', 'c.bdf', '--to', 'cp437', '--report', 'json'], ops.charset);
  assert.ok(charset.summary.charset);
  assert.ok(charset.report);
  for (const args of [
    ['render', 's.bdf', 'Hello', '-o', 'h.png', '--scale', '3', '--report', 'json'],
    ['render', 's.bdf', 'Hi', '-o', 'h.svg', '--shape', 'dot'],
    ['render', 's.bdf', '--all', '--range', 'U+41-5A', '-o', 'a.png'],
  ]) {
    compareWriter(args, ops.render);
  }
});

// ------------------------------------------------------------ readers

test('readers equal the command line', { skip: needsCli }, () => {
  const cases = [
    [['validate', 's.bdf', '--json'], { input: 's.bdf', json: true }, ops.validate],
    [['validate', 's.bdf'], { input: 's.bdf' }, ops.validate],
    [['info', 's.bdf', '--json'], { input: 's.bdf', json: true }, ops.info],
    [['info', 's.bdf'], { input: 's.bdf' }, ops.info],
    [['diff', 's.bdf', 's.bdf', '--json'], { a: 's.bdf', b: 's.bdf', json: true }, ops.diff],
    [['diff', 's.bdf', 'b.bdf', '--json'], { a: 's.bdf', b: 'b.bdf', json: true }, ops.diff],
    [['diff', 's.bdf', 'b.bdf'], { a: 's.bdf', b: 'b.bdf' }, ops.diff],
    [['preview', 's.bdf', 'Hi', '--chars', '#.'], { input: 's.bdf', text: 'Hi', chars: '#.' }, ops.preview],
    [['preview', 's.bdf', '--all', '--range', 'U+41-43'], { input: 's.bdf', all: true, range: 'U+41-43' }, ops.preview],
  ];
  if (FULL) cases.push([['validate', 'e.ttf', '--json'], { input: 'e.ttf', json: true }, ops.validate]);
  for (const [args, request, operation] of cases) {
    const cli = cliFiles(INPUTS, args);
    const result = operation(request, { files: INPUTS });
    const jsonStyle = args.includes('--json');
    assert.equal(jsonStyle ? result.json() : result.text(), text(cli.stdout), args.join(' '));
    assert.equal(result.exitCode, cli.code, args.join(' '));
    assert.deepEqual(result.outputs, {});
    if (FULL) assert.deepEqual(ops.parseArgs(args).request, ops.inspectRequest(args[0], request).request, args.join(' '));
  }
});

test('reader results expose typed fields', () => {
  const info = ops.info({ input: 's.bdf' }, { files: INPUTS });
  assert.ok(info.fonts instanceof fp.FontSet);
  assert.equal(info.fonts.length, 1);
  assert.equal(info.summary.format, 'bdf');
  assert.equal(info.summary.font_count, 1);
  assert.deepEqual(JSON.parse(info.fontJson(0)), info.document.fonts[0]);
  assert.equal(JSON.parse(info.fontJson(0, false)).glyph_count, info.fonts.font(0).length);
  assert.ok(Array.isArray(info.report.entries));
  assert.throws(() => info.fontJson(5), fp.FontpixelError);
  // The fonts handle shares the result's storage; edits do not change the documents.
  const before = info.json();
  info.fonts.font(0).removeGlyph(info.fonts.font(0).glyphIds()[0]);
  assert.equal(info.fonts.font(0).length, JSON.parse(before).fonts[0].glyph_count - 1);
  assert.equal(info.json(), before);
  const same = ops.diff({ a: 's.bdf', b: 's.bdf' }, { files: INPUTS });
  assert.equal(same.summary.equal, true);
  assert.equal(same.exitCode, 0);
  const other = ops.diff({ a: 's.bdf', b: 'b.bdf', max_differences: 2 }, { files: INPUTS });
  assert.equal(other.summary.equal, false);
  assert.equal(other.exitCode, 1);
  assert.ok(other.summary.differences.length <= 2);
  assert.ok(other.summary.difference_count >= other.summary.differences.length);
  const check = ops.validate({ input: 's.bdf' }, { files: INPUTS });
  assert.equal(check.summary.kind, 'native');
  assert.equal(check.summary.format, 'bdf');
  if (FULL) assert.equal(ops.validate({ input: 'e.ttf' }, { files: INPUTS }).summary.kind, 'sfnt');
  const preview = ops.preview({ input: 's.bdf', text: 'A', chars: 'X.' }, { files: INPUTS });
  assert.ok(preview.summary.text.includes('X'));
  assert.equal(preview.text().replace(/\n$/, ''), preview.summary.text);
  assert.deepEqual(JSON.parse(preview.json()), preview.document);
});

// ------------------------------------------------------------ errors, strict, limits

test('errors match the command line', { skip: needsCli }, () => {
  const cases = [
    // strict loss: exit 3 with the full report
    [['convert', 's.bdf', '-o', 'x.pcf', '--strict', '--report', 'json'], { inputs: ['s.bdf'], output: 'x.pcf', strict: true, report: 'json' }, ops.convert, fp.StrictLossError, 3],
    // resource limits: exit 1 with the resource and limit
    [['convert', 's.bdf', '-o', 'x.pcf', '--max-input-bytes', '100', '--report', 'json'], { inputs: ['s.bdf'], output: 'x.pcf', max_input_bytes: 100, report: 'json' }, ops.convert, fp.ResourceLimitError, 1],
    [['info', 's.bdf', '--max-input-bytes', '100', '--json'], { input: 's.bdf', max_input_bytes: 100, json: true }, ops.info, fp.ResourceLimitError, 1],
  ];
  if (FULL) {
    // the writer rejects the font
    cases.push([['convert', 's.bdf', '-o', 'x.hex', '--report', 'json'], { inputs: ['s.bdf'], output: 'x.hex', report: 'json' }, ops.convert, fp.FontpixelError, 1]);
  }
  for (const [args, request, operation, Class, exitCode] of cases) {
    const cli = cliFiles(INPUTS, args);
    assert.equal(cli.code, exitCode, args.join(' '));
    assert.deepEqual(cli.files, {});
    const document = JSON.parse(cli.stderr);
    assert.throws(
      () => operation(request, { files: INPUTS }),
      (e) => {
        assert.ok(e instanceof Class, args.join(' '));
        assert.equal(e.exitCode, exitCode);
        assert.equal(e.message, document.error.message);
        assert.deepEqual(e.report ?? null, document.report);
        return true;
      },
    );
  }
  assert.throws(
    () => ops.convert({ inputs: ['s.bdf'], output: 'x.pcf', max_input_bytes: 100 }, { files: INPUTS }),
    (e) => e instanceof fp.ResourceLimitError && e.limit === 100 && typeof e.resource === 'string',
  );
});

test('missing inputs and provider errors', () => {
  assert.throws(
    () => ops.info({ input: 'missing.bdf' }, { files: INPUTS }),
    (e) => e instanceof fp.FontpixelIoError && e.message.includes('no such memory file: missing.bdf') && e.exitCode === 1,
  );
  class Boom extends Error {}
  const thrown = new Boom('boom s.bdf');
  const broken = {
    entry: (path) => (path === 's.bdf' ? S.length : null),
    read: () => {
      throw thrown;
    },
    list: () => [],
  };
  assert.throws(() => ops.info({ input: 's.bdf' }, { files: broken }), (e) => e === thrown);
  const badEntry = { entry: () => 'file', read: () => S, list: () => [] };
  assert.throws(() => ops.info({ input: 's.bdf' }, { files: badEntry }), (e) => e instanceof fp.InvalidInputError && /entry\(\) must return/.test(e.message));
  assert.throws(() => ops.info({ input: 's.bdf' }, { files: 42 }), fp.InvalidInputError);
  assert.throws(() => ops.info({ input: 's.bdf' }, { files: { 's.bdf': 'text' } }), fp.InvalidInputError);
  assert.throws(() => ops.info({ input: 's.bdf' }, { files: { entry: () => null } }), fp.InvalidInputError);
});

test('provider objects and Maps match named buffers', () => {
  const files = { 'fonts/s.bdf': S };
  const reads = [];
  const provider = {
    entry(path) {
      if (path in files) return files[path].length;
      const prefix = path ? `${path}/` : '';
      return Object.keys(files).some((k) => k.startsWith(prefix)) ? 'directory' : null;
    },
    read(path) {
      reads.push(path);
      return files[path];
    },
    list(directory) {
      const prefix = directory ? `${directory}/` : '';
      return [...new Set(Object.keys(files).filter((k) => k.startsWith(prefix)).map((k) => k.slice(prefix.length).split('/')[0]))].sort();
    },
  };
  const request = { inputs: ['fonts/s.bdf'], output: 'o.pcf' };
  const a = ops.convert(request, { files: provider });
  const b = ops.convert(request, { files: new Map([['fonts/s.bdf', S]]) });
  const c = ops.convert(request, { files: { 'fonts/s.bdf': S.slice().buffer } });
  const d = ops.convert(request, { files: { 'fonts/s.bdf': new DataView(S.slice().buffer) } });
  const e = ops.convert(request, { files: { 'fonts/./s.bdf': S } });
  for (const other of [b, c, d, e]) assert.deepEqual(other.outputs, a.outputs);
  assert.deepEqual(reads, ['fonts/s.bdf']);
  // A bigint length is accepted from entry().
  const big = { ...provider, entry: (path) => (path === 'fonts/s.bdf' ? BigInt(S.length) : provider.entry(path)) };
  assert.deepEqual(ops.convert(request, { files: big }).outputs, a.outputs);
});

test('memory outputs with sidecars are grouped by destination', { skip: !FULL && 'needs the full profile' }, () => {
  const result = ops.convert(
    { inputs: ['s.bdf'], output: 'atlas/s.png', to: 'image', image_layout: 'imageset', image_domain: 'unicode' },
    { files: INPUTS },
  );
  const keys = Object.keys(result.outputs);
  assert.ok(keys.length > 1 && keys.every((k) => k.startsWith('atlas/s.png/')));
  const bundle = result.bundles['atlas/s.png'];
  assert.equal(bundle.directory, true);
  assert.equal(bundle.assets.length, keys.length);
  assert.deepEqual(result.destinations, ['atlas/s.png']);
  assert.throws(
    () =>
      ops.merge(
        { family: 'F', spacing: 'prop', parts: [{ input: 's.bdf', take: 'all' }], outputs: [{ destination: 'm.bdf' }, { destination: 'm.bdf' }] },
        { files: INPUTS },
      ),
    (e) => e instanceof fp.InvalidInputError && /overlap/.test(e.message),
  );
});

test('filesystem storage is unavailable and throws UnsupportedError', () => {
  const check = (call, entry) =>
    assert.throws(call, (e) => {
      assert.ok(e instanceof fp.UnsupportedError, entry);
      assert.equal(e.capability, 'filesystem');
      assert.equal(e.kind, 'Unsupported');
      assert.ok(e.reason && e.evidence && e.alternative);
      assert.ok(e.message.includes(entry), e.message);
      return true;
    });
  check(() => ops.convert({ inputs: ['s.bdf'], output: 'o.pcf' }), 'ops.convert()');
  check(() => ops.vectorize({ inputs: ['s.bdf'], output: 'o.pcf' }, {}), 'ops.vectorize()');
  check(() => ops.merge({ family: 'F', parts: [{ input: 's.bdf', take: 'all' }], outputs: [{ destination: 'm.bdf' }] }), 'ops.merge()');
  check(() => ops.info({ input: 's.bdf' }), 'ops.info()');
  check(() => ops.preview({ input: 's.bdf', text: 'A' }, { files: null }), 'ops.preview()');
  check(() => ops.diff({ a: 's.bdf', b: 's.bdf' }), 'ops.diff()');
  check(() => ops.validate({ input: 's.bdf' }), 'ops.validate()');
  check(() => ops.charset({ input: 's.bdf', output: 'c.bdf', to: 'cp437' }), 'ops.charset()');
  check(() => ops.render({ input: 's.bdf', text: 'A', output: 'a.svg' }), 'ops.render()');
  check(() => ops.batch({ tasks: [] }), 'ops.batch()');
  check(() => ops.batchToml('version = 1'), 'ops.batchToml()');
  check(() => ops.batchFile('jobs.toml'), 'ops.batchFile()');
  check(() => ops.batchFile('jobs.toml', { jobs: 2 }), 'ops.batchFile()');
  if (SFNT) {
    check(() => ops.variants({ base: 's.bdf', output: 'v.ttf' }), 'ops.variants()');
    check(() => ops.subset({ input: 'e.ttf', output: 's.ttf', text: 'A' }), 'ops.subset()');
  }
  assert.throws(() => fp.requireCapability('filesystem'), fp.UnsupportedError);
});

test('profile capabilities gate parseArgs, batchToml and sfnt operations', () => {
  const caps = Object.fromEntries(fp.capabilities().map((c) => [c.name, c]));
  for (const name of ['cli_parser', 'sfnt_operations', 'toml_batch']) {
    const cap = caps[name];
    if (!cap.available) assert.ok(cap.reason && cap.evidence && cap.alternative, name);
  }
  assert.equal(caps.cli_parser.available, FULL);
  assert.equal(caps.sfnt_operations.available, PROFILE !== 'minimal');
  const unsupported = (call, capability) =>
    assert.throws(call, (e) => e instanceof fp.UnsupportedError && e.capability === capability && e.reason.length > 0);
  if (!caps.cli_parser.available) unsupported(() => ops.parseArgs(['formats']), 'cli_parser');
  if (!caps.toml_batch.available) {
    unsupported(() => ops.batchToml('version = 1', { files: INPUTS }), 'toml_batch');
    unsupported(() => ops.parseBatchToml('version = 1'), 'toml_batch');
  }
  if (!caps.sfnt_operations.available) {
    unsupported(() => ops.variants({ base: 's.bdf', output: 'v.ttf' }, { files: INPUTS }), 'sfnt_operations');
    unsupported(() => ops.subset({ input: 'e.ttf', output: 's.ttf', text: 'A' }, { files: INPUTS }), 'sfnt_operations');
    unsupported(() => ops.inspectRequest('subset', {}), 'sfnt_operations');
  }
});

// ------------------------------------------------------------ batch

const JOB = `version = 1
jobs = 1
[[tasks]]
id = "pcf"
kind = "convert"
inputs = ["s.bdf"]
output = "out/s.pcf"
[[tasks]]
id = "ttf"
kind = "vectorize"
inputs = ["b.bdf"]
output = "out/b.ttf"
[tasks.options]
shape = "dot"
[[tasks]]
id = "check"
kind = "validate"
inputs = ["s.bdf"]
[[tasks]]
id = "bad"
kind = "convert"
inputs = ["s.bdf"]
output = "out/bad.psf"
to = "psf"
[tasks.options]
psf_version = "3"
[[tasks]]
id = "mix"
kind = "merge"
family = "Mix"
outputs = ["out/m.bdf"]
[tasks.options]
spacing = "prop"
[[tasks.parts]]
input = "s.bdf"
take = "all"
`;
const TYPED = {
  jobs: 1,
  tasks: [
    { id: 'pcf', kind: 'convert', inputs: ['s.bdf'], output: 'out/s.pcf', report: 'json' },
    { id: 'ttf', kind: 'vectorize', inputs: ['b.bdf'], output: 'out/b.ttf', shape: 'dot', report: 'json' },
    { id: 'check', kind: 'validate', input: 's.bdf', json: true },
    { id: 'bad', kind: 'convert', inputs: ['s.bdf'], output: 'out/bad.psf', to: 'psf', psf_version: '3', report: 'json' },
    { id: 'mix', kind: 'merge', family: 'Mix', spacing: 'prop', parts: [{ input: 's.bdf', take: 'all' }], outputs: [{ destination: 'out/m.bdf' }], report: 'json' },
  ],
};

test('batch toml and typed tasks equal the command line', { skip: fullOnly() }, () => {
  const job = new TextEncoder().encode(JOB);
  const cli = cliIn({ ...INPUTS, 'jobs.toml': job }, ['batch', 'jobs.toml'], ['out']);
  // Task paths are resolved against the job file's directory.
  const stdout = text(cli.stdout).replaceAll(`${cli.dir}/`, '');
  const document = JSON.parse(stdout);
  assert.equal(cli.code, 2);
  assert.deepEqual(document.tasks.map((t) => t.kind).slice(0, 2), ['convert', 'vectorize']);

  const memory = ops.batchToml(JOB, { files: INPUTS });
  assert.equal(memory.exitCode, cli.code);
  assert.deepEqual(plainFiles(memory.outputs), cli.files);
  assert.equal(memory.json(), stdout);
  assert.deepEqual(memory.document.tasks.map((t) => t.id), ['pcf', 'ttf', 'check', 'bad', 'mix']);
  assert.deepEqual(Object.keys(memory.bundles).sort(), ['out/b.ttf', 'out/m.bdf', 'out/s.pcf']);
  assert.deepEqual(memory.tasks.map((t) => t.kind), ['convert', 'vectorize', 'validate', 'convert', 'merge']);
  assert.equal(memory.tasks[3].exit_code, 2);
  assert.equal(memory.tasks[3].success, false);
  assert.equal(memory.jobs, 1);
  assert.equal(memory.workers, 1);

  const typed = ops.batch(TYPED, { files: INPUTS });
  assert.equal(typed.exitCode, cli.code);
  assert.deepEqual(plainFiles(typed.outputs), cli.files);
  const strip = (doc) => {
    for (const t of doc.tasks) delete t.error; // messages differ: serde text vs parsed command line
    return doc;
  };
  assert.deepEqual(strip(typed.document), strip(memory.document));
  assert.ok(typed.tasks[3].error.includes("invalid value '3' for psf_version"));
});

test('typed batch runs in every profile', () => {
  const result = ops.batch(
    {
      tasks: [
        { id: 'pcf', kind: 'convert', inputs: ['s.bdf'], output: 'out/s.pcf' },
        { id: 'check', kind: 'validate', input: 's.bdf' },
        { id: 'nope', kind: 'convert', inputs: ['s.bdf'], output: 'out/x.pcf', bogus: 1 },
      ],
    },
    { files: INPUTS },
  );
  assert.equal(result.exitCode, 2);
  assert.deepEqual(Object.keys(result.outputs), ['out/s.pcf']);
  assert.deepEqual(result.outputs['out/s.pcf'], ops.convert({ inputs: ['s.bdf'], output: 'out/s.pcf' }, { files: INPUTS }).outputs['out/s.pcf']);
  assert.deepEqual(result.tasks.map((t) => t.success), [true, true, false]);
  assert.ok(result.tasks[2].error.includes('unknown field `bogus`'));
  assert.equal(result.workers, 1);
});

test('whole batch failures carry the command-line document', () => {
  if (FULL && HAS_CLI) {
    const bad = "version = 2\n[[tasks]]\nid='a'\nkind='validate'\n";
    const cli = cliFiles({ 'jobs.toml': new TextEncoder().encode(bad) }, ['batch', 'jobs.toml']);
    assert.throws(
      () => ops.batchToml(bad, { files: {} }),
      (e) => e instanceof fp.InvalidInputError && e.exitCode === cli.code && cli.code === 2 && e.document === text(cli.stdout),
    );
  }
  assert.throws(() => ops.batch({ tasks: [] }, { files: INPUTS }), (e) => e instanceof fp.InvalidInputError && JSON.parse(e.document).exit_code === 2);
  assert.throws(() => ops.batch({ tasks: [{ kind: 'validate', input: 's.bdf' }] }, { files: INPUTS }), (e) => e instanceof fp.InvalidInputError && /needs a string id/.test(e.message) && e.exitCode === 2);
  assert.throws(() => ops.batch({ tasks: [], bogus: 1 }, { files: INPUTS }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2);
});

test('a batch that needs several workers throws UnsupportedError', () => {
  const tasks = [
    { id: 'a', kind: 'validate', input: 's.bdf' },
    { id: 'b', kind: 'validate', input: 'b.bdf' },
  ];
  assert.throws(
    () => ops.batch({ jobs: 2, tasks }, { files: INPUTS }),
    (e) => {
      assert.ok(e instanceof fp.UnsupportedError, `${e.name}: ${e.message}`);
      assert.equal(e.ioKind, 'Unsupported');
      assert.equal(JSON.parse(e.document).exit_code, 1);
      return true;
    },
  );
  assert.equal(ops.batch({ jobs: 1, tasks }, { files: INPUTS }).exitCode, 0);
});

test('parseBatchToml lists prepared tasks', { skip: !FULL && 'needs the full profile (toml_batch)' }, () => {
  const parsed = ops.parseBatchToml(JOB, { base: 'root' });
  assert.equal(parsed.jobs, 1);
  const tasks = Object.fromEntries(parsed.tasks.map((t) => [t.id, t]));
  assert.equal(tasks.ttf.kind, 'vectorize');
  assert.equal(tasks.ttf.request.kind, 'vectorize');
  assert.equal(tasks.ttf.request.shape, 'dot');
  assert.deepEqual(tasks.pcf.declared_inputs, ['root/s.bdf']);
  assert.equal(tasks.pcf.weight, 256 * 1024 * 1024);
  assert.ok(tasks.bad.error.includes("invalid value '3'"));
  assert.throws(() => ops.parseBatchToml('version = 2'), fp.InvalidInputError);
  assert.throws(() => ops.parseBatchToml(JOB, { bogus: 1 }), fp.InvalidInputError);
});
