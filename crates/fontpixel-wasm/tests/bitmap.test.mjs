// Bitmap handle: construction, pixels, bytes, plain data and every
// fontpixel::bitmap operation for both depths (ported from
// crates/fontpixel-py/tests/test_bitmap.py, which ports
// crates/fontpixel/tests/bitmap.rs).
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { load } from './helpers.mjs';

const fp = await load();

const I64_MIN = -(2n ** 63n);
const I64_MAX = 2n ** 63n - 1n;
const U32_MAX = 2 ** 32 - 1;
const SMALL = { max_dimension: 20, max_pixels: 20, max_bytes: 20 };

const mono = (text) => fp.Bitmap.fromText(text);
const gray = (rows) =>
  fp.Bitmap.fromRaw(rows.length ? rows[0].length : 0, rows.length, new Uint8Array(rows.flat()), { depth: 'gray8' });
const bytes = (b) => [...b.asBytes()];
const str = (b) => b.toString();
const isError = (Class, check) => (e) => {
  assert.ok(e instanceof Class, `expected ${Class.name}, got ${e?.constructor?.name}: ${e?.message}`);
  if (check) check(e);
  return true;
};

// A deterministic PRNG (mulberry32) for the randomized reference checks.
function rng(seed) {
  let a = seed >>> 0;
  const next = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  return { range: (lo, hi) => (hi === undefined ? Math.floor(next() * lo) : lo + Math.floor(next() * (hi - lo))), next };
}

test('bitmap round trip: text, bytes, plain data and pixels', () => {
  const text = '#..#\n.##.\n#..#';
  const b = fp.Bitmap.fromText(text);
  assert.equal(b.depth, 'mono');
  assert.equal(b.width, 4);
  assert.equal(b.height, 3);
  assert.equal(b.toText(), text);
  const raw = b.asBytes();
  assert.deepEqual([...raw], [0x90, 0x60, 0x90]);
  const fromBytes = fp.Bitmap.fromBytes(4, 3, raw);
  assert.ok(fromBytes.equals(b));
  const fromDict = fp.Bitmap.fromDict(b.toDict());
  assert.ok(fromDict.equals(b));
  b.setPixel(1, 0, 1);
  assert.equal(b.getPixel(1, 0), 1);
  assert.ok(!fromDict.equals(b));
  const g = new fp.Bitmap(2, 1, 'gray8');
  g.setPixel(1, 0, 200);
  assert.deepEqual([...g.asBytes()], [0, 200]);
  assert.deepEqual(g.toDict(), { gray8: { width: 2, height: 1, data: [0, 200] } });
});

test('bitmap errors: bounds, values, sizes, depths', () => {
  const b = new fp.Bitmap(2, 2);
  assert.throws(() => b.getPixel(2, 0), fp.InvalidInputError);
  assert.throws(() => b.setPixel(0, 0, 2), fp.InvalidInputError);
  assert.throws(() => b.setPixel(0, 0, 0.5), /not an exact integer/);
  assert.throws(() => b.setPixel(-1, 0, 1), /out of range/);
  assert.throws(() => fp.Bitmap.fromBytes(4, 3, new Uint8Array(2)), fp.InvalidInputError);
  assert.throws(() => new fp.Bitmap(1, 1, 'rgb'), /depth must be/);
  assert.throws(
    () => fp.Bitmap.fromBytes(4, 3, new Uint8Array(3), { limits: { max_pixels: 4 } }),
    (e) => e instanceof fp.ResourceLimitError || e instanceof fp.InvalidInputError,
  );
  assert.throws(() => new fp.Bitmap(1, 1, 'gray8').toText(), /explicit output palette/);
});

