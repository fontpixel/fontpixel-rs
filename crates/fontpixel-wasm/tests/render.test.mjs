// Rendering: Font.draw/drawCodepoints/drawAll, Rendered, RenderOptions,
// PNG/SVG export and colors (ported from crates/fontpixel-py/tests/
// test_render.py, which ports crates/fontpixel/tests/render.rs), plus byte
// comparisons with the native CLI's `preview` and `render`.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { inflateSync } from 'node:zlib';
import { HAS_CLI, SPLEEN, cliFiles, load, runCli, sample, tempDir } from './helpers.mjs';
import { writeFileSync } from 'node:fs';
import path from 'node:path';

const fp = await load();

const I64_MIN = -(2n ** 63n);
const I64_MAX = 2n ** 63n - 1n;

/** The two-glyph font of crates/fontpixel/tests/render.rs::font(), as plain data. */
function renderFontDict() {
  const glyph = (cp, rows, advance) => ({
    codepoints: [{ Unicode: cp }],
    unicode_sequences: [],
    metrics: {
      bounds: { width: 3, height: 3, x: -1, y: -1 },
      advance: { x: advance, y: 0 },
      vertical_advance: { x: 0, y: 3 },
    },
    bitmap: fp.Bitmap.fromText(rows).toDict(),
    properties: [],
  });
  const dict = new fp.Font().toDict();
  dict.metrics.bounds = { width: 4, height: 3, x: -1, y: -1 };
  dict.glyphs = [glyph(65, '.#.\n###\n#.#', 3), glyph(66, '##.\n###\n##.', 2)];
  return dict;
}
const renderFont = () => fp.Font.fromDict(renderFontDict());
const hasLoss = (report) => report.entries.some((e) => e.impact === 'loss');
const isError = (Class, check) => (e) => {
  assert.ok(e instanceof Class, `expected ${Class.name}, got ${e?.constructor?.name}: ${e?.message}`);
  if (check) check(e);
  return true;
};

test('default advance, cell, ink and gaps', () => {
  const f = renderFont();
  const width = (text, options) => f.draw(text, options).bitmap.width;
  assert.equal(width('AB'), 7);
  assert.equal(width('AB', { mode: 'cell' }), 8);
  assert.equal(width('AB', { mode: 'ink' }), 6);
  assert.equal(width('AB', { mode: 'cell', letter_spacing: 2 }), 10);
  const r = f.draw('AB', { mode: 'cell', line_limit: 4, line_spacing: 2 });
  assert.equal(r.bitmap.width, 4);
  assert.equal(r.bitmap.height, 8);
  assert.equal(String(r.bitmap), '.#..\n###.\n#.#.\n....\n....\n##..\n###.\n##..');
  assert.equal(f.draw('AB', { mode: 'cell', line_limit: 4, line_spacing: -1 }).bitmap.height, 5);
  assert.equal(width('AB', { use_current_glyph_spacing: true }), 6);
  assert.deepEqual(r.report, { entries: [] });
  const dict = r.toDict();
  assert.equal(dict.bitmap.width, 4);
  assert.deepEqual(dict.report, { entries: [] });
  assert.deepEqual(fp.Bitmap.fromDict({ mono: dict.bitmap }).asBytes(), r.bitmap.asBytes());
  assert.match(String(r), /^Rendered\(bitmap=4x8, report_entries=0\)$/);
  // Each read of `bitmap` is a fresh copy.
  const copy = r.bitmap;
  copy.invert();
  assert.ok(!copy.equals(r.bitmap));
  // A Font view inside a FontSet renders the same.
  const set = new fp.FontSet();
  set.append(f);
  assert.ok(set.font(0).draw('AB').bitmap.equals(f.draw('AB').bitmap));
});

