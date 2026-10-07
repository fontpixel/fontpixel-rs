// Name overrides, bitmap projection, report helpers and the error classes
// (mirrors crates/fontpixel-py/tests/test_metadata.py), compared with the
// native command line where it applies the same operation.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { BLOCK, HAS_CLI, buildInfo, cliFiles, load, sample } from './helpers.mjs';

const fp = await load();
const invalid = (pattern) => (e) => e instanceof fp.InvalidInputError && e.kind === 'InvalidInput' && pattern.test(e.message);

function sampleSet() {
  const font = new fp.Font();
  font.names = {
    family: 'Original',
    style: 'Regular',
    full: 'Original Regular',
    postscript: 'Original-Regular',
    xlfd_fields: ['source', 'original'],
    localized: [],
  };
  const g = new fp.Glyph(fp.Bitmap.fromText('#.\n##'), { codepoints: [65, 66], name: 'A' });
  const m = g.metrics;
  m.advance = { x: 4, y: 0 };
  g.metrics = m;
  font.addGlyph(g);
  return new fp.FontSet([font, font], { sourceFormat: 'source-container' });
}

function projectionSet() {
  const g = new fp.Glyph(fp.Bitmap.fromText('#'));
  const m = g.metrics;
  m.advance = { x: 6, y: 0 };
  m.scalable_advance = { x: 500, y: 0 };
  g.metrics = m;
  return new fp.FontSet([new fp.Font([g])]);
}

test('applyNames is atomic and bounded', () => {
  const fonts = sampleSet();
  const before = fonts.copy();
  for (const value of ['', ' \t\n', ' ']) {
    assert.throws(
      () => fp.applyNames(fonts, { family: value, style: 'Bold' }),
      (e) => e instanceof fp.InvalidInputError && e.kind === 'InvalidInput' && e.exitCode === 1,
    );
    assert.ok(fonts.equals(before));
  }
  assert.throws(
    () => fp.applyNames(fonts, { family: '字母' }, { max_table_bytes: 5 }),
    (e) => e instanceof fp.ResourceLimitError && e.resource === 'metadata name bytes' && e.limit === 5,
  );
  assert.ok(fonts.equals(before));
  const report = fp.applyNames(fonts, { family: '字母' }, { max_table_bytes: 6 });
  assert.equal(fonts.font(0).names.family, '字母');
  assert.equal(fonts.font(1).names.family, '字母');
  assert.ok(Array.isArray(report.entries));
  assert.ok(report.entries.every((e) => e.code.startsWith('metadata.')), JSON.stringify(report));
  assert.throws(() => fp.applyNames(fonts, { famliy: 'typo' }), invalid(/unknown field `famliy`/));
  assert.deepEqual(fp.applyNames(fonts), { entries: [] });
  assert.deepEqual(fp.applyNames(fonts, {}), { entries: [] });
  assert.equal(fp.nameOverridesIsEmpty({}), true);
  assert.equal(fp.nameOverridesIsEmpty({ family: 'x' }), false);
  assert.throws(() => fp.nameOverridesIsEmpty({ bogus: 1 }), fp.InvalidInputError);
  const doc = new fp.FontSet([new fp.Font()]);
  assert.equal(fp.reportHasLoss(fp.applyNames(doc, { family: 'Example Pixel' })), false);
  assert.equal(doc.font(0).names.family, 'Example Pixel');
});

test('applyNames matches the native convert --family report and model', { skip: !HAS_CLI && 'native CLI not found' }, () => {
  const cli = cliFiles({ 'in.bdf': sample(BLOCK) }, ['convert', 'in.bdf', '-o', 'out.bdf', '--family', 'Demo Pixel', '--style', 'Bold', '--report', 'json']);
  assert.equal(cli.code, 0, cli.stderr);
  const document = JSON.parse(new TextDecoder().decode(cli.stdout));
  const set = fp.read(sample(BLOCK)).fonts;
  const report = fp.applyNames(set, { family: 'Demo Pixel', style: 'Bold' });
  assert.deepEqual(report, document.report);
  const written = fp.read(cli.files['out.bdf']).fonts.font(0).names;
  assert.equal(written.family, set.font(0).names.family);
  assert.equal(written.style, set.font(0).names.style);
});