test('canonical storage, limits and invalid input', () => {
  const b = fp.Bitmap.fromRaw(9, 2, new Uint8Array([0xff, 0xff, 0x80, 0x7f]));
  assert.deepEqual(bytes(b), [0xff, 0x80, 0x80, 0]);
  assert.equal(b.stride, 2);
  assert.equal(b.depth, 'mono');
  assert.equal(b.packed, true);
  assert.equal(b.maxValue, 1);
  assert.throws(() => b.getPixel(9, 0), fp.InvalidInputError);
  assert.throws(() => b.getPixel(0, 2), fp.InvalidInputError);
  const c = b.copy();
  assert.throws(
    () => c.setPixel(0, 0, 2),
    isError(fp.InvalidInputError, (e) => assert.equal(e.message, 'pixel value is not representable in this bitmap format')),
  );
  assert.ok(b.equals(c));
  assert.throws(
    () => new fp.Bitmap(U32_MAX, 0),
    isError(fp.ResourceLimitError, (e) => {
      assert.equal(e.resource, 'bitmap dimension');
      assert.equal(e.limit, 1048576);
      assert.equal(e.kind, 'ResourceLimit');
      assert.equal(e.exitCode, 1);
    }),
  );
  assert.throws(() => new fp.Bitmap(U32_MAX, U32_MAX), fp.FontpixelError);
  // Integer parameters are exact: beyond u32, negative or fractional fails.
  assert.throws(() => new fp.Bitmap(2 ** 32, 1), isError(fp.InvalidInputError, (e) => assert.match(e.message, /out of range/)));
  assert.throws(() => new fp.Bitmap(1.5, 1), isError(fp.InvalidInputError));
  assert.throws(() => fp.Bitmap.fromRaw(9, 2, new Uint8Array([0])), fp.InvalidInputError);
  assert.throws(() => new fp.Bitmap(5, 5, 'mono', SMALL), fp.ResourceLimitError);
  assert.throws(() => fp.Bitmap.withLimits(4, 6, SMALL, 'gray8'), fp.ResourceLimitError);
  assert.equal(fp.Bitmap.withLimits(4, 5, SMALL, 'gray8').depth, 'gray8');
  const one = mono('#');
  for (const call of [
    () => one.crop(U32_MAX, 1, 0, 0),
    () => one.enlarge(0, 1),
    () => one.enlarge(U32_MAX, 2),
    () => one.rotate(45),
    () => one.italic(0, 0),
    () => one.bytePad(0),
    () => one.shadow(I64_MIN, 0, 1),
    () => one.shadow(0, I64_MAX, 1),
    () => one.bold(U32_MAX),
    () => one.extend(U32_MAX, 1, 0, 0),
    () => one.concat(mono('#'), { direction: 'right', alignment: 'start', gap: I64_MAX }),
    () => fp.Bitmap.concatAll([one], { direction: 'right', alignment: 'start', gaps: [0], limits: SMALL }),
  ]) {
    assert.throws(call, fp.FontpixelError);
  }
  // 64-bit parameters take number or bigint; out-of-range bigints fail.
  assert.throws(() => one.shift(2n ** 63n, 0), isError(fp.InvalidInputError, (e) => assert.match(e.message, /out of range/)));
  assert.throws(() => one.shift(2 ** 53, 0), isError(fp.InvalidInputError, (e) => assert.match(e.message, /exact integer/)));
  assert.throws(() => one.shift('1', 0), fp.InvalidInputError);
  assert.throws(() => new fp.Bitmap(1, 1, 'rgb'), fp.InvalidInputError);
  assert.throws(() => fp.Bitmap.fromRaw(1, 1, new Uint8Array(1), { bogus: 1 }), /unknown option `bogus`/);
});