test('directions and missing glyphs are explicit', () => {
  for (const a of ['lr', 'rl', 'tb', 'bt']) {
    for (const b of ['lr', 'rl', 'tb', 'bt']) fp.parseWritingDirection(a + b);
  }
  assert.throws(() => fp.parseWritingDirection('garbage'), fp.InvalidInputError);
  assert.deepEqual(fp.writingDirectionFromSpec('garbage'), { glyphs: 'right', lines: 'down' });
  assert.deepEqual(fp.writingDirectionFromSpec('lrbt-suffix'), fp.parseWritingDirection('lrbt'));
  assert.deepEqual(fp.parseWritingDirection('tbrl'), { glyphs: 'down', lines: 'left' });
  assert.deepEqual(fp.parseWritingDirection('tb'), { glyphs: 'down', lines: 'left' });
  const f = renderFont();
  const r = f.draw('A??B');
  assert.ok(hasLoss(r.report));
  assert.equal(r.report.entries.length, 1); // a repeated missing code is deduplicated
  assert.throws(
    () => f.draw('A?', { strict: true }),
    isError(fp.StrictLossError, (e) => {
      assert.equal(e.kind, 'StrictLoss');
      assert.equal(e.exitCode, 3);
      assert.equal(e.report.entries[0].impact, 'loss');
    }),
  );
  assert.throws(() => f.draw('A?', { missing: 'error' }), fp.FontpixelError);
  const replacement = f.glyph(1); // B, a view
  assert.ok(f.draw('A?', undefined, replacement).bitmap.equals(f.draw('AB').bitmap));
  assert.ok(f.draw('A?', {}, replacement.toDict()).bitmap.equals(f.draw('AB').bitmap));
  assert.ok(f.draw('', undefined, replacement).bitmap.equals(new fp.Bitmap()));
  assert.equal(replacement.isView, true); // still usable: only a copy was taken
  assert.ok(replacement.bitmap.hasInk());
  assert.throws(() => f.draw('A', { missing: { glyph: {} } }), isError(fp.InvalidInputError, (e) => assert.match(e.message, /replacement glyph separately/)));
  assert.throws(() => f.draw('A', { not_an_option: 1 }), isError(fp.InvalidInputError, (e) => assert.match(e.message, /unknown field `not_an_option`/)));
  assert.throws(() => f.draw('A', undefined, new fp.Bitmap(1, 1)), fp.InvalidInputError);
  const rl = f.draw('AB', { direction: { glyphs: 'left', lines: 'down' } });
  assert.equal(rl.bitmap.width, 7);
  assert.ok(!rl.bitmap.equals(f.draw('AB').bitmap));
});

test('clipping is real ink loss', () => {
  // Glyph::shift(-2, 0): move glyph A's bounding box two pixels left.
  const shifted = renderFontDict();
  shifted.glyphs[0].metrics.bounds.x -= 2;
  const f = fp.Font.fromDict(shifted);
  const r = f.draw('A');
  assert.ok(r.report.entries.some((e) => e.code === 'render.cell_clipping'));
  assert.equal(String(r.bitmap), '....\n#...\n#...');
  const dict = f.toDict();
  dict.metrics.bounds = { width: 4, height: 3, x: I64_MAX, y: I64_MIN };
  assert.ok(!fp.Font.fromDict(dict).draw('A').bitmap.hasInk());
});

test('limits apply before rendering', () => {
  const f = renderFont();
  assert.throws(() => f.draw('AAA', { limits: { max_glyphs: 2 } }), fp.ResourceLimitError);
  assert.throws(() => f.draw('A', { limits: { max_layout_bytes: 1 } }), fp.ResourceLimitError);
  assert.throws(() => f.draw('A', { limits: { bitmap: { max_pixels: 1 } } }), fp.ResourceLimitError);
  assert.throws(() => f.draw('A', { letter_spacing: I64_MAX }), fp.FontpixelError);
  assert.throws(() => f.draw('AA', { letter_spacing: I64_MIN }), fp.FontpixelError);
  assert.throws(() => f.draw('A', { letter_spacing: 2n ** 63n }), fp.InvalidInputError);
  const empty = new fp.Font();
  assert.throws(() => empty.draw('AAAA', { limits: { max_glyphs: 2 } }), fp.FontpixelError);
  assert.ok(empty.drawAll('file', [], { limits: { max_glyphs: 2 } }).bitmap.equals(new fp.Bitmap()));
  assert.deepEqual(fp.defaultRenderLimits(), {
    max_glyphs: 1000000,
    max_layout_bytes: 134217728,
    max_report_bytes: 16777216,
    bitmap: fp.defaultBitmapLimits(),
  });
  const defaults = fp.defaultRenderOptions();
  assert.equal(defaults.line_limit, 512);
  assert.equal(defaults.mode, 'advance');
  assert.equal(defaults.missing, 'blank');
  assert.ok(f.draw('AB', defaults).bitmap.equals(f.draw('AB').bitmap));
});