test('BitmapProjection scalable_from_device: outcome, retained bytes and loss report', () => {
  const s = projectionSet();
  const p = fp.BitmapProjection.scalableFromDevice(1000, 13);
  assert.equal(p.isIdentity(), false);
  assert.deepEqual(p.scalableAdvance, { from_device: { units_per_em: 1000, reference_ppem: 13 } });
  const outcome = fp.applyBitmapProjection(s, p, 1 << 20);
  assert.deepEqual(s.font(0).glyph(0).metrics.scalable_advance, { x: 462, y: 0 });
  assert.equal(outcome.scalable_advance_changes.length, 1);
  const change = outcome.scalable_advance_changes[0];
  assert.equal(change.glyph, 0);
  assert.deepEqual(change.after, { x: 462, y: 0 });
  assert.equal(fp.reportHasLoss(outcome.report), true);
  assert.ok(outcome.retained_bytes > 0);
  // The same through the method, on a fresh set.
  const again = projectionSet();
  assert.deepEqual(p.apply(again).scalable_advance_changes, outcome.scalable_advance_changes);
});

test('BitmapProjection metadata options and property edits', () => {
  const s = new fp.FontSet([new fp.Font()]);
  const p = new fp.BitmapProjection({
    font_name: '-opf-demo-medium-r-normal--13-130-75-75-p-130-iso10646-1',
    size: { point_size: { numerator: 13, denominator: 1 }, resolution: { x: 75, y: 75 } },
    properties: [{ set: { name: 'PIXEL_SIZE', value: 13 } }],
  });
  assert.deepEqual(p.propertyNames(), ['PIXEL_SIZE']);
  assert.deepEqual(p.properties, [{ set: { name: 'PIXEL_SIZE', value: 13 } }]);
  assert.ok(p.fontName.startsWith('-opf-demo'));
  assert.deepEqual(p.size.resolution, { x: 75, y: 75 });
  assert.equal(new fp.BitmapProjection().size, undefined);
  const outcome = p.apply(s, 1 << 20);
  const font = s.font(0);
  assert.equal(font.names.xlfd_fields[6], '13');
  assert.equal(font.metrics.pixel_size, 13);
  assert.equal(fp.reportHasLoss(outcome.report), false);
  assert.equal(outcome.metadata_changes.length, 5);
  // Remove edits, bigint integer values and the options-object form of the function.
  const out = fp.applyBitmapProjection(s, {
    properties: [
      { remove: { name: 'pixel_size' } },
      { set: { name: 'WEIGHT_NAME', value: 'Bold' } },
      { set: { name: 'QUAD_WIDTH', value: 7n } },
    ],
  });
  assert.equal(s.font(0).properties.getCaseInsensitive('PIXEL_SIZE'), undefined);
  assert.deepEqual(s.font(0).properties.get('WEIGHT_NAME'), { Text: 'Bold' });
  assert.deepEqual(s.font(0).properties.get('QUAD_WIDTH'), { Integer: 7 });
  assert.ok(out.retained_bytes >= 0);
});