test('buffers, rows and fresh pixel copies', () => {
  const data = new Uint8Array([0xa0, 0xe0]);
  const a = fp.Bitmap.fromRaw(3, 2, data);
  const b = fp.Bitmap.fromRaw(3, 2, data.buffer.slice(0));
  const c = fp.Bitmap.fromRaw(3, 2, new DataView(data.buffer));
  assert.ok(a.equals(b) && b.equals(c) && c.equals(mono('#.#\n###')));
  data[0] = 0; // the bitmap owns a copy
  assert.deepEqual(a.toRows().map((r) => [...r]), [[1, 0, 1], [1, 1, 1]]);
  assert.deepEqual([...a.toList()], [1, 0, 1, 1, 1, 1]);
  assert.ok(fp.Bitmap.fromRows(a.toRows()).equals(a));
  assert.ok(fp.Bitmap.fromRows([[1, 0, 1], [1, 1, 1]]).equals(a));
  // Every copy out is a fresh, independent Uint8Array.
  for (const out of [a.asBytes(), a.toList(), a.toRows()[0], a.toBytes()]) {
    assert.ok(out instanceof Uint8Array);
    out.fill(0);
  }
  assert.ok(a.equals(mono('#.#\n###')));
  assert.notEqual(a.asBytes().buffer, a.asBytes().buffer);
  const g = fp.Bitmap.fromRows([[0, 200], [255, 1]], { depth: 'gray8' });
  assert.equal(g.depth, 'gray8');
  assert.equal(g.packed, false);
  assert.equal(g.maxValue, 255);
  assert.equal(g.stride, 2);
  assert.deepEqual(bytes(g), [0, 200, 255, 1]);
  assert.throws(() => fp.Bitmap.fromRows([[1, 0], [1]]), fp.InvalidInputError);
  assert.throws(() => fp.Bitmap.fromRows([[2]]), fp.InvalidInputError);
  assert.throws(() => fp.Bitmap.fromRows([[256]], { depth: 'gray8' }), fp.InvalidInputError);
  assert.throws(() => fp.Bitmap.fromRaw(1, 1, 'x'), fp.InvalidInputError);
  assert.throws(() => fp.Bitmap.fromRaw(1, 1, [0]), fp.InvalidInputError);
  g.setPixel(1, 0, 7);
  assert.equal(g.getPixel(1, 0), 7);
  assert.deepEqual(g.toDict(), { gray8: { width: 2, height: 2, data: [0, 7, 255, 1] } });
  assert.ok(fp.Bitmap.fromDict(g.toDict()).equals(g));
  assert.deepEqual(a.toDict(), { mono: { width: 3, height: 2, data: [160, 224] } });
  assert.deepEqual(bytes(fp.Bitmap.fromDict({ mono: { width: 9, height: 1, data: [255, 255] } })), [0xff, 0x80]);
  assert.throws(() => fp.Bitmap.fromDict({ mono: { width: 9, height: 1, data: [255] } }), fp.InvalidInputError);
  assert.throws(() => fp.Bitmap.fromDict({ rgb: {} }), fp.InvalidInputError);
});

test('bitmap limits are inherited and checked', () => {
  const b = new fp.Bitmap(4, 4);
  assert.deepEqual(b.limits, fp.defaultBitmapLimits());
  assert.deepEqual(b.limits, { max_dimension: 1048576, max_pixels: 16777216, max_bytes: 67108864 });
  b.setLimits(SMALL);
  assert.deepEqual(b.limits, SMALL);
  assert.throws(() => b.enlarge(3, 3), isError(fp.ResourceLimitError, (e) => assert.equal(e.resource, 'bitmap pixels')));
  assert.throws(() => b.setLimits({ max_dimension: 2 }), fp.ResourceLimitError);
  assert.throws(() => b.setLimits({ max_size: 2 }), isError(fp.InvalidInputError, (e) => assert.match(e.message, /unknown field `max_size`/)));
  // Limits are inherited and not compared by equals.
  assert.deepEqual(b.crop(2, 2).limits, SMALL);
  assert.deepEqual(b.copy().limits, SMALL);
  assert.ok(b.equals(new fp.Bitmap(4, 4)));
});

test('zero dimensions are explicit', () => {
  for (const [w, h] of [[0, 0], [0, 4], [5, 0]]) {
    const b = new fp.Bitmap(w, h);
    assert.deepEqual([b.width, b.height], [w, h]);
    assert.equal(b.asBytes().length, 0);
    assert.deepEqual(b.bbox(), { width: 0, height: 0, x: 0, y: 0 });
    assert.ok(b.trim().equals(new fp.Bitmap()));
    assert.ok(b.rotate(90).rotate(-90).equals(b));
    assert.ok(b.flipHorizontal().flipHorizontal().equals(b));
    assert.ok(b.flipVertical().flipVertical().equals(b));
    assert.ok(b.toGray8().toMono(128).equals(b));
    assert.equal(b.toBytes().length, 0);
    assert.equal(b.toRows().length, h);
    assert.equal(b.toList().length, 0);
  }
  assert.ok(!new fp.Bitmap(1, 0).equals(new fp.Bitmap(2, 0)));
  const z = new fp.Bitmap(0, 3);
  assert.deepEqual(z.toNumericHexRows(), ['', '', '']);
  assert.deepEqual(z.toRowValues(), [0, 0, 0]);
  const a = mono('#');
  const b = fp.Bitmap.concatAll([z, a, z, a], { direction: 'down', alignment: 'start', gaps: [999, 999, 0] });
  assert.equal(str(b), '#\n#');
  // concatAll copies its inputs: the handles stay usable.
  assert.equal(str(a), '#');
  assert.equal(z.height, 3);
});

