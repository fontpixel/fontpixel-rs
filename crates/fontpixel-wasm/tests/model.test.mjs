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
  // Round trip through the CLI: BDF -> PCF (native; PCF is in every
  // profile), then read both in WASM.
  const result = cliFiles({ 'in.bdf': sample(SPLEEN) }, ['convert', 'in.bdf', '-o', 'out.pcf']);
  assert.equal(result.code, 0, result.stderr);
  const viaCli = fp.read(result.files['out.pcf'], { name: 'out.pcf' });
  assert.equal(viaCli.format, 'pcf');
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

// ---------------------------------------------------------------------------
// Complete model API (mirrors crates/fontpixel-py/tests/test_model.py).

const mono = (text) => fp.Bitmap.fromText(text);
const glyph = (text, codepoints = [], name) => new fp.Glyph(mono(text), { codepoints, name });
const invalid = (pattern) => (e) => e instanceof fp.InvalidInputError && e.kind === 'InvalidInput' && pattern.test(e.message);

test('Codepoint class: kinds, ordering, plain data and CodepointLike arguments', () => {
  const a = fp.Codepoint.unicode(0x41);
  assert.ok(a.equals(new fp.Codepoint(0x41)) && a.equals(new fp.Codepoint(0x41, 'unicode')));
  assert.equal(a.value, 0x41);
  assert.equal(a.kind, 'unicode');
  assert.equal(a.isUnicode, true);
  assert.equal(a.scalar(), 'A');
  const legacy = fp.Codepoint.legacy(0x80);
  assert.equal(legacy.kind, 'legacy');
  assert.equal(legacy.isUnicode, false);
  assert.equal(legacy.scalar(), undefined);
  assert.equal(fp.Codepoint.unicode(0xd800).scalar(), undefined);
  assert.equal(fp.Codepoint.unicode(0x110000).scalar(), undefined);
  assert.ok(!a.equals(fp.Codepoint.legacy(0x41)));
  // Every Unicode code precedes every legacy code (the Rust Ord).
  const sorted = [legacy, a, fp.Codepoint.unicode(0x20)].sort((x, y) => x.compare(y));
  assert.deepEqual(sorted.map(String), ['Codepoint.unicode(0x0020)', 'Codepoint.unicode(0x0041)', 'Codepoint.legacy(0x0080)']);
  assert.equal(fp.Codepoint.unicode(0xffffffff).compare(fp.Codepoint.legacy(0)), -1);
  assert.deepEqual(a.toDict(), { Unicode: 65 });
  assert.deepEqual(legacy.toDict(), { Legacy: 128 });
  assert.ok(fp.Codepoint.fromDict({ Legacy: 128 }).equals(legacy));
  // Accepted spellings: number and one-character string (Unicode), class, plain data.
  for (const like of [65, 'A', a, { Unicode: 65 }]) assert.ok(a.equals(like), String(like));
  assert.ok(fp.Codepoint.fromDict('😀').equals(0x1f600));
  assert.equal(a.equals('AB'), false);
  assert.equal(a.toString(), 'Codepoint.unicode(0x0041)');
  assert.throws(() => new fp.Codepoint(1, 'other'), invalid(/unicode.*legacy/));
  assert.throws(() => new fp.Codepoint(-1), invalid(/out of range/));
  assert.throws(() => new fp.Codepoint(2 ** 32), invalid(/out of range/));
  assert.throws(() => new fp.Codepoint(1.5), invalid(/not an exact integer/));
  assert.throws(() => fp.Codepoint.fromDict({ Unicode: 1, Legacy: 2 }), fp.InvalidInputError);
  assert.throws(() => fp.Codepoint.fromDict(true), invalid(/boolean/));
  assert.equal(fp.Codepoint.unicode(0xffffffff).value, 0xffffffff);
});