test('gray conversion is explicit and reported', () => {
  const f = renderFont();
  f.glyph(0).bitmap = fp.Bitmap.fromRaw(3, 3, new Uint8Array([0, 127, 128, 255, 0, 254, 42, 200, 100]), { depth: 'gray8' });
  assert.throws(() => f.draw('A'), fp.FontpixelError);
  const r = f.draw('A', { gray_threshold: 128 });
  assert.ok(hasLoss(r.report));
  assert.equal(String(r.bitmap), '..#.\n#.#.\n.#..');
  assert.throws(() => f.draw('A', { gray_threshold: 128, strict: true }), fp.StrictLossError);
  assert.throws(() => f.draw('A', { gray_threshold: 256 }), fp.InvalidInputError);
});

test('unicode to legacy uses exact tables', () => {
  const dict = renderFontDict();
  dict.glyphs[0].codepoints = [{ Legacy: 0x80 }];
  dict.charset.name = 'cp1252';
  const f = fp.Font.fromDict(dict);
  assert.ok(f.draw('€').bitmap.equals(f.drawCodepoints([{ Legacy: 0x80 }]).bitmap));
  assert.ok(hasLoss(f.draw('人').report));
  dict.charset.name = 'unknown-test';
  const unknown = fp.Font.fromDict(dict);
  assert.ok(unknown.draw('B').report.entries.some((e) => e.code === 'render.unknown_charset'));
});

test('draw codepoints and draw all', () => {
  const f = renderFont();
  assert.ok(f.drawCodepoints([65, 'B']).bitmap.equals(f.draw('AB').bitmap));
  assert.ok(f.drawCodepoints([{ Unicode: 65 }, 66]).bitmap.equals(f.draw('AB').bitmap));
  assert.equal(f.drawCodepoints([65, null]).report.entries.length, 1);
  assert.equal(f.drawCodepoints([65, undefined]).report.entries.length, 1);
  assert.throws(() => f.drawCodepoints([65, 'AB']), fp.InvalidInputError);
  assert.throws(() => f.drawCodepoints('AB'), fp.InvalidInputError);
  assert.throws(() => f.drawCodepoints([-1]), fp.InvalidInputError);
  const cell = { mode: 'cell' };
  const asc = f.drawAll('ascending', undefined, cell);
  assert.ok(asc.bitmap.equals(f.draw('AB', cell).bitmap));
  assert.ok(f.drawAll(undefined, undefined, cell).bitmap.equals(asc.bitmap));
  const desc = f.drawAll('descending', undefined, cell);
  assert.ok(desc.bitmap.equals(f.draw('BA', cell).bitmap));
  assert.ok(f.drawAll('file', undefined, cell).bitmap.equals(asc.bitmap));
  assert.ok(f.drawAll('reverse_file', undefined, cell).bitmap.equals(desc.bitmap));
  const onlyB = f.drawAll(undefined, [[66, 66]], cell);
  assert.ok(onlyB.bitmap.equals(f.draw('B', cell).bitmap));
  assert.throws(() => f.drawAll('sideways'), fp.InvalidInputError);
  assert.throws(() => f.drawAll('file', [[66]]), fp.InvalidInputError);
});

test('diagnostic deduplication and report budget', () => {
  const empty = new fp.Font();
  const r = empty.drawCodepoints(Array.from({ length: 12000 }, (_, i) => i));
  assert.equal(r.report.entries.length, 12000);
  assert.ok(r.bitmap.equals(new fp.Bitmap()));
  assert.throws(
    () => empty.draw('?', { limits: { max_report_bytes: 1 } }),
    isError(fp.ResourceLimitError, (e) => assert.equal(e.resource, 'render diagnostic bytes')),
  );
  assert.ok(renderFont().draw('A', { limits: { max_report_bytes: 0 } }).bitmap.hasInk());
});