test('strings, palettes, hex rows and thresholds', () => {
  let b = fp.Bitmap.fromText(' \r\n.#\r\n10\n\n');
  assert.equal(str(b), '.#\n#.');
  for (const text of ['#\n##', '&', '# #', '#\n\n#']) {
    assert.throws(() => fp.Bitmap.fromText(text), fp.InvalidInputError);
  }
  assert.throws(() => fp.Bitmap.fromText('x', { palette: [['x', 1], ['x', 2]], depth: 'gray8' }), fp.InvalidInputError);
  assert.throws(() => fp.Bitmap.fromText('x', { depth: 'gray8' }), fp.InvalidInputError);
  assert.throws(() => fp.Bitmap.fromText('x', { palette: { xy: 1 } }), /exactly one character/);
  assert.equal(b.toText({ 0: '白', 1: '黑' }), '白黑\n黑白');
  assert.equal(b.toText(new Map([[0, '0'], [1, '1']])), '01\n10');
  assert.equal(b.toText([[0, '-'], [1, '@']]), '-@\n@-');
  assert.equal(b.toText(), '.#\n#.');
  assert.throws(() => b.toText({ 0: '.' }), fp.InvalidInputError);
  assert.throws(() => b.toText([[0, '.'], [0, '#']]), fp.InvalidInputError);
  assert.deepEqual(
    [...b.toPaletteBytes({ 0: new Uint8Array([255, 255, 255]), 1: [0, 0, 0] })],
    [255, 255, 255, 0, 0, 0, 0, 0, 0, 255, 255, 255],
  );
  for (const palette of [{ 0: [0], 1: [0, 0] }, { 0: [0] }, {}]) {
    assert.throws(() => b.toPaletteBytes(palette), fp.InvalidInputError);
  }
  b = mono('#...#....');
  assert.deepEqual(b.toHexRows(), ['8800']);
  assert.deepEqual(b.toNumericHexRows(), ['110']);
  assert.deepEqual(b.toRowValues(), [272]);
  assert.ok(fp.Bitmap.fromHexRows(9, ['887f']).equals(b));
  for (const rows of [['zz'], ['0'], ['0000']]) {
    assert.throws(() => fp.Bitmap.fromHexRows(8, rows), fp.InvalidInputError);
  }
  assert.throws(() => fp.Bitmap.fromHexRows(9, ['8800'], { limits: { max_dimension: 4 } }), fp.ResourceLimitError);
  assert.throws(() => new fp.Bitmap(65, 1).toRowValues(), fp.InvalidInputError);
  // u64 row values beyond 2^53 come back as bigints, per value.
  const wide = new fp.Bitmap(64, 2);
  for (let x = 0; x < 64; x++) wide.setPixel(x, 0, 1);
  wide.setPixel(63, 1, 1);
  assert.deepEqual(wide.toRowValues(), [2n ** 64n - 1n, 1]);
  const g = gray([[0, 1, 127, 128, 254, 255]]);
  assert.equal(str(g.toMono(128)), '...###');
  assert.equal(str(g.toMono(0)), '######');
  assert.equal(str(g.toMono(255)), '.....#');
  let inverse = g.copy();
  inverse.invert();
  assert.deepEqual(bytes(inverse), [255, 0, 0, 0, 0, 0]);
  inverse = g.copy();
  inverse.negate();
  assert.deepEqual(bytes(inverse), [255, 254, 128, 127, 1, 0]);
  b = mono('#.#');
  b.invert();
  assert.deepEqual(bytes(b), [0x40]);
  const lab = gray([[0, 3, 1]]);
  lab.invertWith(5);
  assert.deepEqual(bytes(lab), [5, 0, 0]);
  assert.throws(() => mono('#').invertWith(2), fp.InvalidInputError);
});

test('depth-specific methods and conversions', () => {
  const m = mono('#.');
  const g = gray([[0, 9]]);
  assert.deepEqual(bytes(m.toGray8()), [255, 0]);
  assert.deepEqual(bytes(m.toGray8Palette(10, 20)), [20, 10]);
  for (const call of [() => g.toGray8(), () => g.toGray8Palette(0, 1), () => g.toHexRows(), () => g.toNumericHexRows(), () => g.toRowValues()]) {
    assert.throws(call, isError(fp.InvalidInputError, (e) => assert.match(e.message, /only available for mono/)));
  }
  for (const call of [() => m.negate(), () => m.toMono(1)]) {
    assert.throws(call, isError(fp.InvalidInputError, (e) => assert.match(e.message, /only available for gray8/)));
  }
  assert.throws(() => m.paste(g), /different depths/);
  assert.throws(() => m.concat(g), /different depths/);
  assert.throws(() => fp.Bitmap.concatAll([m, g]), /different depths/);
  assert.throws(() => fp.Bitmap.concatAll([m, {}]), /element 1 is not a Bitmap/);
  assert.throws(() => fp.Bitmap.concatAll(m), /array of Bitmap handles/);
  assert.match(str(g), /gray8/);
  assert.ok(!mono('#').equals(mono('#').toGray8()));
});