test('Font lookup: glyph IDs, views, codepoint maps and iteration', () => {
  const font = new fp.Font([glyph('#', ['A'], 'A'), glyph('##', [0x42])]);
  assert.equal(font.length, 2);
  assert.equal(font.slotCount, 2);
  assert.equal(font.isEmpty(), false);
  assert.equal(new fp.Font().isEmpty(), true);
  assert.deepEqual(font.glyphIds(), [0, 1]);
  assert.equal(font.glyphId('A'), 0);
  assert.equal(font.glyphId(fp.Codepoint.unicode(66)), 1);
  assert.equal(font.glyphId({ Unicode: 67 }), undefined);
  const g = font.glyphForCodepoint(66);
  assert.ok(g.isView && g.id === 1 && g.bitmap.width === 2);
  assert.equal(font.glyphForCodepoint('Z'), undefined);
  assert.equal(font.glyph(0).name, 'A');
  assert.equal(font.glyph(7), undefined);
  assert.deepEqual([...font].map((v) => v.id), [0, 1]);
  assert.deepEqual(font.glyphs().map(([id, v]) => [id, v.name]), [[0, 'A'], [1, undefined]]);
  assert.deepEqual(font.codepoints(), [[{ Unicode: 65 }, 0], [{ Unicode: 66 }, 1]]);
  assert.throws(() => font.glyphId(true), invalid(/boolean/));
  assert.throws(() => font.glyphId('AB'), invalid(/exactly one character/));
  assert.throws(() => font.glyph(-1), invalid(/out of range/));
  assert.throws(() => font.glyph(0.5), invalid(/not an exact integer/));
  assert.equal(font.toString(), 'Font(glyphs=2, family=null, view=false)');
});

test('duplicate codes, inclusive codepoint ranges and Unicode sequences', () => {
  const font = new fp.Font();
  const ids = [font.addGlyph(glyph('#', [65])), font.addGlyph(glyph('##', [65, fp.Codepoint.legacy(3)])), font.addGlyph(glyph('###', [90]))];
  assert.deepEqual(ids, [0, 1, 2]);
  // The later glyph wins; both IDs stay listed in file order.
  assert.equal(font.glyphId(65), 1);
  assert.deepEqual(font.glyphIdsForCodepoint(65), [0, 1]);
  assert.deepEqual(font.glyphIdsForCodepoint(66), []);
  assert.deepEqual(font.codepointRange(65, 80), [[{ Unicode: 65 }, 1]]);
  assert.deepEqual(font.codepointRange(66, 65), []);
  const everything = font.codepointRange(new fp.Codepoint(0), fp.Codepoint.legacy(0xffffffff));
  assert.deepEqual(everything.map(([cp]) => cp), [{ Unicode: 65 }, { Unicode: 90 }, { Legacy: 3 }]);
  const fi = glyph('#', [], 'fi');
  fi.unicodeSequences = [[0x66, 0x69]];
  assert.deepEqual(fi.unicodeSequences, [[0x66, 0x69]]);
  const seq = new fp.Font([fi]);
  assert.equal(seq.glyphForSequence('fi').name, 'fi');
  assert.equal(seq.glyphForSequence([0x66, 0x69]).id, 0);
  assert.equal(seq.glyphForSequence('ff'), undefined);
  fi.unicodeSequences = [[]];
  assert.throws(() => fi.validate(), invalid(/empty Unicode glyph sequence/));
  assert.throws(() => seq.addGlyph(fi), invalid(/empty Unicode glyph sequence/));
  fi.unicodeSequences = [[0x110000]];
  assert.throws(() => fi.validate(), invalid(/exceeds U\+10FFFF/));
  assert.throws(() => seq.glyphForSequence([1.5]), invalid(/not an exact integer/));
});

test('replaceGlyph, removeGlyph and stable IDs that are never reused', () => {
  const font = new fp.Font([glyph('#', [65]), glyph('##', [66])]);
  const old = font.replaceGlyph(0, glyph('###', [67]));
  assert.equal(old.isView, false);
  assert.deepEqual(old.codepoints, [{ Unicode: 65 }]);
  assert.equal(font.glyphId(65), undefined);
  assert.equal(font.glyphId(67), 0);
  const removed = font.removeGlyph(1);
  assert.equal(removed.bitmap.width, 2);
  assert.equal(font.removeGlyph(1), undefined);
  assert.equal(font.length, 1);
  assert.equal(font.slotCount, 2);
  assert.equal(font.addGlyph(glyph('#', [68])), 2);
  assert.throws(() => font.replaceGlyph(1, glyph('#')), invalid(/unknown or removed glyph ID/));
  // Plain data is accepted wherever a glyph is.
  assert.equal(font.addGlyph(glyph('#', [69]).toDict()), 3);
  assert.equal(font.replaceGlyph(3, { ...glyph('##', [70]).toDict(), name: 'plain' }).name, undefined);
  assert.equal(font.glyph(3).name, 'plain');
  assert.throws(() => font.addGlyph({ ...glyph('#').toDict(), bogus: 1 }), invalid(/unknown field `bogus`/));
  assert.throws(() => font.addGlyph(fp.Bitmap.fromText('#')), invalid(/expected a Glyph or its plain data, got a Bitmap/));
});

