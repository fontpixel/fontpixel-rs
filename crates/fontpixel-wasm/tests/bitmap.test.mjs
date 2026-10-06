// Bitmap handle: construction, pixels, bytes, plain data.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { load } from './helpers.mjs';

const fp = await load();

test('bitmap round trip: text, bytes, plain data and pixels', () => {
  const text = '#..#\n.##.\n#..#';
  const b = fp.Bitmap.fromText(text);
  assert.equal(b.depth, 'mono');
  assert.equal(b.width, 4);
  assert.equal(b.height, 3);
  assert.equal(b.toText(), text);
  const bytes = b.asBytes();
  assert.deepEqual([...bytes], [0x90, 0x60, 0x90]);
  const fromBytes = fp.Bitmap.fromBytes(4, 3, bytes);
  assert.ok(fromBytes.equals(b));
  const fromDict = fp.Bitmap.fromDict(b.toDict());
  assert.ok(fromDict.equals(b));
  b.setPixel(1, 0, 1);
  assert.equal(b.getPixel(1, 0), 1);
  assert.ok(!fromDict.equals(b));
  const gray = new fp.Bitmap(2, 1, 'gray8');
  gray.setPixel(1, 0, 200);
  assert.deepEqual([...gray.asBytes()], [0, 200]);
  assert.deepEqual(gray.toDict(), { gray8: { width: 2, height: 1, data: [0, 200] } });
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
  assert.throws(() => new fp.Bitmap(1, 1, 'gray8').toText(), /mono/);
});