test('checked geometry and label effects', () => {
  const b = mono('.....\n..##.\n...#.\n.....');
  assert.deepEqual(b.bbox(), { width: 2, height: 2, x: 2, y: 1 });
  assert.equal(str(b.trim()), '##\n.#');
  assert.equal(str(mono('#').crop(3, 3, -1, -1)), '...\n.#.\n...');
  assert.equal(str(mono('#').cropBounds({ width: 3, height: 3, x: -1, y: -1 })), '...\n.#.\n...');
  assert.equal(str(mono('##\n.#').shift(1, 1)), '..\n..');
  assert.ok(!b.crop(2, 2, I64_MIN, I64_MAX).hasInk());
  assert.ok(!b.shift(I64_MIN, I64_MIN).hasInk());
  assert.ok(b.hasInk());
  const c = b.copy();
  c.paste(b, I64_MAX, I64_MIN);
  assert.ok(b.equals(c));
  assert.equal(str(mono('#..\n.##').rotate(90)), '.#\n#.\n#.');
  assert.equal(str(mono('#.').enlarge(3, 2)), '###...\n###...');
  assert.equal(str(mono('#.').enlarge(2)), '##..\n##..');
  assert.equal(str(mono('#.#').bytePad(8)), '#.#.....');
  assert.equal(str(mono('#.').flipHorizontal()), '.#');
  assert.equal(str(mono('#\n.').flipVertical()), '.\n#');
  assert.equal(str(mono('#').extend(1, 2, 1, 0)), '....\n.#..');
  assert.equal(str(mono('#').extend()), '#');
  const labels = fp.Bitmap.fromText('.#.\n.&.', { palette: { '.': 0, '#': 1, '&': 2 }, depth: 'gray8' });
  assert.deepEqual(bytes(labels.shadow(1, -1, 2)), [0, 1, 0, 0, 0, 2, 2, 0, 0, 0, 2, 0]);
  assert.deepEqual(bytes(labels.glow(false, 1, 2)), [0, 0, 2, 0, 0, 0, 2, 1, 2, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]);
  assert.deepEqual(bytes(labels.bold(1)), [0, 1, 1, 0, 0, 2, 2, 0]);
  assert.ok(labels.bold().equals(labels.bold(1)));
  assert.equal(str(mono('#\n#\n#').italic(2, 1)), '.#\n.#\n#.');
  assert.equal(str(mono('#\n#').italic(I64_MAX, I64_MIN)), '#\n#');
  let p = gray([[1, 2, 1, 2], [2, 1, 2, 1]]);
  assert.deepEqual(bytes(p.replacePattern([1, 2], new Uint8Array([2, 1]))), [2, 1, 2, 1, 2, 2, 1, 1]);
  assert.throws(() => p.replacePattern([1, 2], [1]), fp.InvalidInputError);
  assert.equal(str(mono('#.').replacePattern(new Uint8Array(0), [1])), '###.#');
  p = mono('#.');
  p.replace(0, 1);
  assert.equal(str(p), '##');
  p.replace(1, 0);
  assert.equal(str(p), '..');
  // The receiver may also be the argument (copied before borrowing).
  const self = mono('#.');
  self.composite(self, 1, 0, 'or');
  assert.equal(str(self), '##');
});

function scalar(a, b, mode, mx) {
  switch (mode) {
    case 'over': return b === 0 ? a : b;
    case 'replace': return b;
    case 'or': return a | b;
    case 'and': return a & b;
    case 'xor': return a ^ b;
    case 'clear': return a & ~b & 0xff;
    case 'add': return Math.min(a + b, 255, mx);
    case 'subtract': return Math.max(a - b, 0);
    case 'max': return Math.max(a, b);
    case 'min': return Math.min(a, b);
    case 'multiply': return Math.floor((a * b + Math.floor(mx / 2)) / mx);
    case 'alpha_over': return a + b - Math.floor((a * b + Math.floor(mx / 2)) / mx);
    default: throw new Error(mode);
  }
}