test('editGlyph: owned copy, no borrow held, transactional on throw', () => {
  const set = new fp.FontSet([new fp.Font([glyph('#', [65]), glyph('##', [66])])]);
  const font = set.font(0);
  let seen;
  font.editGlyph(0, (g) => {
    assert.equal(g.isView, false);
    // The font is not borrowed while the callback runs.
    seen = [font.length, set.toDict().fonts.length, font.glyph(0).name];
    g.name = 'edited';
    g.shift(2, -1);
  });
  assert.deepEqual(seen, [2, 1, undefined]);
  assert.equal(font.glyph(0).name, 'edited');
  assert.deepEqual(font.glyph(0).metrics.bounds, { width: 1, height: 1, x: 2, y: -1 });
  const stop = new RangeError('stop');
  assert.throws(
    () =>
      font.editGlyph(0, (g) => {
        g.name = 'lost';
        throw stop;
      }),
    (e) => e === stop,
  );
  assert.equal(font.glyph(0).name, 'edited');
  // An edit that fails validation changes nothing either.
  assert.throws(
    () =>
      font.editGlyph(0, (g) => {
        g.unicodeSequences = [[]];
      }),
    invalid(/empty Unicode glyph sequence/),
  );
  assert.deepEqual(font.glyph(0).unicodeSequences, []);
  // The callback may remove the glyph: committing then fails explicitly.
  assert.throws(() => font.editGlyph(1, () => font.removeGlyph(1)), invalid(/unknown or removed glyph ID/));
  assert.throws(() => font.editGlyph(1, () => {}), invalid(/unknown or removed glyph ID/));
  // Codes edited in the callback re-index the font.
  font.editGlyph(0, (g) => {
    g.codepoints = [0x5a];
  });
  assert.equal(font.glyphId('Z'), 0);
  assert.equal(font.glyphId('A'), undefined);
});

test('glyph views edit in place: bitmap keeps its offset, codepoints re-index, copy detaches', () => {
  const set = new fp.FontSet([new fp.Font([glyph('#', [65])])]);
  const g = set.font(0).glyph(0);
  g.shift(1, 2);
  g.bitmap = mono('##\n##');
  assert.equal(set.fonts[0].glyph(0).bitmap.toText(), '##\n##');
  assert.deepEqual(set.font(0).glyph(0).metrics.bounds, { width: 2, height: 2, x: 1, y: 2 });
  g.bitmap = { mono: { width: 1, height: 1, data: [0x80] } };
  assert.deepEqual(g.metrics.bounds, { width: 1, height: 1, x: 1, y: 2 });
  g.codepoints = ['B'];
  assert.equal(set.font(0).glyphId('B'), 0);
  assert.equal(set.font(0).glyphId('A'), undefined);
  const c = g.copy();
  c.name = 'copy';
  assert.equal(g.name, undefined);
  assert.equal(c.id, undefined);
  assert.equal(g.id, 0);
  set.font(0).removeGlyph(0);
  assert.throws(() => g.name, invalid(/glyph 0 no longer exists/));
  assert.equal(c.name, 'copy');
});

test('Glyph setBitmap, bitmapInCell and shift with 64-bit offsets (number or bigint)', () => {
  const g = new fp.Glyph();
  assert.equal(g.bitmap.width, 0);
  assert.equal(g.id, undefined);
  g.setBitmap(mono('...\n.#.\n...'), 1, 2, true);
  assert.deepEqual(g.metrics.bounds, { width: 1, height: 1, x: 2, y: 3 });
  const cell = g.bitmapInCell({ width: 4, height: 4, x: 0, y: 0 });
  assert.equal(cell.toText(), '..#.\n....\n....\n....');
  g.setBitmap(mono('##'), -1);
  assert.deepEqual(g.metrics.bounds, { width: 2, height: 1, x: -1, y: 0 });
  // Offsets beyond 2^53 are exact as bigints, and come back as bigints.
  const far = 2n ** 60n;
  g.setBitmap(mono('#'), far, -far);
  assert.deepEqual(g.metrics.bounds, { width: 1, height: 1, x: far, y: -far });
  assert.equal(typeof g.metrics.bounds.x, 'bigint');
  g.shift(-far, far);
  assert.deepEqual(g.metrics.bounds, { width: 1, height: 1, x: 0, y: 0 });
  assert.equal(typeof g.metrics.bounds.x, 'number');
  // i64::MIN is exact; one more step overflows (checked, never wrapped).
  g.shift(-(2n ** 63n), 0);
  assert.equal(g.metrics.bounds.x, -(2n ** 63n));
  assert.throws(() => g.shift(-1, 0), invalid(/overflows/));
  assert.equal(g.metrics.bounds.x, -(2n ** 63n));
  g.shift(2n ** 63n - 1n, 0);
  g.shift(1, 0);
  assert.throws(() => g.shift(2n ** 63n, 0), invalid(/out of range/));
  assert.throws(() => g.shift(2 ** 60, 0), invalid(/not an exact integer/));
  assert.throws(() => g.setBitmap(mono('#'), 2n ** 63n - 1n), invalid(/overflows/));
  g.validate();
  // A trimmed empty bitmap gets a zero box.
  g.setBitmap(mono('..\n..'), 5, 5, true);
  assert.deepEqual(g.metrics.bounds, { width: 0, height: 0, x: 0, y: 0 });
  assert.throws(() => g.bitmapInCell({ width: 1, height: 1 }), fp.InvalidInputError);
});

