// Model handles: FontSet / Font / Glyph views, copies, plain data, release.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { BLOCK, HAS_CLI, SPLEEN, cliFiles, load, sample, withoutSource } from './helpers.mjs';

const fp = await load();

test('edits through FontSet -> Font view -> Glyph view change the set', () => {
  const set = fp.read(sample(SPLEEN)).fonts;
  const font = set.font(0);
  assert.equal(font.isView, true);
  const [first] = font.glyphIds();
  const glyph = font.glyph(first);
  assert.equal(glyph.isView, true);
  const original = glyph.bitmap; // a copy
  const edited = new fp.Bitmap(original.width, original.height);
  edited.setPixel(0, 0, 1);
  glyph.bitmap = edited;
  // Every path to the glyph sees the edit; the earlier copy does not.
  assert.ok(set.font(0).glyph(first).bitmap.equals(edited));
  assert.ok(!original.equals(edited));
  const dict = set.toDict();
  assert.deepEqual(fp.Bitmap.fromDict(dict.fonts[0].glyphs[first].bitmap).asBytes(), edited.asBytes());
  // Reading a bitmap returns a copy: editing it does not change the glyph.
  const copy = glyph.bitmap;
  copy.setPixel(1, 0, 1);
  assert.ok(glyph.bitmap.equals(edited));
});

test('views fail with InvalidInputError once their target is gone', () => {
  const set = fp.read(sample(BLOCK)).fonts;
  const font = set.font(0);
  const id = font.glyphIds()[0];
  const glyph = font.glyph(id);
  const removed = font.removeGlyph(id);
  assert.equal(removed.isView, false);
  assert.equal(font.glyph(id), undefined);
  assert.throws(() => glyph.bitmap, (e) => e instanceof fp.InvalidInputError && /no longer exists/.test(e.message));
  const popped = set.pop(0);
  assert.equal(popped.isView, false);
  assert.throws(() => font.length, (e) => e instanceof fp.InvalidInputError && /font 0 no longer exists/.test(e.message));
  assert.throws(() => set.font(0), fp.InvalidInputError);
  assert.throws(() => set.font(1.5), /not an exact integer/);
  assert.throws(() => set.font(-1), fp.InvalidInputError);
});

test('copies are independent; adding a glyph copies it', () => {
  const font = fp.read(sample(BLOCK)).fonts.font(0);
  const owned = font.copy();
  assert.equal(owned.isView, false);
  const glyph = new fp.Glyph(fp.Bitmap.fromText('##\n#.'), { codepoints: [0x41, { Legacy: 7 }] });
  assert.deepEqual(glyph.codepoints, [{ Unicode: 0x41 }, { Legacy: 7 }]);
  const before = owned.length;
  const id = owned.addGlyph(glyph);
  assert.equal(owned.length, before + 1);
  assert.equal(font.length, before);
  glyph.bitmap = fp.Bitmap.fromText('..\n..');
  assert.equal(owned.glyph(id).bitmap.toText(), '##\n#.');
  // A view of the same font as argument: snapshotted before the edit.
  const again = owned.addGlyph(owned.glyph(id));
  assert.ok(owned.glyph(again).bitmap.equals(owned.glyph(id).bitmap));
  assert.throws(() => new fp.Glyph(fp.Bitmap.fromText('#'), { codepoints: [1], bogus: 1 }), /unknown option `bogus`/);
});

test('plain-data round trip keeps a property value beyond 2^53', () => {
  const set = fp.read(sample(SPLEEN)).fonts;
  const dict = set.toDict();
  dict.fonts[0].properties.push({ name: 'BIG_VALUE', value: { Unsigned: 2n ** 63n + 5n } });
  const rebuilt = fp.FontSet.fromDict(dict);
  const out = rebuilt.toDict();
  assert.deepEqual(out, dict);
  const prop = out.fonts[0].properties.find((p) => p.name === 'BIG_VALUE');
  assert.equal(prop.value.Unsigned, 2n ** 63n + 5n);
  assert.equal(typeof out.fonts[0].metrics.ascent, 'number');
});

test('model read in WASM equals the native CLI read of the same file', { skip: !HAS_CLI && 'native CLI not found' }, () => {
  // Round trip through the CLI: BDF -> YAFF (native), then read both in WASM.
  const result = cliFiles({ 'in.bdf': sample(SPLEEN) }, ['convert', 'in.bdf', '-o', 'out.yaff']);
  assert.equal(result.code, 0, result.stderr);
  const viaCli = fp.read(result.files['out.yaff'], { name: 'out.yaff' });
  assert.equal(viaCli.format, 'yaff');
  const direct = fp.read(sample(SPLEEN));
  const glyphsOf = (set) => withoutSource(set.font(0).toDict()).glyphs.map((g) => g && [g.codepoints, g.bitmap]);
  assert.deepEqual(glyphsOf(viaCli.fonts), glyphsOf(direct.fonts));
});

test('handle release: free(), Symbol.dispose, use after free, double free', () => {
  const bitmap = fp.Bitmap.fromText('#.');
  const other = bitmap.copy();
  bitmap.free();
  assert.throws(
    () => bitmap.width,
    (e) => e instanceof fp.InvalidInputError && /Bitmap handle was released/.test(e.message),
  );
  assert.throws(() => bitmap.getPixel(0, 0), fp.InvalidInputError);
  assert.throws(() => other.equals(bitmap), /Bitmap handle was released/);
  bitmap.free(); // idempotent
  assert.equal(other.width, 2); // the copy is independent
  // `using x = ...` calls [Symbol.dispose]() (= free()) at scope exit.
  const scoped = fp.Bitmap.fromText('#');
  assert.equal(scoped.width, 1);
  scoped[Symbol.dispose]();
  assert.throws(() => scoped.width, fp.InvalidInputError);
  const set = fp.read(sample(BLOCK)).fonts;
  const view = set.font(0);
  set.free();
  // A view keeps the shared storage alive (reference counted).
  assert.ok(view.length > 0);
  assert.equal(typeof fp.Font.prototype[Symbol.dispose], 'function');
});