test('bitmap projection validation is transactional; MAX_* limits', () => {
  const s = projectionSet();
  const before = s.copy();
  assert.equal(new fp.BitmapProjection().isIdentity(), true);
  assert.deepEqual(fp.applyBitmapProjection(s, new fp.BitmapProjection()).metadata_changes, []);
  assert.throws(() => fp.applyBitmapProjection(s, fp.BitmapProjection.scalableFromDevice(1000, 0)), fp.InvalidInputError);
  assert.throws(() => fp.applyBitmapProjection(s, { font_name: ' padded ' }), fp.InvalidInputError);
  const edits = Array.from({ length: fp.BitmapProjection.MAX_PROPERTY_EDITS + 1 }, (_, i) => ({ set: { name: `P${i}`, value: i } }));
  assert.throws(() => fp.applyBitmapProjection(s, { properties: edits }), fp.InvalidInputError);
  assert.throws(() => fp.applyBitmapProjection(s, { font_name: 'x'.repeat(fp.BitmapProjection.MAX_FONT_NAME_BYTES + 1) }), fp.InvalidInputError);
  const longName = 'N'.repeat(fp.BitmapProjection.MAX_PROPERTY_NAME_BYTES + 1);
  assert.throws(() => fp.applyBitmapProjection(s, { properties: [{ set: { name: longName, value: 1 } }] }), fp.InvalidInputError);
  const big = 't'.repeat(fp.BitmapProjection.MAX_PROPERTY_TEXT_BYTES + 1);
  assert.throws(() => fp.applyBitmapProjection(s, { properties: [{ set: { name: 'T', value: big } }] }), fp.InvalidInputError);
  assert.throws(
    () => fp.applyBitmapProjection(s, fp.BitmapProjection.scalableFromDevice(1000, 13), 1),
    (e) => e instanceof fp.ResourceLimitError && e.limit === 1,
  );
  assert.ok(s.equals(before));
  assert.throws(() => new fp.BitmapProjection({ properties: [{ rename: { name: 'X' } }] }), invalid(/a property edit is/));
  assert.throws(() => new fp.BitmapProjection({ properties: [{ set: { name: 'X', value: 1.5 } }] }), invalid(/a property edit is/));
  assert.throws(() => new fp.BitmapProjection({ properties: [{ set: { name: 'X', value: 2n ** 63n } }] }), invalid(/a property edit is/));
  assert.throws(() => fp.applyBitmapProjection(s, { fontname: 'x' }), invalid(/unknown option `fontname`/));
  assert.throws(() => fp.applyBitmapProjection(s, s), invalid(/expected a BitmapProjection or its plain data, got a FontSet/));
  assert.throws(() => fp.BitmapProjection.scalableFromDevice(2 ** 32, 1), invalid(/out of range/));
  assert.throws(() => fp.applyBitmapProjection(s, {}, 2 ** 32), invalid(/out of range/));
  assert.deepEqual(
    [fp.BitmapProjection.MAX_PROPERTY_EDITS, fp.BitmapProjection.MAX_PROPERTY_NAME_BYTES],
    [64, 255],
  );
  assert.deepEqual([fp.BitmapProjection.MAX_PROPERTY_TEXT_BYTES, fp.BitmapProjection.MAX_FONT_NAME_BYTES], [65535, 4096]);
  assert.match(new fp.BitmapProjection().toString(), /^BitmapProjection\(/);
});

test('bitmap projection matches the native convert --swidth-ppem --font-name', { skip: !HAS_CLI && 'native CLI not found' }, () => {
  const fontName = '-x-demo-medium-r-normal--6-60-75-75-c-30-iso10646-1';
  const cli = cliFiles({ 'in.bdf': sample(BLOCK) }, [
    'convert', 'in.bdf', '-o', 'out.bdf', '--swidth-ppem', '7', '--swidth-units', '1000', `--font-name=${fontName}`, '--report', 'json',
  ]);
  assert.equal(cli.code, 0, cli.stderr);
  const document = JSON.parse(new TextDecoder().decode(cli.stdout));
  const set = fp.read(sample(BLOCK)).fonts;
  const outcome = fp.applyBitmapProjection(set, {
    scalable_advance: { from_device: { units_per_em: 1000, reference_ppem: 7 } },
    font_name: fontName,
  });
  // The complete report equals the command line's, entry for entry.
  assert.deepEqual(outcome.report, document.report);
  const advances = (fonts) => fonts.font(0).glyphs().map(([id, g]) => [id, g.metrics.scalable_advance]);
  const written = fp.read(cli.files['out.bdf']).fonts;
  assert.deepEqual(advances(written), advances(set));
  assert.equal(written.font(0).names.xlfd, set.font(0).names.xlfd);
  // --strict rejects the same loss natively (exit code 3).
  const strict = cliFiles({ 'in.bdf': sample(BLOCK) }, ['convert', 'in.bdf', '-o', 'out.bdf', '--swidth-ppem', '7', '--strict']);
  assert.equal(strict.code, 3);
  assert.equal(fp.reportHasLoss(outcome.report), true);
});

const ENTRY = {
  level: 'warning',
  impact: 'loss',
  code: 'test.loss',
  location: { font_index: 0, glyph_index: null, codepoint: 65, asset: null },
  message: 'lost',
};

test('report helpers: push, extend, loss/error checks, text form', () => {
  const empty = { entries: [] };
  assert.equal(fp.reportHasLoss(empty), false);
  assert.equal(fp.reportHasErrors(empty), false);
  const one = fp.reportPush(empty, ENTRY);
  assert.deepEqual(one, { entries: [ENTRY] });
  assert.deepEqual(empty, { entries: [] });
  assert.equal(fp.reportHasLoss(one), true);
  assert.equal(fp.reportHasErrors(one), false);
  const err = { ...ENTRY, level: 'error', impact: 'none', code: 'test.error' };
  const both = fp.reportExtend(one, { entries: [err] });
  assert.deepEqual(both.entries.map((e) => e.code), ['test.loss', 'test.error']);
  assert.equal(fp.reportHasErrors(both), true);
  assert.equal(fp.reportToText(both), 'Warning [test.loss] Loss: lost\nError [test.error] None: lost\n');
  assert.throws(() => fp.reportPush(empty, { ...ENTRY, level: 'fatal' }), invalid(/unknown variant `fatal`/));
  assert.throws(() => fp.reportHasLoss({ entries: [], extra: 1 }), invalid(/unknown field `extra`/));
  // Locations are usize/u32 fields: numbers; out-of-range values are rejected.
  assert.throws(() => fp.reportPush(empty, { ...ENTRY, location: { ...ENTRY.location, codepoint: 2 ** 32 } }), fp.InvalidInputError);
  assert.throws(() => fp.reportPush(empty, { ...ENTRY, location: { ...ENTRY.location, font_index: 2n ** 60n } }), fp.InvalidInputError);
});

test('error classes carry kind, exitCode and structured fields', () => {
  assert.ok(fp.ResourceLimitError.prototype instanceof fp.FontpixelError);
  assert.throws(
    () => new fp.Font([new fp.Glyph(fp.Bitmap.fromText('##'))]).validate({ max_pixels_per_glyph: 1 }),
    (e) => {
      assert.ok(e instanceof fp.ResourceLimitError && e instanceof fp.FontpixelError && e instanceof Error);
      assert.equal(e.kind, 'ResourceLimit');
      assert.equal(e.exitCode, 1);
      assert.equal(e.resource, 'glyph pixels');
      assert.equal(e.limit, 1);
      assert.equal(e.message, 'glyph pixels exceeds configured limit 1');
      return true;
    },
  );
  assert.throws(
    () => new fp.OutputBundle('a', [['a', new Uint8Array([1])], ['b', new Uint8Array([2])]]).intoSingle(),
    (e) => e instanceof fp.MultipleAssetsError && e.kind === 'MultipleAssets' && e.count === 2 && e.exitCode === 1,
  );
  assert.throws(
    () => new fp.ResourceSet().resolve('x.bin'),
    (e) => e instanceof fp.MissingResourceError && e.kind === 'MissingResource' && e.path === 'x.bin',
  );
  // A function result is the return value; failures are thrown, never returned.
  assert.equal(fp.resourcePath('a/b'), 'a/b');
});

test('NotImplementedFormatError names the operation (Operation::Read)', { skip: buildInfo().profile === 'full' && 'every format has a reader in the full build' }, () => {
  // Formats compiled out of this profile stay planned catalogue entries without a reader.
  assert.throws(
    () => fp.read(new Uint8Array([1, 2, 3]), { format: buildInfo().profile === 'web' ? 'amiga' : buildInfo().profile === 'merge' ? 'hex' : 'otb' }),
    (e) => e instanceof fp.NotImplementedFormatError && e.kind === 'NotImplemented' && e.operation === 'Read' && typeof e.format === 'string',
  );
});