test('glyph metric plain data: vectors, rationals, ABC widths and spacing are validated', () => {
  const g = glyph('##', [65]);
  const m = g.metrics;
  assert.deepEqual(m, { bounds: { width: 2, height: 1, x: 0, y: 0 } });
  Object.assign(m, {
    advance: { x: 3, y: 0 },
    scalable_advance: { x: 500, y: 0 },
    abc: { a: 0, b: 2, c: 1 },
    left_bearing: 0,
    image_offset: { x: 1, y: -1 },
    vertical_advance: { x: 0, y: 2n ** 62n },
  });
  g.metrics = m;
  assert.deepEqual(g.metrics, m);
  assert.equal(typeof g.metrics.vertical_advance.y, 'bigint');
  assert.throws(() => (g.metrics = { ...m, abc: { a: 0, b: 2 } }), invalid(/missing field `c`/));
  assert.throws(() => (g.metrics = { ...m, advance: { x: 1, y: 0, z: 0 } }), invalid(/unknown field `z`/));
  assert.throws(() => (g.metrics = { ...m, advance: { x: 2 ** 60, y: 0 } }), invalid(/floating point/));
  // Bounds must agree with the bitmap; a failed assignment changes nothing.
  assert.throws(() => (g.metrics = { ...m, bounds: { ...m.bounds, width: 5 } }), invalid(/BBX dimensions differ/));
  assert.equal(g.metrics.bounds.width, 2);
  const font = new fp.Font([g]);
  const fm = font.metrics;
  Object.assign(fm, {
    point_size: { numerator: 21, denominator: 2 },
    resolution: { x: 75, y: 75 },
    spacing: 'Monospaced',
    ascent: 7,
    descent: 1,
  });
  font.metrics = fm;
  assert.deepEqual(font.metrics.point_size, { numerator: 21, denominator: 2 });
  assert.equal(font.metrics.spacing, 'Monospaced');
  assert.throws(() => (font.metrics = { ...fm, point_size: { numerator: 1, denominator: 0 } }), invalid(/denominator must be nonzero/));
  assert.throws(() => (font.metrics = { ...fm, spacing: 'monospaced' }), invalid(/unknown variant/));
  assert.throws(() => (font.metrics = { ...fm, point_size: { numerator: 1, denominator: -1 } }), fp.InvalidInputError);
  // Rational numerators are i64: beyond 2^53 as bigint.
  font.metrics = { ...fm, point_size: { numerator: -(2n ** 63n), denominator: 4294967295 } };
  assert.deepEqual(font.metrics.point_size, { numerator: -(2n ** 63n), denominator: 4294967295 });
  assert.ok(fp.Font.fromDict(font.toDict()).equals(font));
});

test('font metrics, names, charset, default glyph, glyphInCell and updateBounds', () => {
  const font = new fp.Font([glyph('#\n#', [65]), glyph('.#', [66])]);
  font.updateBounds();
  assert.deepEqual(font.metrics.bounds, { width: 2, height: 2, x: 0, y: 0 });
  const names = font.names;
  names.family = 'Demo';
  font.names = names;
  assert.equal(font.names.family, 'Demo');
  assert.throws(() => (font.names = { family: 'x', nope: 1 }), invalid(/unknown field `nope`/));
  font.charset = { registry: 'ISO8859', encoding: '1', code_ranges: [] };
  assert.equal(font.charsetCanonicalName(), 'iso88591');
  assert.deepEqual(font.charset, { registry: 'ISO8859', encoding: '1', code_ranges: [] });
  font.charset = { name: 'cp1252', code_ranges: [[0x20, 0x7e]] };
  assert.equal(font.charsetCanonicalName(), 'cp1252');
  assert.throws(() => (font.charset = { bogus: 1 }), fp.InvalidInputError);
  const metrics = font.metrics;
  metrics.default_glyph = 1;
  font.metrics = metrics;
  assert.throws(() => (font.metrics = { ...metrics, default_glyph: 9 }), invalid(/default glyph refers to a removed or nonexistent glyph/));
  font.removeGlyph(1);
  assert.ok(!('default_glyph' in font.metrics));
  assert.equal(font.glyphInCell(0).toText(), '#.\n#.');
  assert.throws(() => font.glyphInCell(1), invalid(/unknown or removed glyph ID/));
  assert.throws(() => new fp.Font().glyphInCell(0), invalid(/font has no cell bounds/));
  font.sourceFormat = 'bdf';
  assert.equal(font.sourceFormat, 'bdf');
  font.sourceFormat = null;
  assert.equal(font.sourceFormat, undefined);
  assert.equal(font.toString(), 'Font(glyphs=1, family="Demo", view=false)');
});

