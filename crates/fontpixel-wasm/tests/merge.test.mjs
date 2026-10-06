// merge (library level, `fontpixel::merge`): compared with the same Rust
// calls (expected.json) and with live runs of the native CLI `merge`
// command on the same BDF files.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { DATA, HAS_CLI, cliFiles, has, load, withoutSource } from './helpers.mjs';
import { E, P3, assertBytes, font, h, norm, throwsError } from './sfnt_support.mjs';

const fp = await load();
const OPTIONS = { family: 'P3 Mix' };

/** CLI: --part mini.bdf --take U+20-41 --part alt.bdf --take U+42 */
function parts() {
  return [
    { font: font('mini'), selection: 'U+20-41' },
    { font: font('alt'), selection: new fp.Selection('U+42') },
  ];
}

test('merge matches rust and cli', () => {
  const r = fp.merge.merge(parts(), OPTIONS);
  assert.deepEqual(Object.keys(r).sort(), ['font', 'report']);
  assert.ok(r.font instanceof fp.Font);
  assert.equal(r.font.isView, false);
  // the merged model equals the direct Rust merge
  assert.deepEqual(norm(r.font.toDict()), E['merge.font']);
  if (has(fp, 'opentype')) {
    assert.deepEqual(norm(new fp.Variants(r.font).toDict().base), E['merge.font']);
    assertBytes(fp.variants.encodeSingle(r.font).bytes, h('merge.single'));
  }
  const text = fp.merge.reportToJson(r.report);
  assert.equal(text, E['merge.report_json']);
  assert.deepEqual(JSON.parse(text), E['cli.merge-report.json'].merge);
  assert.equal(r.report.family, 'P3 Mix');
  // explicit defaults give the same result; Font handles work as part fonts
  const full = { ...fp.merge.defaults().MergeOptions, family: 'P3 Mix' };
  const again = fp.merge.merge(
    [
      { font: fp.Font.fromDict(font('mini')), selection: 'U+20-41', dy: 0, label: null, license: fp.merge.defaults().PartLicense },
      { font: font('alt'), selection: 'U+42' },
    ],
    full,
  );
  assert.equal(fp.merge.reportToJson(again.report), text);
});

test('merge equals a live native CLI merge', { skip: !HAS_CLI && 'native CLI not found' }, () => {
  const inputs = {
    'mini.bdf': new Uint8Array(readFileSync(path.join(DATA, 'compat', 'mini.bdf'))),
    'alt.bdf': new Uint8Array(readFileSync(path.join(P3, 'alt.bdf'))),
  };
  const cli = cliFiles(inputs, [
    'merge', '--part', 'mini.bdf', '--take', 'U+20-41', '--part', 'alt.bdf', '--take', 'U+42',
    '--family', 'P3 Mix', '-o', 'merged.bdf', '--report', 'json',
  ]);
  assert.equal(cli.code, 0, cli.stderr);
  // Parts read from the same bytes in WASM, merged by the library.
  const read = (name) => fp.read(inputs[name], { name }).fonts.font(0);
  const r = fp.merge.merge(
    [
      { font: read('mini.bdf'), selection: 'U+20-41' },
      { font: read('alt.bdf'), selection: 'U+42' },
    ],
    OPTIONS,
  );
  // The command's report document carries the library report unchanged.
  const doc = JSON.parse(Buffer.from(cli.stdout).toString('utf8'));
  assert.deepEqual(JSON.parse(fp.merge.reportToJson(r.report)), doc.merge);
  // The written BDF, read back, holds exactly the merged glyphs and metrics.
  const written = fp.read(cli.files['merged.bdf'], { name: 'merged.bdf' }).fonts.font(0).toDict();
  const merged = r.font.toDict();
  const glyphs = (d) => d.glyphs.filter(Boolean).map((g) => [g.codepoints, g.bitmap, g.advance ?? null]);
  assert.deepEqual(glyphs(written), glyphs(merged));
  assert.deepEqual(written.metrics, merged.metrics);
  assert.equal(written.names.family, 'P3 Mix');
  assert.deepEqual(withoutSource(merged).names, withoutSource(norm(E['merge.font'])).names);
});

