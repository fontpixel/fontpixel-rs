// W8.3: the merge page's calls `info(bytes)`, `merge(parts, options)` and
// `render(fontBytes, text, options)`, the Web Worker adapter
// (`createWorker`, `mergeInWorker`: progress, AbortSignal cancellation,
// terminate) and the Node file helpers.
//
// Identity: merge outputs (BDF and WOFF2; a monospace and a proportional
// merge) must equal the native CLI `merge` byte for byte for the same inputs
// and options, `info` must equal `info --json` and `render` must equal
// `preview`. The large merges (Unifont 57,086 glyphs + Tong Pixel Sans
// Latin; Tong SC + Tong Mono Latin) run when the samples exist; the large
// WOFF2 merge runs with FONTPIXEL_WASM_LARGE=1 (it takes minutes).
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { pathToFileURL } from 'node:url';
import { BLOCK, HAS_CLI, PKG, SPLEEN, buildInfo, cliFiles, load, sample } from './helpers.mjs';

const fp = await load();
const PROFILE = buildInfo().profile;
const WOFF2 = PROFILE !== 'minimal';
const decoder = new TextDecoder();
const text = (bytes) => decoder.decode(bytes);
const needsCli = !HAS_CLI && 'native CLI not found (FONTPIXEL_CLI)';

const S = sample(SPLEEN);
const B = sample(BLOCK);

const SAMPLES = '/home/chen/githubprojects/fontpixel/samples/tong-pixel-sans';
const UNIFONT_PATH =
  process.env.FONTPIXEL_UNIFONT ?? '/home/chen/githubprojects/fontpixel/reference/bdfparser/tests/fonts/unifont-13.0.04.bdf';
const LATIN_PATH = path.join(SAMPLES, 'TongPixelSans14Large-Latin.bdf');
const SC_PATH = path.join(SAMPLES, 'TongPixelSans14Large-SC.bdf');
const MONO_LATIN_PATH = path.join(SAMPLES, 'TongPixelSansMono14Large-Latin.bdf');
const HAS_UNIFONT = existsSync(UNIFONT_PATH) && existsSync(LATIN_PATH);
const HAS_TONG_MONO = existsSync(SC_PATH) && existsSync(MONO_LATIN_PATH);
const LARGE = process.env.FONTPIXEL_WASM_LARGE === '1';
const unifontSkip = needsCli || (!HAS_UNIFONT && `samples missing: ${UNIFONT_PATH}, ${LATIN_PATH}`);
const read = (file) => new Uint8Array(readFileSync(file));

/** A synthetic monospace CJK BDF (10x8 cells, ascent 7, descent 1, like Spleen 5x8). */
function cjkFont() {
  const codes = [0x3001, 0x3002, ...Array.from({ length: 32 }, (_, i) => 0x4e00 + i), 0x41];
  let out =
    'STARTFONT 2.1\nFONT -Test-Synthetic Han-Medium-R-Normal--8-80-75-75-C-100-ISO10646-1\nSIZE 8 75 75\n' +
    'FONTBOUNDINGBOX 10 8 0 -1\nSTARTPROPERTIES 6\nFAMILY_NAME "Synthetic Han"\nFONT_ASCENT 7\nFONT_DESCENT 1\n' +
    `SPACING "C"\nCOPYRIGHT "Test data"\nLICENSE "MIT"\nENDPROPERTIES\nCHARS ${codes.length}\n`;
  for (const [g, code] of codes.entries()) {
    const advance = code === 0x41 ? 5 : 10;
    out += `STARTCHAR U+${code.toString(16).toUpperCase()}\nENCODING ${code}\nSWIDTH ${advance * 100} 0\nDWIDTH ${advance} 0\nBBX 10 8 0 -1\nBITMAP\n`;
    for (let r = 0; r < 8; r++) out += `${(((g * 37 + r * 11) & 0x3ff) << 6).toString(16).toUpperCase().padStart(4, '0')}\n`;
    out += 'ENDCHAR\n';
  }
  return new TextEncoder().encode(`${out}ENDFONT\n`);
}
const HAN = cjkFont();

/** Run the CLI merge for `args` over `inputs`; returns {stdout, stderr, files}. */
function cliMerge(inputs, args) {
  const cli = cliFiles(inputs, ['merge', ...args]);
  assert.equal(cli.code, 0, cli.stderr);
  return cli;
}