test('composite matches the scalar reference for every mode and depth', () => {
  const modes = ['over', 'replace', 'or', 'and', 'xor', 'clear', 'add', 'subtract', 'max', 'min', 'multiply', 'alpha_over'];
  for (const depth of ['mono', 'gray8']) {
    const r = rng(0xcafe);
    const mx = depth === 'mono' ? 1 : 255;
    for (let i = 0; i < 150; i++) {
      const [w, h, sw, sh] = [r.range(20), r.range(6), r.range(20), r.range(6)];
      const a = Array.from({ length: h }, () => Array.from({ length: w }, () => r.range(mx + 1)));
      const s = Array.from({ length: sh }, () => Array.from({ length: sw }, () => r.range(mx + 1)));
      const [x, y] = [r.range(-10, 20), r.range(-4, 8)];
      const mode = modes[r.range(modes.length)];
      const target = h ? fp.Bitmap.fromRows(a, { depth }) : new fp.Bitmap(w, 0, depth);
      const source = sh ? fp.Bitmap.fromRows(s, { depth }) : new fp.Bitmap(sw, 0, depth);
      target.composite(source, x, BigInt(y), mode);
      const expected = a.map((row) => row.slice());
      for (let sy = 0; sy < sh; sy++) {
        for (let sx = 0; sx < sw; sx++) {
          const [tx, ty] = [sx + x, sy + y];
          if (tx >= 0 && tx < w && ty >= 0 && ty < h) expected[ty][tx] = scalar(a[ty][tx], s[sy][sx], mode, mx);
        }
      }
      assert.deepEqual(target.toRows().map((row) => [...row]), expected, JSON.stringify({ depth, w, h, sw, sh, x, y, mode }));
    }
  }
});

test('composite region, paste and overlay', () => {
  const source = mono('#..##.#..\n.#####.#.\n####..##.');
  for (let sx = -3; sx < 12; sx += 2) {
    for (let sy = -2; sy < 6; sy++) {
      for (let tx = -3; tx < 10; tx += 3) {
        for (let ty = -2; ty < 5; ty += 2) {
          const actual = new fp.Bitmap(12, 7);
          const expected = actual.copy();
          actual.compositeRegion(source, { width: 8, height: 4, x: sx, y: sy }, tx, ty, 'over');
          for (let y = 0; y < 4; y++) {
            for (let x = 0; x < 8; x++) {
              const [a, b, c, d] = [sx + x, sy + y, tx + x, ty + y];
              if (a >= 0 && a < 9 && b >= 0 && b < 3 && c >= 0 && c < 12 && d >= 0 && d < 7) {
                expected.setPixel(c, d, source.getPixel(a, b));
              }
            }
          }
          assert.ok(actual.equals(expected));
        }
      }
    }
  }
  const t = new fp.Bitmap(3, 1);
  t.overlay(mono('#.#'));
  assert.equal(str(t), '#.#');
  t.paste(mono('#'), 1, 0);
  assert.equal(str(t), '###');
  t.composite(mono('#'), 1, 0, 'clear');
  assert.equal(str(t), '#.#');
  t.composite(mono('#'), 1);
  assert.equal(str(t), '###');
  assert.throws(() => t.composite(mono('#'), 0, 0, 'blend'), isError(fp.InvalidInputError, (e) => assert.match(e.message, /unknown variant `blend`/)));
});

test('concat directions and alignment', () => {
  const a = mono('#\n#');
  const b = mono('##');
  assert.ok(a.concat(b).equals(a.concat(b, { direction: 'right', alignment: 'start', gap: 0 })));
  assert.equal(str(a.concat(b, { direction: 'right', alignment: 'start', gap: 0 })), '###\n#..');
  assert.equal(str(a.concat(b, { direction: 'right', alignment: 'end' })), '#..\n###');
  assert.equal(str(a.concat(b, { gap: 1 })), '#.##\n#...');
  assert.equal(str(a.concat(b, { gap: 1n })), '#.##\n#...');
  assert.equal(str(a.concat(b, { direction: 'left' })), '###\n..#');
  assert.equal(str(a.concat(b, { direction: 'down' })), '#.\n#.\n##');
  assert.equal(str(a.concat(b, { direction: 'up', alignment: 'end' })), '##\n.#\n.#');
  assert.equal(str(fp.Bitmap.concatAll([a, b, a], { direction: 'right' })), '####\n#..#');
  assert.equal(str(fp.Bitmap.concatAll([a, b, a])), '####\n#..#');
  assert.throws(() => fp.Bitmap.concatAll([a, b], { gaps: [0, 0] }), fp.InvalidInputError);
  assert.throws(() => a.concat(b, { direction: 'sideways' }), fp.InvalidInputError);
  assert.throws(() => a.concat(b, { spacing: 1 }), /unknown option `spacing`/);
});