test('validate and retained/cloned usage: inclusive budgets, usize arguments', () => {
  const font = new fp.Font([glyph('##\n##', [65]), glyph('#', [66])]);
  font.validate();
  assert.throws(
    () => font.validate({ max_glyphs: 1 }),
    (e) => e instanceof fp.ResourceLimitError && e.kind === 'ResourceLimit' && e.resource === 'glyph count' && e.limit === 1 && e.exitCode === 1,
  );
  assert.throws(() => font.validate({ max_pixels_per_glyph: 3 }), (e) => e.resource === 'glyph pixels' && e.limit === 3);
  assert.throws(() => font.validate({ unknown_limit: 1 }), invalid(/unknown field `unknown_limit`/));
  // usize is 32-bit on wasm32: a limit beyond 2^32 - 1 (or 2^53) is rejected, never truncated.
  assert.throws(() => font.validate({ max_glyphs: 2 ** 32 }), fp.InvalidInputError);
  assert.throws(() => font.validate({ max_glyphs: 2n ** 60n }), fp.InvalidInputError);
  font.validate({ max_glyphs: 2n ** 32n - 1n });
  const usage = font.retainedUsage();
  assert.deepEqual(Object.keys(usage).sort(), ['bytes', 'fonts', 'glyph_slots', 'glyphs']);
  assert.equal(usage.fonts, 1);
  assert.equal(usage.glyphs, 2);
  assert.equal(usage.glyph_slots, 2);
  assert.ok(usage.bytes > 0);
  const cloned = font.clonedUsage();
  assert.equal(cloned.glyphs, 2);
  assert.ok(cloned.bytes > 0 && cloned.bytes <= usage.bytes);
  // The budget is inclusive: exactly the retained size passes, one less fails.
  assert.deepEqual(font.retainedUsage(usage.bytes), usage);
  assert.throws(
    () => font.retainedUsage(usage.bytes - 1),
    (e) => e instanceof fp.ResourceLimitError && e.resource === 'retained model bytes' && e.limit === usage.bytes - 1,
  );
  assert.deepEqual(font.clonedUsage(cloned.bytes), cloned);
  assert.throws(() => font.clonedUsage(cloned.bytes - 1), fp.ResourceLimitError);
  assert.throws(() => font.retainedUsage(2 ** 32), invalid(/out of range/));
  assert.throws(() => font.retainedUsage(-1), invalid(/out of range/));
  // Removed slots stay charged; a set charges its strikes plus its own header.
  font.removeGlyph(1);
  const after = font.retainedUsage();
  assert.equal(after.glyphs, 1);
  assert.equal(after.glyph_slots, 2);
  const set = new fp.FontSet([font]);
  assert.ok(set.retainedUsage().bytes > set.font(0).retainedUsage().bytes);
  assert.throws(
    () => new fp.Font([glyph('#'.repeat(9))]).validate({ max_total_bytes: 1 }),
    (e) => e instanceof fp.ResourceLimitError && e.resource === 'model bitmap bytes' && e.limit === 1,
  );
});