function plainFiles(files) {
  return Object.fromEntries(Object.entries(files).map(([k, v]) => [k, new Uint8Array(v)]));
}

function sha256(bytes) {
  return globalThis.crypto.subtle.digest('SHA-256', bytes).then((d) => Buffer.from(d).toString('hex'));
}

/** Character art of a mono render result, re-derived from its bits. */
function artOf(result) {
  const rows = [];
  for (let y = 0; y < result.height; y++) {
    let row = '';
    for (let x = 0; x < result.width; x++) row += result.data[y * result.stride + (x >> 3)] & (0x80 >> (x & 7)) ? '#' : '.';
    rows.push(row);
  }
  return rows.join('\n');
}

// ------------------------------------------------------------ info

test('info equals info --json', { skip: needsCli }, () => {
  const inputs = { 's.bdf': S, 'b.bdf': B, 'han.bdf': HAN };
  for (const name of Object.keys(inputs)) {
    const cli = cliFiles(inputs, ['info', name, '--json']);
    assert.equal(cli.code, 0);
    assert.deepEqual(fp.info(inputs[name], { name }), JSON.parse(text(cli.stdout)), name);
    assert.equal(fp.ops.info({ input: name, json: true }, { files: inputs }).json(), text(cli.stdout));
  }
  // Options: the name defaults to "font"; info options are the request's.
  assert.equal(fp.info(S).format, 'bdf');
  assert.equal(fp.info(S.slice().buffer, { from: 'bdf' }).font_count, 1);
  assert.throws(() => fp.info(S, { input: 'x' }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2);
  assert.throws(() => fp.info(S, { bogus: 1 }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2 && /unknown field `bogus`/.test(e.message));
  assert.throws(
    () => fp.info(S, { max_input_bytes: 100 }),
    (e) => e instanceof fp.ResourceLimitError && e.limit === 100,
  );
  assert.throws(() => fp.info(new Uint8Array([1, 2, 3])), fp.UnrecognizedFormatError);
  assert.throws(() => fp.info([1, 2, 3]), fp.InvalidInputError);
});

// ------------------------------------------------------------ render

const PREVIEWS = [
  ['Hello', {}, []],
  ['Hi\nyo', { line_limit: 12 }, ['--line-limit', '12']],
  ['Ab', { direction: { glyphs: 'left', lines: 'down' } }, ['--direction', 'rltb']],
  ['Wide', { letter_spacing: 2, line_spacing: -1, mode: 'ink' }, ['--letter-spacing', '2', '--line-spacing=-1', '--mode', 'ink']],
  ['A一B', { missing: 'U+003F' }, ['--missing', 'U+003F']],
  ['Cell', { mode: 'cell', use_current_glyph_spacing: true }, ['--mode', 'cell', '--use-current-glyph-spacing']],
  [null, { all: true, range: 'U+41-4A', order: 'descending' }, ['--all', '--range', 'U+41-4A', '--order', 'descending']],
];

test('render equals preview', { skip: needsCli }, () => {
  for (const [input, options, flags] of PREVIEWS) {
    const args = ['preview', 's.bdf', ...(input === null ? [] : [input]), ...flags];
    const cli = cliFiles({ 's.bdf': S }, args);
    assert.equal(cli.code, 0, cli.stderr);
    const result = fp.render(S, input, { name: 's.bdf', ...options });
    assert.equal(result.format, 'mono');
    assert.equal(result.art + (result.art ? '\n' : ''), text(cli.stdout), args.join(' '));
    assert.equal(artOf(result), result.art, args.join(' '));
    assert.equal(result.stride, Math.ceil(result.width / 8));
    assert.ok(result.data instanceof Uint8Array);
    assert.deepEqual(result.report, fp.ops.preview({ input: 's.bdf', text: input ?? undefined, ...options }, { files: { 's.bdf': S } }).report);
  }
  // With missing glyphs the report says so, as on the command line.
  const missing = fp.render(S, '一', { name: 's.bdf' });
  assert.ok(missing.report.entries.length > 0);
});

test('render returns RGBA pixels for page previews', () => {
  const mono = fp.render(S, 'Hi', { name: 's.bdf' });
  const rgba = fp.render(S, 'Hi', {
    name: 's.bdf',
    format: 'rgba',
    scale: 3,
    color: { red: 10, green: 20, blue: 30, alpha: 255 },
    background: { red: 255, green: 255, blue: 255, alpha: 0 },
  });
  assert.ok(rgba.data instanceof Uint8ClampedArray);
  assert.equal(rgba.width, mono.width * 3);
  assert.equal(rgba.height, mono.height * 3);
  assert.equal(rgba.stride, rgba.width * 4);
  assert.equal(rgba.data.length, rgba.width * rgba.height * 4);
  assert.equal(rgba.art, mono.art);
  for (let y = 0; y < rgba.height; y++) {
    for (let x = 0; x < rgba.width; x++) {
      const ink = mono.data[(y / 3 | 0) * mono.stride + ((x / 3 | 0) >> 3)] & (0x80 >> ((x / 3 | 0) & 7));
      const pixel = [...rgba.data.subarray((y * rgba.width + x) * 4, (y * rgba.width + x) * 4 + 4)];
      assert.deepEqual(pixel, ink ? [10, 20, 30, 255] : [255, 255, 255, 0]);
    }
  }
  const plain = fp.render(S, 'Hi', { format: 'rgba' });
  assert.deepEqual([...plain.data.subarray(0, 4)], mono.data[0] & 0x80 ? [0, 0, 0, 255] : [255, 255, 255, 255]);
  // Errors: options are checked like requests (exit code 2), limits apply.
  assert.throws(() => fp.render(S, 'Hi', { format: 'png' }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2);
  assert.throws(() => fp.render(S, 'Hi', { scale: 2 }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2);
  assert.throws(() => fp.render(S, 'Hi', { format: 'rgba', scale: 0 }), (e) => e instanceof fp.InvalidInputError);
  assert.throws(() => fp.render(S, 'Hi', { text: 'x' }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2);
  assert.throws(() => fp.render(S, undefined), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2);
  assert.throws(() => fp.render(S, 'Hi', { bogus: true }), (e) => e instanceof fp.InvalidInputError && /unknown field `bogus`/.test(e.message));
  assert.throws(
    () => fp.render(S, 'Hi', { format: 'rgba', scale: 1000, max_pixels: 100000 }),
    (e) => e instanceof fp.ResourceLimitError && e.limit === 100000,
  );
  assert.throws(() => fp.render(S, 'Hi', { fail_missing: true, missing: 'U+41' }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2);
  assert.throws(() => fp.render(S, '一', { fail_missing: true }), fp.FontpixelError);
  const empty = fp.render(S, '');
  assert.equal(empty.art, '');
  assert.equal(empty.width * empty.height, 0);
});

// ------------------------------------------------------------ merge (small, identity)

const bytesEqual = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
/** Byte identity without a huge diff in the failure message. */
function assertBytes(actual, expected, label) {
  assert.ok(actual instanceof Uint8Array && expected instanceof Uint8Array, label);
  if (bytesEqual(actual, expected)) return;
  const first = actual.findIndex((v, i) => v !== expected[i]);
  assert.fail(`${label}: ${actual.length} bytes vs ${expected.length} bytes, first difference at ${first}`);
}
/** WOFF2 outputs of the identity tests, checked byte for byte at the end. */
const WOFF2_CASES = [];

/**
 * A WOFF2 output must decode to the same font as the CLI's (checked here);
 * its raw bytes are compared in "WOFF2 merge outputs equal the command line
 * byte for byte" (portable-float makes the Brotli stream platform independent).
 */
function assertSameWoff2(label, name, wasm, cli) {
  const decode = (bytes) => fp.read(bytes, { format: 'woff2' }).fonts.toDict();
  assert.deepEqual(decode(wasm), decode(cli), `${label}: ${name}`);
  WOFF2_CASES.push({ label, name, wasm, cli });
}

async function assertMergeIdentity(inputs, parts, options, args, label) {
  const cli = cliMerge(inputs, args);
  const result = fp.merge(parts, options);
  assert.deepEqual(Object.keys(result.files).sort(), Object.keys(cli.files).sort());
  for (const [name, bytes] of Object.entries(cli.files)) {
    if (name.endsWith('.woff2')) assertSameWoff2(label, name, result.files[name], bytes);
    else assertBytes(result.files[name], bytes, `${label}: ${name}`);
  }
  if (args.includes('--report') && args[args.indexOf('--report') + 1] === 'json') {
    assert.equal(result.json, text(cli.stdout));
    assert.deepEqual(result.report, JSON.parse(text(cli.stdout)));
  } else {
    assert.equal(result.text, text(cli.stdout));
  }
  return { cli, result };
}

test('monospace merge equals the command line (BDF and WOFF2)', { skip: needsCli }, async () => {
  const outputs = WOFF2 ? ['mono.bdf', 'mono.woff2'] : ['mono.bdf', 'mono.pcf'];
  const inputs = { 'han.bdf': HAN, 's.bdf': S };
  const parts = [
    { bytes: HAN, take: 'cjk', name: 'han.bdf' },
    { bytes: S, take: 'western,geometric', name: 's.bdf', dy: 0 },
  ];
  const options = { family: 'Mix Mono', spacing: 'mono', outputs, report: 'json' };
  const args = ['--family', 'Mix Mono', '--part', 'han.bdf', '--take', 'cjk', '--part', 's.bdf', '--take', 'western,geometric',
    '--spacing', 'mono', ...outputs.flatMap((o) => ['-o', o]), '--report', 'json'];
  const { result } = await assertMergeIdentity(inputs, parts, options, args, 'mono');
  assert.equal(result.report.merge.effective_spacing, 'mono');
  assert.equal(result.report.merge.full_width, 10);
  assert.ok(result.report.glyphs > 32);
  // auto resolves to mono here (both parts are cell-spaced); text report too.
  const auto = await assertMergeIdentity(inputs, parts, { ...options, spacing: 'auto', report: 'text' },
    args.map((a) => (a === 'mono' ? 'auto' : a)).filter((a) => a !== '--report' && a !== 'json'), 'mono auto');
  assert.equal(auto.result.report.merge.effective_spacing, 'mono');
  assert.deepEqual(auto.result.files, result.files);
});

test('proportional merge equals the command line (BDF and WOFF2)', { skip: needsCli }, async () => {
  const outputs = WOFF2 ? ['m.bdf', 'm.woff2'] : ['m.bdf', 'm.pcf'];
  const inputs = { 's.bdf': S, 'b.bdf': B };
  const parts = [
    { bytes: S, take: 'western', name: 's.bdf' },
    { bytes: B, take: 'all', name: 'b.bdf', dy: -1 },
  ];
  const options = { family: 'Mix', spacing: 'prop', fallback: true, outputs: outputs.map((destination) => ({ destination })), report: 'json' };
  const args = ['--family', 'Mix', '--part', 's.bdf', '--take', 'western', '--part', 'b.bdf', '--take', 'all', '--dy=-1',
    '--spacing', 'prop', '--fallback', ...outputs.flatMap((o) => ['-o', o]), '--report', 'json'];
  const { result } = await assertMergeIdentity(inputs, parts, options, args, 'prop');
  assert.equal(result.report.merge.effective_spacing, 'prop');
  assert.deepEqual(Object.keys(result.files).sort(), [...outputs].sort());
  // ops.merge on the same named buffers gives the same bytes and document.
  const viaOps = fp.ops.merge(
    { family: 'Mix', spacing: 'prop', fallback: true, report: 'json', parts: parts.map(({ bytes, name, ...p }) => ({ input: name, ...p })), outputs: options.outputs },
    { files: inputs },
  );
  assert.deepEqual(viaOps.outputs, result.files);
  assert.equal(viaOps.json(), result.json);
});

test('merge options mirror the command line and fail like it', { skip: needsCli }, () => {
  const parts = [{ bytes: S, take: 'all', name: 's.bdf' }];
  // A missing family: the same message and exit code as the command line.
  const cli = cliFiles({ 's.bdf': S }, ['merge', '--part', 's.bdf', '--take', 'all', '-o', 'm.bdf']);
  assert.equal(cli.code, 2);
  assert.throws(
    () => fp.merge(parts, { outputs: ['m.bdf'] }),
    (e) => e instanceof fp.InvalidInputError && e.exitCode === 2 && cli.stderr.includes(e.message.split('\n')[0]),
  );
  assert.throws(() => fp.merge(parts, { family: 'F', spacing: 'prop', outputs: ['m.bdf'], bogus: 1 }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2 && /unknown field `bogus`/.test(e.message));
  assert.throws(() => fp.merge(parts, { family: 'F', spacing: 'prop', outputs: ['m.bdf'], parts: [] }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2);
  assert.throws(() => fp.merge([{ bytes: S, take: 'all', bogus: 1 }], { family: 'F', spacing: 'prop', outputs: ['m.bdf'] }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2);
  assert.throws(() => fp.merge([{ take: 'all' }], { family: 'F', spacing: 'prop', outputs: ['m.bdf'] }), fp.InvalidInputError);
  assert.throws(() => fp.merge('s.bdf', { family: 'F', spacing: 'prop', outputs: ['m.bdf'] }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2);
  assert.throws(
    () => fp.merge([{ bytes: S, take: 'all', name: 'x.bdf' }, { bytes: B, take: 'all', name: 'x.bdf' }], { family: 'F', spacing: 'prop', outputs: ['m.bdf'] }),
    (e) => e instanceof fp.InvalidInputError && /already used/.test(e.message),
  );
  // The same bytes under one name may appear twice; default names are part-<n>.
  const twice = fp.merge([{ bytes: S, take: 'western' }, { bytes: S, take: 'all', name: 'part-1' }], { family: 'F', spacing: 'prop', outputs: ['m.bdf'], report: 'json' });
  assert.deepEqual(twice.report.merge.parts.map((p) => p.selection), ['western', 'all']);
  assert.throws(() => fp.merge(parts, { family: 'F', outputs: ['m.bdf'], spacing: 'diagonal' }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2);
  // Progress and cancellation as in ops.merge.
  const events = [];
  const observed = fp.merge(parts, { family: 'F', spacing: 'prop', outputs: ['m.bdf'], progress: (e) => events.push(e.step) });
  assert.deepEqual(events, ['read', 'merge', 'encode']);
  assert.deepEqual(observed.files, fp.merge(parts, { family: 'F', spacing: 'prop', outputs: ['m.bdf'] }).files);
  assert.throws(() => fp.merge(parts, { family: 'F', spacing: 'prop', outputs: ['m.bdf'], progress: () => false }), fp.CancelledError);
});

// ------------------------------------------------------------ merge (large, identity)

test('large merge of Unifont and Tong Latin equals the command line', { skip: unifontSkip, timeout: 600000 }, async (t) => {
  const unifont = read(UNIFONT_PATH);
  const latin = read(LATIN_PATH);
  const inputs = { 'latin.bdf': latin, 'unifont.bdf': unifont };
  const args = ['--family', 'Unifont Western Mix', '--part', 'latin.bdf', '--take', 'western', '--part', 'unifont.bdf', '--take', 'all',
    '--spacing', 'prop', '-o', 'merged.bdf', '--report', 'json'];
  let started = performance.now();
  const cli = cliMerge(inputs, args);
  const cliMs = performance.now() - started;
  const parts = [
    { bytes: latin, take: 'western', name: 'latin.bdf' },
    { bytes: unifont, take: 'all', name: 'unifont.bdf' },
  ];
  const events = [];
  started = performance.now();
  const result = fp.merge(parts, {
    family: 'Unifont Western Mix',
    spacing: 'prop',
    outputs: ['merged.bdf'],
    report: 'json',
    progress: (e) => events.push({ ...e, ms: performance.now() - started }),
  });
  const wasmMs = performance.now() - started;
  assert.equal(result.report.glyphs, 57106);
  assert.deepEqual(Object.keys(result.files), Object.keys(cli.files));
  assertBytes(result.files['merged.bdf'], cli.files['merged.bdf'], 'merged.bdf');
  assert.equal(result.json, text(cli.stdout));
  assert.deepEqual(events.map((e) => e.step), ['read', 'read', 'merge', 'encode']);
  const infoStarted = performance.now();
  const info = fp.info(unifont, { name: 'unifont.bdf' });
  const infoMs = performance.now() - infoStarted;
  assert.deepEqual(info, JSON.parse(text(cliFiles({ 'unifont.bdf': unifont }, ['info', 'unifont.bdf', '--json']).stdout)));
  t.diagnostic(
    `unifont+latin -> merged.bdf: ${result.files['merged.bdf'].length} bytes sha256 ${await sha256(result.files['merged.bdf'])}; ` +
      `wasm ${wasmMs.toFixed(0)} ms (steps ${events.map((e) => `${e.step}@${e.ms.toFixed(0)}`).join(' ')}), ` +
      `CLI process ${cliMs.toFixed(0)} ms; info(unifont) ${infoMs.toFixed(0)} ms`,
  );
});

test('large monospace merge of Tong SC and Tong Mono Latin equals the command line', { skip: needsCli || (!HAS_TONG_MONO && 'Tong samples missing'), timeout: 1800000 }, async (t) => {
  const sc = read(SC_PATH);
  const latin = read(MONO_LATIN_PATH);
  const inputs = { 'sc.bdf': sc, 'mono-latin.bdf': latin };
  const outputs = LARGE && WOFF2 ? ['mono.bdf', 'mono.woff2'] : ['mono.bdf'];
  const args = ['--family', 'Tong Mono Mix', '--part', 'sc.bdf', '--take', 'cjk', '--part', 'mono-latin.bdf', '--take', 'western',
    '--spacing', 'mono', '--allow-misaligned', ...outputs.flatMap((o) => ['-o', o]), '--report', 'json'];
  let started = performance.now();
  const cli = cliMerge(inputs, args);
  const cliMs = performance.now() - started;
  started = performance.now();
  const result = fp.merge(
    [
      { bytes: sc, take: 'cjk', name: 'sc.bdf' },
      { bytes: latin, take: 'western', name: 'mono-latin.bdf' },
    ],
    { family: 'Tong Mono Mix', spacing: 'mono', allow_misaligned: true, outputs, report: 'json' },
  );
  const wasmMs = performance.now() - started;
  assert.equal(result.report.merge.effective_spacing, 'mono');
  for (const name of outputs) {
    if (name.endsWith('.woff2')) assertSameWoff2('tong mono', name, result.files[name], cli.files[name]);
    else assertBytes(result.files[name], cli.files[name], name);
  }
  assert.equal(result.json, text(cli.stdout));
  const hashes = await Promise.all(outputs.map(async (o) => `${o} ${result.files[o].length} bytes sha256 ${await sha256(result.files[o])}`));
  t.diagnostic(`tong sc+mono latin (${result.report.glyphs} glyphs): ${hashes.join('; ')}; wasm ${wasmMs.toFixed(0)} ms, CLI process ${cliMs.toFixed(0)} ms`);
});

test('WOFF2 merge outputs equal the command line byte for byte', { skip: (needsCli || !WOFF2) && 'needs the CLI and WOFF2 output' }, async (t) => {
  assert.ok(WOFF2_CASES.length >= 2, 'the identity tests above produced WOFF2 outputs');
  const differing = [];
  for (const c of WOFF2_CASES) {
    const same = bytesEqual(c.wasm, c.cli);
    let diff = 0;
    for (let i = 0; i < Math.max(c.wasm.length, c.cli.length); i++) if (c.wasm[i] !== c.cli[i]) diff++;
    t.diagnostic(
      `${c.label} ${c.name}: wasm ${c.wasm.length} B sha256 ${await sha256(c.wasm)}, ` +
        `CLI ${c.cli.length} B sha256 ${await sha256(c.cli)}, ${same ? 'identical' : `${diff} bytes differ`}`,
    );
    if (!same) differing.push(`${c.label} ${c.name}`);
  }
  assert.deepEqual(differing, []);
});

// ------------------------------------------------------------ worker

test('merge in a worker equals merge, reports progress and keeps the main thread responsive', { skip: unifontSkip, timeout: 600000 }, async (t) => {
  const unifont = read(UNIFONT_PATH);
  const latin = read(LATIN_PATH);
  const parts = [
    { bytes: latin, take: 'western', name: 'latin.bdf' },
    { bytes: unifont, take: 'all', name: 'unifont.bdf' },
  ];
  const options = { family: 'Unifont Western Mix', spacing: 'prop', outputs: ['merged.bdf'], report: 'json' };
  const sync = fp.merge(parts, options);
  const syncEvents = [];
  fp.merge(parts, { ...options, progress: (e) => syncEvents.push(e) });
  const created = performance.now();
  const worker = await fp.createWorker();
  const startupMs = performance.now() - created;
  try {
    assert.equal(worker.cancellation, 'cooperative');
    let ticks = 0;
    const timer = setInterval(() => ticks++, 5);
    const events = [];
    const started = performance.now();
    const result = await worker.merge(parts, options, { onProgress: (e) => events.push(e) });
    const workerMs = performance.now() - started;
    clearInterval(timer);
    assert.deepEqual(result.files, sync.files);
    assert.equal(result.json, sync.json);
    assert.equal(result.text, sync.text);
    assert.deepEqual(events, syncEvents);
    // The main thread kept running while the worker merged 57,106 glyphs.
    assert.ok(ticks >= Math.min(10, Math.floor(workerMs / 20)), `${ticks} ticks in ${workerMs.toFixed(0)} ms`);
    assert.equal(worker.pending, 0);
    t.diagnostic(`worker merge ${workerMs.toFixed(0)} ms (startup ${startupMs.toFixed(0)} ms), ${ticks} main-thread ticks`);
  } finally {
    worker.terminate();
  }
  // mergeInWorker: a one-off worker, terminated afterwards.
  const once = await fp.mergeInWorker(parts, options);
  assert.deepEqual(once.files, sync.files);
});

test('worker calls equal the synchronous calls and keep error classes', async () => {
  const worker = await fp.createWorker({ module: await fp.compileModule() });
  try {
    const parts = [
      { bytes: S, take: 'western', name: 's.bdf' },
      { bytes: B, take: 'all', name: 'b.bdf', dy: -1 },
    ];
    const options = { family: 'Mix', outputs: ['m.bdf'], fallback: true, report: 'json' };
    assert.deepEqual(await worker.merge(parts, options), fp.merge(parts, options));
    assert.deepEqual(await worker.info(S, { name: 's.bdf' }), fp.info(S, { name: 's.bdf' }));
    assert.deepEqual(await worker.info(B), fp.info(B));
    const rgba = { name: 's.bdf', format: 'rgba', scale: 2 };
    assert.deepEqual(await worker.render(S, 'Hi', rgba), fp.render(S, 'Hi', rgba));
    assert.deepEqual(await worker.render(S, 'Hi'), fp.render(S, 'Hi'));
    // Errors are rebuilt with their class and fields.
    await assert.rejects(worker.merge(parts, { outputs: ['m.bdf'] }), (e) => e instanceof fp.InvalidInputError && e.exitCode === 2 && e.kind === 'InvalidInput');
    await assert.rejects(worker.info(new Uint8Array([1, 2, 3])), fp.UnrecognizedFormatError);
    await assert.rejects(
      worker.info(S, { max_input_bytes: 100 }),
      (e) => e instanceof fp.ResourceLimitError && e.limit === 100 && typeof e.resource === 'string',
    );
    await assert.rejects(worker.merge(parts, { ...options, strict: true }), (e) => e instanceof fp.StrictLossError && e.exitCode === 3 && Array.isArray(e.report?.entries));
    await assert.rejects(worker.merge(parts, { ...options, progress: () => {} }), fp.InvalidInputError);
    // The worker is still usable after errors; calls queue in order.
    const [a, b] = await Promise.all([worker.info(S), worker.info(B)]);
    assert.deepEqual([a, b], [fp.info(S), fp.info(B)]);
  } finally {
    worker.terminate();
  }
  await assert.rejects(fp.createWorker({ cancellation: 'never' }), fp.InvalidInputError);
});

test('AbortSignal cancels a worker merge cooperatively', { timeout: 600000 }, async () => {
  const big = HAS_UNIFONT ? read(UNIFONT_PATH) : HAN;
  const parts = [
    { bytes: S, take: 'western', name: 's.bdf' },
    { bytes: big, take: 'all', name: 'big.bdf' },
  ];
  const options = { family: 'Mix', spacing: 'prop', outputs: ['m.bdf'] };
  const worker = await fp.createWorker({ cancellation: 'cooperative' });
  try {
    // Abort after the first step: the next step boundary returns false.
    const controller = new AbortController();
    const seen = [];
    await assert.rejects(
      worker.merge(parts, options, {
        signal: controller.signal,
        onProgress: (e) => {
          seen.push(e.step);
          controller.abort();
        },
      }),
      (e) => e instanceof fp.CancelledError && e.kind === 'Cancelled' && e.exitCode === 1,
    );
    assert.equal(seen[0], 'read');
    // An already aborted signal rejects before anything is sent.
    await assert.rejects(worker.merge(parts, options, { signal: AbortSignal.abort() }), fp.CancelledError);
    await assert.rejects(worker.info(S, undefined, { signal: AbortSignal.abort() }), fp.CancelledError);
    // A throwing onProgress cancels too and its error is the rejection.
    const failure = new Error('stop here');
    await assert.rejects(
      worker.merge(parts, options, {
        onProgress: () => {
          throw failure;
        },
      }),
      (e) => e === failure,
    );
    // The same worker keeps working after cancellations.
    const done = await worker.merge(parts, options);
    assert.deepEqual(done.files, fp.merge(parts, options).files);
  } finally {
    worker.terminate();
  }
});

test('terminate cancellation and terminate() stop the worker at once', { timeout: 600000 }, async () => {
  const big = HAS_UNIFONT ? read(UNIFONT_PATH) : HAN;
  const parts = [{ bytes: big, take: 'all', name: 'big.bdf' }];
  const options = { family: 'Mix', spacing: 'prop', outputs: ['m.bdf'] };
  const worker = await fp.createWorker({ cancellation: 'terminate' });
  assert.equal(worker.cancellation, 'terminate');
  const controller = new AbortController();
  const pending = worker.merge(parts, options, { signal: controller.signal });
  const other = worker.info(S);
  setTimeout(() => controller.abort(), 5);
  await assert.rejects(pending, fp.CancelledError);
  await assert.rejects(other, fp.CancelledError); // every pending call of the terminated worker
  // The next call starts a fresh worker from the same module.
  assert.deepEqual(await worker.info(S), fp.info(S));
  // terminate(): hard stop; the worker cannot be used afterwards.
  const running = worker.merge(parts, options);
  worker.terminate();
  await assert.rejects(running, fp.CancelledError);
  await assert.rejects(worker.info(S), fp.InvalidInputError);
});

// ------------------------------------------------------------ Node file helpers

test('node file helpers read inputs and write outputs without transactions', async () => {
  const files = await import(pathToFileURL(path.join(PKG, 'ops-node-files.js')).href);
  const root = await mkdtemp(path.join(tmpdir(), 'fontpixel-wasm-node-files-'));
  try {
    await writeFile(path.join(root, 's.bdf'), S);
    const { mkdir } = await import('node:fs/promises');
    await mkdir(path.join(root, 'set', 'sub'), { recursive: true });
    await writeFile(path.join(root, 'set', 'b.bdf'), B);
    await writeFile(path.join(root, 'set', 'sub', 'x.bin'), new Uint8Array([1, 2]));
    const inputs = await files.readFiles(['s.bdf'], { root });
    assert.deepEqual(inputs['s.bdf'], S);
    const set = await files.readDirectory('set', { root });
    assert.deepEqual(Object.keys(set).sort(), ['set/b.bdf', 'set/sub/x.bin']);
    const result = fp.ops.convert({ inputs: ['s.bdf'], output: 'out/s.pcf' }, { files: { ...inputs, ...set } });
    const written = await files.writeOutputs(result.outputs, { root });
    assert.deepEqual(written, [path.join(root, 'out', 's.pcf')]);
    assert.deepEqual(new Uint8Array(await readFile(written[0])), result.outputs['out/s.pcf']);
    // No source protection: an output named like an input replaces it.
    await files.writeOutputs({ 's.bdf': new Uint8Array([0]) }, { root });
    assert.deepEqual(new Uint8Array(await readFile(path.join(root, 's.bdf'))), new Uint8Array([0]));
    // Not transactional: the first file stays written when a later one fails.
    await assert.rejects(
      files.writeOutputs({ 'new.bin': new Uint8Array([1]), 'out/s.pcf': new Uint8Array([2]) }, { root, overwrite: false }),
      (e) => e.code === 'EEXIST',
    );
    assert.ok(existsSync(path.join(root, 'new.bin')));
    // Names must stay inside the root.
    await assert.rejects(files.writeOutputs({ '../escape.bin': new Uint8Array() }, { root }), fp.InvalidInputError);
    await assert.rejects(files.writeOutputs({ '/abs.bin': new Uint8Array() }, { root }), fp.InvalidInputError);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