test('all wire layouts round trip; known vectors', () => {
  const r = rng(99);
  for (const w of [0, 1, 7, 8, 9, 15, 16, 17, 33, 65]) {
    const b = new fp.Bitmap(w, 5);
    for (let y = 0; y < 5; y++) for (let x = 0; x < w; x++) b.setPixel(x, y, r.range(2));
    for (const bit_order of ['msb', 'lsb']) {
      for (const byte_order of ['big', 'little']) {
        for (const row_order of ['top-down', 'bottom-up']) {
          for (const row_alignment of [1, 2, 4, 8]) {
            for (const scan_unit of [1, 2, 4, 8]) {
              const layout = { bit_order, byte_order, row_order, row_alignment, scan_unit };
              const invalid = (bit_order === 'msb') !== (byte_order === 'big') && scan_unit > row_alignment;
              if (invalid) {
                assert.throws(() => b.toBytes(layout), fp.InvalidInputError);
                continue;
              }
              const data = b.toBytes(layout);
              assert.ok(fp.Bitmap.fromBytes(w, 5, data, { layout }).equals(b));
              if (data.length) assert.throws(() => fp.Bitmap.fromBytes(w, 5, data.subarray(0, -1), { layout }), fp.InvalidInputError);
            }
          }
        }
      }
    }
  }
  const b = mono('#........#......');
  const layout = { byte_order: 'little', scan_unit: 2, row_alignment: 2 };
  assert.deepEqual([...b.toBytes(layout)], [0x40, 0x80]);
  assert.deepEqual([...b.toBytes({ ...layout, bit_order: 'lsb_first' })], [0x01, 0x02]);
  const g = gray([[1, 2, 3], [4, 5, 6]]);
  const gl = { row_alignment: 4, row_order: 'bottom-up' };
  assert.deepEqual([...g.toBytes(gl)], [4, 5, 6, 0, 1, 2, 3, 0]);
  assert.ok(fp.Bitmap.fromBytes(3, 2, g.toBytes(gl), { layout: gl, depth: 'gray8' }).equals(g));
  assert.throws(() => g.toBytes({ bit_order: 'lsb' }), fp.InvalidInputError);
  assert.throws(() => g.toBytes({ bit_order: 'msb', extra: 1 }), fp.InvalidInputError);
  assert.deepEqual(fp.defaultByteLayout(), {
    bit_order: 'msb',
    byte_order: 'big',
    row_alignment: 1,
    scan_unit: 1,
    row_order: 'top-down',
  });
});

test('copies, equality, released handles and toString', () => {
  const b = mono('#.\n.#');
  const c = b.copy();
  c.setPixel(0, 0, 0);
  assert.equal(b.getPixel(0, 0), 1);
  assert.ok(!c.equals(b));
  assert.equal(String(b), '#.\n.#');
  assert.equal(String(new fp.Bitmap(2, 2, 'gray8')), "Bitmap(width=2, height=2, depth='gray8')");
  const released = mono('#');
  released.free();
  released.free(); // idempotent
  assert.throws(() => released.width, isError(fp.InvalidInputError, (e) => assert.match(e.message, /released/)));
  assert.throws(() => b.paste(released), isError(fp.InvalidInputError, (e) => assert.match(e.message, /released/)));
  assert.throws(() => fp.Bitmap.concatAll([b, released]), isError(fp.InvalidInputError, (e) => assert.match(e.message, /released/)));
  // Glyph bitmaps are copies; GlyphBitmap::crop is cropBounds on the variant.
  const glyph = new fp.Glyph(mono('#.'));
  const copy = glyph.bitmap;
  copy.setPixel(1, 0, 1);
  assert.equal(str(glyph.bitmap), '#.');
  assert.equal(str(glyph.bitmap.cropBounds({ width: 1, height: 1, x: 0, y: 0 })), '#');
});