test('svg export and color validation', () => {
  assert.deepEqual(fp.parseRgba('#f08'), fp.rgba(255, 0, 136));
  assert.deepEqual(fp.parseRgba('#f08c'), fp.rgba(255, 0, 136, 204));
  assert.deepEqual(fp.parseRgba('#aabbccdd'), { red: 170, green: 187, blue: 204, alpha: 221 });
  assert.deepEqual(fp.parseRgba('transparent'), fp.rgbaTransparent());
  assert.deepEqual(fp.rgbaTransparent(), fp.rgba(0, 0, 0, 0));
  assert.deepEqual(fp.rgbaBlack(), fp.rgba(0, 0, 0));
  assert.deepEqual(fp.rgbaWhite(), fp.rgba(255, 255, 255));
  for (const bad of ['red', '#ff', '#zz0000', '#１２３', '"><script/>']) {
    assert.throws(() => fp.parseRgba(bad), fp.InvalidInputError);
  }
  assert.throws(() => fp.rgba(256, 0, 0), fp.InvalidInputError);
  const b = fp.Bitmap.fromText('#.#\n###');
  const svg = fp.toSvg(b, { scale: 4 });
  assert.match(svg, /width="12" height="8"/);
  assert.ok(svg.includes('M0 1h3v1h-3z'));
  assert.throws(() => fp.toSvg(b, { scale: 0 }), fp.FontpixelError);
  assert.throws(() => fp.toSvg(b, { limits: { max_bytes: 10 } }), fp.ResourceLimitError);
  assert.ok(fp.toSvg(new fp.Bitmap()).startsWith('<svg'));
  assert.throws(() => fp.toSvg(b.toGray8()), isError(fp.InvalidInputError, (e) => assert.match(e.message, /mono bitmap/)));
  assert.throws(() => fp.toSvg(b, { scale: 1, colour: 'red' }), fp.InvalidInputError);
  assert.deepEqual(fp.defaultImageOptions(), {
    scale: 1,
    foreground: fp.rgbaBlack(),
    background: fp.rgbaWhite(),
    limits: fp.defaultBitmapLimits(),
  });
});

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(bytes) {
  let c = 0xffffffff;
  for (const b of bytes) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

test('png round trips bits, dimensions, alpha and limits', () => {
  const b = fp.Bitmap.fromText('#.#\n###');
  const o = { scale: 3, foreground: fp.rgba(2, 4, 6, 128), background: fp.rgbaTransparent() };
  const data = fp.toPng(b, o);
  assert.ok(data instanceof Uint8Array);
  assert.deepEqual([...data.subarray(0, 8)], [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const view = new DataView(data.buffer, data.byteOffset, data.byteLength);
  let offset = 8;
  const idat = [];
  const kinds = [];
  while (offset < data.length) {
    const n = view.getUint32(offset);
    const kind = String.fromCharCode(...data.subarray(offset + 4, offset + 8));
    const body = data.subarray(offset + 8, offset + 8 + n);
    assert.equal(view.getUint32(offset + 8 + n), crc32(data.subarray(offset + 4, offset + 8 + n)), kind);
    kinds.push(kind);
    if (kind === 'IHDR') {
      const h = new DataView(body.buffer, body.byteOffset, body.byteLength);
      assert.deepEqual([h.getUint32(0), h.getUint32(4), body[8], body[9]], [9, 6, 1, 3]);
    } else if (kind === 'PLTE') {
      assert.deepEqual([...body], [0, 0, 0, 2, 4, 6]);
    } else if (kind === 'tRNS') {
      assert.deepEqual([...body], [0, 128]);
    } else if (kind === 'IDAT') {
      idat.push(body);
    }
    offset += n + 12;
  }
  assert.equal(kinds.at(-1), 'IEND');
  const decoded = inflateSync(Buffer.concat(idat));
  const expected = b.enlarge(3, 3).asBytes();
  for (let row = 0; row < 6; row++) {
    assert.equal(decoded[row * 3], 0); // filter type None
    assert.deepEqual([...decoded.subarray(row * 3 + 1, row * 3 + 3)], [...expected.subarray(row * 2, row * 2 + 2)]);
  }
  assert.throws(() => fp.toPng(new fp.Bitmap(), o), fp.FontpixelError);
  assert.throws(() => fp.toPng(b, { ...o, limits: { max_bytes: 10 } }), fp.ResourceLimitError);
  assert.throws(() => fp.toPng(b.toGray8()), fp.InvalidInputError);
  // Fresh output: mutating it does not affect the next call.
  data.fill(0);
  assert.deepEqual([...fp.toPng(b, o)].slice(0, 4), [0x89, 0x50, 0x4e, 0x47]);
});

test('preview and render PNG bytes equal the native CLI', { skip: !HAS_CLI && 'native CLI missing' }, () => {
  const font = sample(SPLEEN);
  const f = fp.read(font).fonts.font(0);
  const text = 'Hello, fontpixel!';
  // preview: `#`/`.` character art plus a final newline.
  const preview = cliFiles({ 'a.bdf': font }, ['preview', 'a.bdf', text]);
  assert.equal(preview.code, 0, preview.stderr);
  const art = f.draw(text).bitmap.toText([[1, '#'], [0, '.']]);
  assert.equal(new TextDecoder().decode(preview.stdout), `${art}\n`);
  // render to PNG with scale, colors and layout options.
  const cases = [
    { args: [text, '--scale', '3', '--color', '#2468ac80', '--background', 'transparent'], options: {}, image: { scale: 3, foreground: fp.parseRgba('#2468ac80'), background: fp.rgbaTransparent() } },
    { args: ['AB\nCD', '--direction', 'tbrl', '--letter-spacing', '2', '--mode', 'cell'], options: { direction: fp.parseWritingDirection('tbrl'), letter_spacing: 2, mode: 'cell' }, image: {} },
    { args: ['--all', '--range', 'U+0041-005A', '--order', 'descending', '--line-limit', '40'], all: { order: 'descending', ranges: [[0x41, 0x5a]] }, options: { mode: 'cell', line_limit: 40 }, image: {} },
  ];
  for (const { args, options, image, all } of cases) {
    const cli = cliFiles({ 'a.bdf': font }, ['render', 'a.bdf', ...args, '--output', 'out.png']);
    assert.equal(cli.code, 0, cli.stderr);
    const rendered = all ? f.drawAll(all.order, all.ranges, options) : f.draw(args[0], options);
    assert.deepEqual(fp.toPng(rendered.bitmap, image), cli.files['out.png'], args.join(' '));
  }
});

test('missing replacement and strict rendering match the CLI', { skip: !HAS_CLI && 'native CLI missing' }, () => {
  const font = sample(SPLEEN);
  const f = fp.read(font).fonts.font(0);
  const text = 'A一B';
  const cli = cliFiles({ 'a.bdf': font }, ['render', 'a.bdf', text, '--missing', 'U+003F', '--output', 'out.png']);
  assert.equal(cli.code, 0, cli.stderr);
  const question = f.glyphIds().map((id) => f.glyph(id)).find((g) => g.codepoints.some((c) => c.Unicode === 0x3f));
  assert.deepEqual(fp.toPng(f.draw(text, {}, question).bitmap), cli.files['out.png']);
  // Strict loss: the CLI exits 3 and writes nothing; the binding throws exit code 3.
  const strict = cliFiles({ 'a.bdf': font }, ['render', 'a.bdf', text, '--strict', '--output', 'out.png']);
  assert.equal(strict.code, 3);
  assert.equal(strict.files['out.png'], undefined);
  assert.throws(() => f.draw(text, { strict: true }), isError(fp.StrictLossError, (e) => assert.equal(e.exitCode, 3)));
  // Ensure the temp-dir helper is exercised through runCli as well.
  const { dir, cleanup } = tempDir();
  try {
    writeFileSync(path.join(dir, 'a.bdf'), font);
    const out = runCli(['preview', 'a.bdf', 'Hi', '--chars', '@ '], dir);
    assert.equal(new TextDecoder().decode(out.stdout), `${f.draw('Hi').bitmap.toText([[1, '@'], [0, ' ']])}\n`);
  } finally {
    cleanup();
  }
});