test('merge part fields and errors', () => {
  const p = parts();
  p[1].dy = 1;
  p[1].label = 'alt part';
  p[1].license = { license: 'OFL-1.1', modification: 'allowed' };
  const r = fp.merge.merge(p, OPTIONS);
  const rep = r.report.parts[1];
  assert.deepEqual([rep.label, rep.dy], ['alt part', 1]);
  // dy accepts a bigint too
  p[1].dy = 1n;
  assert.equal(fp.merge.merge(p, OPTIONS).report.parts[1].dy, 1);
  throwsError(() => fp.merge.merge([{ font: font('mini'), selection: 'U+41', take: 'x' }], OPTIONS), fp.InvalidInputError, {
    message: /unknown merge part key "take"/,
  });
  throwsError(() => fp.merge.merge([{ selection: 'U+41' }], OPTIONS), fp.InvalidInputError, { message: /needs `font`/ });
  throwsError(() => fp.merge.merge(parts(), { spacing: 'prop' }), fp.InvalidInputError, { message: /family/ });
  throwsError(() => fp.merge.merge(parts(), { family: '' }), fp.InvalidInputError);
  throwsError(() => fp.merge.merge([{ font: font('mini'), selection: 'U+zz' }], OPTIONS), fp.InvalidInputError);
  throwsError(() => fp.merge.merge(parts(), { ...OPTIONS, strict: true }), fp.StrictLossError, { kind: 'StrictLoss' });
  throwsError(() => fp.merge.merge(parts(), { ...OPTIONS, limits: { max_total_bytes: 64 } }), fp.ResourceLimitError, {
    limit: 64,
  });
  throwsError(() => fp.merge.merge([{ font: font('mini'), selection: 'U+41', dy: 0.5 }], OPTIONS), fp.InvalidInputError, {
    message: /dy/,
  });
  throwsError(() => fp.merge.merge('parts', OPTIONS), fp.InvalidInputError, { message: /array/ });
  throwsError(() => fp.merge.merge([[font('mini'), 'U+41']], OPTIONS), fp.InvalidInputError, { message: /plain object/ });
  // handles inside parts: views, released handles, other classes
  const set = new fp.FontSet();
  set.append(fp.Font.fromDict(font('mini')));
  const view = set.font(0);
  assert.equal(fp.merge.merge([{ font: view, selection: 'U+20-41' }, { font: font('alt'), selection: 'U+42' }], OPTIONS).report.family, 'P3 Mix');
  const freed = fp.Font.fromDict(font('mini'));
  freed.free();
  throwsError(() => fp.merge.merge([{ font: freed, selection: 'U+41' }], OPTIONS), fp.InvalidInputError, {
    message: /Font handle was released/,
  });
  throwsError(() => fp.merge.merge([{ font: fp.Bitmap.fromText('#'), selection: 'U+41' }], OPTIONS), fp.InvalidInputError, {
    message: /expected a Font handle or plain data, got a Bitmap handle/,
  });
  const freedSelection = new fp.Selection('U+41');
  freedSelection.free();
  throwsError(() => fp.merge.merge([{ font: font('mini'), selection: freedSelection }], OPTIONS), fp.InvalidInputError, {
    message: /Selection handle was released/,
  });
  // the merged font is an owned handle; the inputs are untouched
  const owned = fp.Font.fromDict(font('mini'));
  const before = owned.toDict();
  fp.merge.merge([{ font: owned, selection: 'U+20-41' }, { font: font('alt'), selection: 'U+42' }], OPTIONS);
  assert.deepEqual(owned.toDict(), before);
});

test('merge selection and enums', () => {
  const s = new fp.Selection('U+41-5A,-U+45');
  const exp = E['merge.selection'];
  assert.deepEqual(norm(s.ranges()), exp.ranges);
  assert.equal(s.scalarCount(), exp.count);
  assert.equal(s.contains(0x45), exp.contains_45);
  assert.equal(s.contains(0x41), true);
  assert.equal(s.expression, 'U+41-5A,-U+45');
  assert.equal(fp.Selection.parse('U+41').scalarCount(), 1);
  assert.equal(String(s), 'Selection("U+41-5A,-U+45")');
  throwsError(() => new fp.Selection('not a range'), fp.InvalidInputError);
  throwsError(() => s.contains(-1), fp.InvalidInputError);
  assert.deepEqual(['auto', 'mono', 'prop'].map(fp.merge.spacingAsStr), ['auto', 'mono', 'prop']);
  assert.deepEqual(['half', 'full'].map(fp.merge.geometricWidthAsStr), ['half', 'full']);
  assert.deepEqual(['unknown', 'allowed', 'prohibited'].map(fp.merge.modificationAsStr), ['unknown', 'allowed', 'prohibited']);
  throwsError(() => fp.merge.spacingAsStr('wide'), fp.InvalidInputError);
  const d = fp.merge.defaults();
  assert.equal(d.MergeOptions.family, '');
  assert.deepEqual(d.PartLicense, {
    copyright: null,
    license: null,
    license_url: null,
    modification: 'unknown',
    reserved_names: [],
  });
});