test('plain-data round trips keep removed slots and stable IDs; equals compares contents', () => {
  const font = new fp.Font([glyph('#', [65]), glyph('##', [66])]);
  font.removeGlyph(0);
  const data = font.toDict();
  assert.equal(data.glyphs[0], null);
  const again = fp.Font.fromDict(data);
  assert.ok(again.equals(font));
  assert.equal(again.slotCount, 2);
  assert.equal(again.glyphId(66), 1);
  assert.throws(() => fp.Font.fromDict({ ...data, extra: 1 }), invalid(/unknown field `extra`/));
  const c = font.copy();
  c.addGlyph(glyph('#'));
  assert.equal(font.length, 1);
  assert.equal(c.length, 2);
  assert.ok(!font.equals(c));
  assert.equal(font.equals('font'), false);
  assert.equal(font.equals(font.glyph(1)), false);
  const g = fp.Glyph.fromDict(font.glyph(1).toDict());
  assert.ok(g.equals(font.glyph(1)));
  assert.ok(g.copy().equals(g));
  assert.match(g.toString(), /^Glyph\(id=undefined, codepoints=\[Codepoint\.unicode\(0x0042\)\]/);
  assert.match(font.glyph(1).toString(), /^Glyph\(id=1, /);
});

test('FontSet API: constructor, fonts, append/pop, single/intoSingle, validate, usage', () => {
  const a = new fp.Font([glyph('#', [65])]);
  const b = new fp.Font([glyph('##', [66])]);
  const s = new fp.FontSet([a], { sourceFormat: 'bdf' });
  assert.equal(s.length, 1);
  assert.equal(s.sourceFormat, 'bdf');
  assert.equal(s.append(b), 1);
  assert.equal(s.append(b.toDict()), 2);
  assert.deepEqual([...s].map((f) => f.length), [1, 1, 1]);
  assert.equal(s.font(2).glyphId(66), 0);
  s.pop();
  assert.throws(() => s.font(2), invalid(/out of range/));
  assert.throws(() => s.single(), (e) => e instanceof fp.InvalidInputError && e.message === 'expected one font, found 2');
  assert.throws(() => s.intoSingle(), invalid(/expected one font/));
  const popped = s.pop();
  assert.equal(popped.isView, false);
  assert.ok(popped.equals(b));
  const single = s.single();
  assert.equal(single.isView, true);
  const owned = s.intoSingle();
  assert.equal(owned.isView, false);
  assert.ok(owned.equals(a));
  assert.equal(s.length, 1);
  single.addGlyph(glyph('#', [67]));
  assert.equal(s.font(0).length, 2);
  assert.equal(owned.length, 1);
  s.fonts = [b, a];
  assert.ok(s.font(0).equals(b));
  assert.deepEqual(s.fonts.map((f) => f.isView), [true, true]);
  s.validate();
  assert.throws(() => s.validate({ max_glyphs: 1 }), (e) => e instanceof fp.ResourceLimitError && e.resource === 'glyph count' && e.limit === 1);
  const usage = s.retainedUsage();
  assert.equal(usage.fonts, 2);
  assert.equal(usage.glyphs, 2);
  assert.throws(() => s.retainedUsage(1), fp.ResourceLimitError);
  const d = fp.FontSet.fromDict(s.toDict());
  assert.ok(d.equals(s));
  assert.ok(s.copy().equals(s));
  assert.equal(s.toString(), 'FontSet(fonts=2, sourceFormat="bdf")');
  s.sourceFormat = undefined;
  assert.equal(s.sourceFormat, undefined);
  assert.throws(() => new fp.FontSet([a], { source_format: 'x' }), invalid(/unknown option `source_format`/));
  assert.throws(() => new fp.FontSet(a), invalid(/fonts: expected an array/));
  assert.throws(() => new fp.FontSet().pop(), invalid(/pop from an empty FontSet/));
});

test('Properties list: value coercion, tagged form, case-insensitive set/remove, raw bytes', () => {
  const p = new fp.Properties();
  assert.equal(p.length, 0);
  assert.equal(p.isEmpty(), true);
  p.set('FOUNDRY', 'Misc');
  p.push('COMMENT', 'one');
  p.push('COMMENT', 'two', new TextEncoder().encode('two'));
  p.set('pixel_size', 13);
  p.set('BIG', 2n ** 64n - 1n);
  p.set('NEG', -(2n ** 63n));
  p.set('FLAG', null);
  p.set('BLOB', new Uint8Array([0, 1]));
  p.set('LIST', [1, 'x']);
  p.set('DEC', { Decimal: '1.5' });
  assert.equal(p.length, 10);
  assert.deepEqual(p.get('COMMENT'), { Text: 'two' });
  assert.equal(p.get('PIXEL_SIZE'), undefined);
  assert.deepEqual(p.getCaseInsensitive('PIXEL_SIZE'), { Integer: 13 });
  assert.deepEqual(p.get('BIG'), { Unsigned: 2n ** 64n - 1n });
  assert.deepEqual(p.get('NEG'), { Integer: -(2n ** 63n) });
  assert.equal(p.get('FLAG'), 'Flag');
  assert.deepEqual(p.get('BLOB'), { Bytes: [0, 1] });
  assert.deepEqual(p.get('LIST'), { List: [{ Integer: 1 }, { Text: 'x' }] });
  assert.deepEqual(p.get('DEC'), { Decimal: '1.5' });
  // A case-insensitive replace keeps the original spelling and position.
  p.set('foundry', 'Other');
  assert.deepEqual(p.at(0), { name: 'FOUNDRY', value: { Text: 'Other' } });
  assert.deepEqual(p.at(2).raw, [116, 119, 111]);
  assert.deepEqual(p.at(-1), { name: 'DEC', value: { Decimal: '1.5' } });
  assert.equal(p.at(50), undefined);
  assert.equal(p.remove('comment'), 2);
  assert.deepEqual([...p].map((q) => q.name), ['FOUNDRY', 'pixel_size', 'BIG', 'NEG', 'FLAG', 'BLOB', 'LIST', 'DEC']);
  assert.ok(p.equals(p.toList()));
  assert.ok(new fp.Properties(p.toList()).equals(p));
  assert.ok(new fp.Properties(p).equals(p));
  assert.throws(() => p.set('X', true), invalid(/boolean/));
  assert.throws(() => p.set('X', 1.5), invalid(/not an exact integer/));
  assert.throws(() => p.set('X', 2n ** 64n), invalid(/out of range/));
  assert.throws(() => p.set('X', { Integer: 1, Text: 'x' }), fp.InvalidInputError);
  assert.equal(p.toString(), 'Properties(["FOUNDRY", "pixel_size", "BIG", "NEG", "FLAG", "BLOB", "LIST", "DEC"])');
});

test('Properties are live views of FontSet, Font, headers, localized names and Glyph', () => {
  const font = new fp.Font();
  font.properties.set('FONT_ASCENT', 7);
  font.headers.push('STARTFONT', '2.1');
  font.localizedNames.set('ja', 'テスト');
  assert.deepEqual(font.properties.get('FONT_ASCENT'), { Integer: 7 });
  assert.deepEqual(font.toDict().headers, [{ name: 'STARTFONT', value: { Text: '2.1' } }]);
  assert.deepEqual(font.names.localized, [{ name: 'ja', value: { Text: 'テスト' } }]);
  const s = new fp.FontSet([font]);
  s.properties.set('SET', 1);
  s.font(0).properties.set('FONT_DESCENT', 2);
  assert.deepEqual(s.toDict().properties, [{ name: 'SET', value: { Integer: 1 } }]);
  assert.deepEqual(s.font(0).properties.get('FONT_DESCENT'), { Integer: 2 });
  assert.equal(font.properties.get('FONT_DESCENT'), undefined);
  const g = new fp.Glyph(mono('#'));
  g.properties.set('X', 'y');
  assert.deepEqual(g.properties.get('X'), { Text: 'y' });
  const detached = g.properties.copy();
  detached.set('X', 'z');
  assert.deepEqual(g.properties.get('X'), { Text: 'y' });
  g.properties = detached;
  assert.deepEqual(g.properties.get('X'), { Text: 'z' });
  font.properties = [{ name: 'A', value: { Integer: 1 } }];
  assert.equal(font.properties.length, 1);
  s.properties = new fp.Properties();
  assert.equal(s.properties.isEmpty(), true);
  s.font(0).headers = font.headers;
  assert.deepEqual(s.font(0).headers.toList(), font.headers.toList());
  // A glyph-properties view follows its glyph; it fails once the glyph is gone.
  const inSet = new fp.FontSet([new fp.Font([glyph('#', [65])])]);
  const props = inSet.font(0).glyph(0).properties;
  props.set('K', 1);
  assert.deepEqual(inSet.toDict().fonts[0].glyphs[0].properties, [{ name: 'K', value: { Integer: 1 } }]);
  inSet.font(0).removeGlyph(0);
  assert.throws(() => props.length, invalid(/no longer exists/));
  assert.throws(() => (s.properties = [{ name: 'x' }]), invalid(/missing field `value`/));
  assert.throws(() => (s.properties = font), invalid(/expected a Properties or its plain data, got a Font/));
});

test('handle-or-plain arguments never consume the handle; released handles throw', () => {
  const g = glyph('#', [65]);
  const font = new fp.Font([g, g, g.toDict()]);
  assert.equal(font.length, 3);
  // The glyph handle is still usable (it was borrowed, not moved).
  assert.equal(g.bitmap.width, 1);
  const set = new fp.FontSet([font, font]);
  set.append(set.font(0)); // a view of the receiver itself
  assert.equal(set.length, 3);
  assert.equal(font.length, 3);
  // Views of the same font as arguments are snapshotted before the edit.
  const view = set.font(0);
  view.addGlyph(view.glyph(0));
  assert.equal(view.length, 4);
  g.free();
  assert.throws(() => new fp.Font([g]), invalid(/Glyph handle was released/));
  assert.throws(() => font.addGlyph(g), invalid(/Glyph handle was released/));
  assert.throws(() => font.glyphId(new fp.Font()), fp.InvalidInputError);
  const cp = fp.Codepoint.unicode(65);
  cp.free();
  assert.throws(() => font.glyphId(cp), invalid(/Codepoint handle was released/));
});

test('Number/BigInt policy on model fields: per value, exact both ways', () => {
  const g = glyph('#', [65]);
  const m = g.metrics;
  m.advance = { x: Number.MAX_SAFE_INTEGER, y: -(2n ** 53n) };
  g.metrics = m;
  assert.deepEqual(g.metrics.advance, { x: Number.MAX_SAFE_INTEGER, y: -(2n ** 53n) });
  assert.equal(typeof g.metrics.advance.x, 'number');
  assert.equal(typeof g.metrics.advance.y, 'bigint');
  const set = new fp.FontSet([new fp.Font([g])]);
  set.properties.set('U64', 2n ** 64n - 1n);
  set.properties.set('SMALL', 2n);
  const dict = set.toDict();
  assert.deepEqual(dict.properties, [
    { name: 'U64', value: { Unsigned: 2n ** 64n - 1n } },
    { name: 'SMALL', value: { Integer: 2 } },
  ]);
  assert.ok(fp.FontSet.fromDict(dict).equals(set));
  // Glyph IDs and counts are usize (32-bit on wasm32): always numbers.
  assert.equal(typeof set.font(0).glyphIds()[0], 'number');
  assert.throws(() => set.font(0).glyph(2 ** 32), invalid(/out of range/));
  assert.throws(() => set.font(0).glyph(2 ** 53), invalid(/not an exact integer/));
});

test('GlyphBitmap through the model: width, height, bytes, ink box and crop', () => {
  const g = new fp.Glyph(mono('....\n.##.\n....'));
  const b = g.bitmap;
  assert.equal(b.width, 4);
  assert.equal(b.height, 3);
  assert.deepEqual([...b.asBytes()], [0x00, 0x60, 0x00]);
  // bbox + crop: trimming crops to the ink box (GlyphBitmap::bbox/crop).
  g.setBitmap(b, 0, 0, true);
  assert.deepEqual(g.metrics.bounds, { width: 2, height: 1, x: 1, y: 1 });
  assert.equal(g.bitmap.toText(), '##');
  // crop also extends: a larger cell pads with blank pixels.
  assert.equal(g.bitmapInCell({ width: 4, height: 3, x: 0, y: 0 }).toText(), '....\n.##.\n....');
  const gray = new fp.Glyph(new fp.Bitmap(2, 2, 'gray8'));
  assert.equal(gray.bitmap.depth, 'gray8');
  assert.equal(gray.bitmapInCell({ width: 3, height: 1, x: 0, y: 0 }).depth, 'gray8');
  // Direct Bitmap.bbox / Bitmap.cropBounds come from the bitmap unit.
  if (typeof fp.Bitmap.prototype.bbox === 'function') {
    assert.deepEqual(b.bbox(), { width: 2, height: 1, x: 1, y: 1 });
  }
});

test('Font.draw, drawCodepoints and drawAll render through the model (render unit)', { skip: typeof fp.Font.prototype.draw !== 'function' && 'Font.draw* are bound by the render unit (not in this build yet)' }, () => {
  const font = new fp.Font([glyph('#.\n##', [65]), glyph('##\n#.', [66])]);
  for (const g of font) {
    const m = g.metrics;
    m.advance = { x: 3, y: 0 };
    g.metrics = m;
  }
  const ab = font.draw('AB');
  assert.ok(ab.bitmap instanceof fp.Bitmap);
  assert.ok(ab.bitmap.width >= 5);
  assert.ok(font.drawCodepoints([0x41, 'B']).bitmap.equals(ab.bitmap));
  assert.ok(font.drawAll().bitmap.width > 0);
  // Strict mode turns the missing glyph into StrictLossError carrying the complete report.
  assert.throws(
    () => new fp.Font().draw('A', { strict: true }),
    (e) => e instanceof fp.StrictLossError && e.kind === 'StrictLoss' && e.exitCode === 3 && fp.reportHasLoss(e.report) && e.report.entries.length >= 1,
  );
});

test('FontCharset codec through CharsetCodec.forFont (charset unit)', { skip: typeof fp.CharsetCodec?.forFont !== 'function' && 'CharsetCodec is bound by the charset unit (not in this build yet)' }, () => {
  const font = new fp.Font();
  font.charset = { name: 'cp1252', code_ranges: [] };
  const codec = fp.CharsetCodec.forFont(font);
  assert.equal(codec.name, font.charsetCanonicalName());
  assert.equal(codec.known, true);
});
