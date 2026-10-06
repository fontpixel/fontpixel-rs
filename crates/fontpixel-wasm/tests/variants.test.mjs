// variants: regional alternatives and locl output compared with the same
// Rust calls (expected.json) and the native CLI (cli.* keys, plus a live CLI
// run on the same BDF files).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { DATA, HAS_CLI, cliFiles, load } from './helpers.mjs';
import { E, P3, assertBytes, font, h, needs, norm, throwsError } from './sfnt_support.mjs';

const fp = await load();
const skip = needs(fp, 'opentype');

/** CLI: --base mini.bdf --lang zh-Hant=alt.bdf --script latn=latin.bdf */
function sample() {
  const v = new fp.Variants(font('mini'));
  v.addLanguage('zh-Hant', font('alt'));
  v.addScript(['latn'], font('latin'));
  return v;
}

test('variants cli bytes', { skip }, () => {
  const v = sample();
  const ttf = v.encode();
  assertBytes(ttf.bytes, h('variants.ttf'));
  assertBytes(ttf.bytes, h('cli.variants.ttf'), 'frozen CLI');
  assert.deepEqual(norm(ttf.report), E['variants.ttf.report']);
  assertBytes(fp.variants.encode(v).bytes, ttf.bytes);
  const otf = v.encode({ flavor: 'cff' });
  assertBytes(otf.bytes, h('variants.otf'));
  assertBytes(otf.bytes, h('cli.variants.otf'), 'frozen CLI');
  assert.equal(new fp.Sfnt(otf.bytes).flavor, 'cff');
  // CLI --shape dot uses dot_size 0.9 and encode_owned
  const d = new fp.Variants(font('mini'));
  d.addLanguage('zh-Hant', font('alt'));
  const dot = d.encodeOwned({ shape: 'dot', dot_size: 0.9 });
  assertBytes(dot.bytes, h('variants.dot'));
  assertBytes(dot.bytes, h('cli.variants-dot.ttf'), 'frozen CLI');
  assert.equal(d.consumed, true);
  throwsError(() => d.encode(), fp.InvalidInputError, { message: /consumed/ });
  throwsError(() => fp.variants.encode(d), fp.InvalidInputError, { message: /consumed/ });
  // CLI --embed-bitmaps --family --vendor
  const e = new fp.Variants(font('mini')).encodeOwned({ embed_bitmaps: true, family: 'P3 Test', vendor: 'TEST' });
  assertBytes(e.bytes, h('variants.embed.owned'));
  assertBytes(e.bytes, h('cli.variants-embed.ttf'), 'frozen CLI');
  // CLI -o x.woff2 (non-dot shapes default to CFF inside WOFF2)
  const w = new fp.Variants(font('mini'));
  w.addLanguage('zh-Hant', font('alt'));
  assertBytes(fp.woff.woff2.encode(w.encode({ flavor: 'cff' }).bytes).bytes, h('cli.variants.woff2'), 'frozen CLI woff2');
});

test('variants output equals a live native CLI run', { skip: skip || (!HAS_CLI && 'native CLI not found') }, () => {
  // The same command as the cli.* expectations, run now on the BDF files.
  const inputs = {
    'mini.bdf': new Uint8Array(readFileSync(path.join(DATA, 'compat', 'mini.bdf'))),
    'alt.bdf': new Uint8Array(readFileSync(path.join(P3, 'alt.bdf'))),
    'latin.bdf': new Uint8Array(readFileSync(path.join(P3, 'latin.bdf'))),
  };
  const read = (name) => fp.read(inputs[name], { name }).fonts.font(0);
  const v = new fp.Variants(read('mini.bdf'));
  v.addLanguage('zh-Hant', read('alt.bdf'));
  v.addScript(['latn'], read('latin.bdf'));
  for (const [out, options] of [
    ['x.ttf', undefined],
    ['x.otf', { flavor: 'cff' }],
  ]) {
    const cli = cliFiles(inputs, [
      'variants', '--base', 'mini.bdf', '--lang', 'zh-Hant=alt.bdf', '--script', 'latn=latin.bdf', '-o', out,
    ]);
    assert.equal(cli.code, 0, cli.stderr);
    assertBytes(v.encode(options).bytes, cli.files[out], out);
  }
  // fonts read from bytes in WASM equal the Rust reader's models
  assert.deepEqual(norm(v.toDict().base), font('mini'));
});

test('variants prepared', { skip }, () => {
  const v = sample();
  assert.deepEqual(norm(v.plan()), E['variants.plan']);
  const p = v.prepare();
  assert.deepEqual(norm(p.plan()), E['variants.plan']);
  assert.ok(p.retainedBytes() > 0);
  const total = fp.sfnt.defaults()['gsub.LayoutLimits'].sfnt.resources.max_total_bytes;
  assert.equal(p.encodeAllowance(), total - p.retainedBytes());
  assertBytes(p.encode().bytes, h('variants.ttf'));
  assertBytes(p.encode({ flavor: 'cff' }).bytes, h('variants.otf'));
  assert.equal(v.consumed, false);
  // the prepared plan is independent of later edits
  v.addScript(['grek'], font('latin'));
  assertBytes(p.encode().bytes, h('variants.ttf'));
  const owned = sample().intoPrepared();
  assertBytes(owned.encode().bytes, h('variants.ttf'));
  const consumedHandle = sample();
  consumedHandle.intoPrepared();
  assert.equal(consumedHandle.consumed, true);
  throwsError(() => consumedHandle.intoPrepared(), fp.InvalidInputError);
  throwsError(() => p.encode({ limits: { max_records: 5 } }), fp.InvalidInputError, { message: /matching layout limits/ });
  throwsError(() => sample().prepare({ max_records: 1 }), fp.ResourceLimitError);
});

test('variants model', { skip }, () => {
  const v = sample();
  const langs = v.languages;
  assert.deepEqual(
    langs.map((l) => [l.language, l.scripts]),
    [['ZHT ', ['DFLT', 'hani', 'kana', 'hang', 'bopo']]],
  );
  assert.ok(langs[0].font instanceof fp.Font);
  assert.deepEqual(v.scripts.map((s) => s.scripts), [['latn']]);
  const d = v.toDict();
  assert.deepEqual(norm(d.base), font('mini'));
  assertBytes(fp.Variants.fromDict(d).encode().bytes, h('variants.ttf'));
  // Font handles (also views) are accepted wherever fonts are
  const w = new fp.Variants(v.base);
  w.languages = [{ language: 'ZHT ', scripts: ['DFLT', 'hani', 'kana', 'hang', 'bopo'], font: langs[0].font }];
  w.scripts = [{ scripts: ['latn'], font: v.scripts[0].font }];
  assertBytes(w.encode().bytes, h('variants.ttf'));
  const set = new fp.FontSet();
  set.append(fp.Font.fromDict(font('alt')));
  const x = new fp.Variants(font('latin'));
  x.base = font('mini');
  x.addLanguageVariant('ZHT ', ['DFLT', 'hani', 'kana', 'hang', 'bopo'], set.font(0));
  x.addScript([new TextEncoder().encode('latn')], font('latin'));
  assertBytes(x.encode().bytes, h('variants.ttf'));
  // copies are independent
  const copy = x.copy();
  x.scripts = [];
  assertBytes(copy.encode().bytes, h('variants.ttf'));
  assert.ok(v.retainedUsage(2 ** 30) > 0);
  throwsError(() => v.retainedUsage(10), fp.ResourceLimitError, { resource: 'retained variants bytes', limit: 10 });
  throwsError(() => (w.languages = [{ language: 'ZHT ', scripts: [], font: font('alt'), extra: 1 }]), fp.InvalidInputError, {
    message: /unknown key `extra`/,
  });
  throwsError(() => (w.scripts = [{ scripts: ['latin'], font: font('alt') }]), fp.InvalidInputError, { message: /four characters/ });
  const freed = fp.Font.fromDict(font('alt'));
  freed.free();
  throwsError(() => w.addLanguage('ja', freed), fp.InvalidInputError, { message: /Font handle was released/ });
});

test('variants single and tags', { skip }, () => {
  assertBytes(fp.variants.encodeSingle(font('mini')).bytes, h('variants.single'));
  assertBytes(fp.variants.encodeSingleOwned(font('mini')).bytes, h('variants.single'));
  assertBytes(fp.variants.encodeSingle(fp.Font.fromDict(font('mini'))).bytes, h('variants.single'));
  const emb = { embed_bitmaps: true, family: 'P3 Test', vendor: 'TEST' };
  assertBytes(fp.variants.encodeSingle(font('mini'), emb).bytes, h('variants.embed'));
  // encode_single keeps physical glyph identity; Variants planning of the
  // same face differs by design (decisions.md)
  assert.notDeepEqual(
    Array.from(fp.variants.encodeSingle(font('mini')).bytes),
    Array.from(new fp.Variants(font('mini')).encode().bytes),
  );
  assert.deepEqual(['zh-Hant', 'zh-HK', 'ja', 'ko', 'ZHS'].map(fp.variants.languageTag), E['variants.language_tag']);
  assert.equal(fp.variants.scriptTag('latn'), 'latn');
  throwsError(() => fp.variants.scriptTag('latin'), fp.InvalidInputError);
  throwsError(() => fp.variants.languageTag('xx-not-a-language'), fp.InvalidInputError);
  assert.deepEqual(
    fp.variants.notdefEmptyMetrics({ empty: { advance_width: 600, left_side_bearing: -5 } }),
    E['variants.notdef_empty'],
  );
  assert.equal(fp.variants.notdefEmptyMetrics('preserve'), null);
});

test('variants errors and options', { skip }, () => {
  const d = fp.variants.defaults();
  assert.equal(d.Options.units_per_pixel, 100);
  assert.equal(d.Options.vendor, 'NONE');
  assert.equal(d.Options.notdef, 'preserve');
  assert.equal(d.NotdefPolicy, 'preserve');
  assert.deepEqual(d.Options.metadata, d.MetadataOptions);
  assert.deepEqual(new fp.Variants(font('mini')).encode(d.Options), new fp.Variants(font('mini')).encode());
  assertBytes(fp.variants.encodeSingle(font('mini'), d.Options).bytes, h('variants.single'));
  fp.variants.validateMetadata(d.MetadataOptions, 'true_type', 'square');
  throwsError(() => fp.variants.validateMetadata({ selection_flags: 0x80 }, 'true_type', 'square'), fp.InvalidInputError);
  throwsError(() => fp.variants.validateMetadata({}, 'true_type', 'circle'), fp.InvalidInputError);
  throwsError(() => new fp.Variants(font('mini')).encode({ no_such_option: true }), fp.InvalidInputError);
  throwsError(() => new fp.Variants(font('mini')).encode({ shape: 'square', dot_size: 0.5 }), fp.InvalidInputError);
  throwsError(() => new fp.Variants(font('mini')).addLanguage('not a tag', font('alt')), fp.InvalidInputError);
  const bigger = font('alt');
  bigger.metrics.pixel_size = 9;
  const v = new fp.Variants(font('mini'));
  v.addLanguage('ja', bigger);
  throwsError(() => v.encode(), fp.InvalidInputError);
  throwsError(() => sample().encode({ limits: { sfnt: { resources: { max_total_bytes: 1000 } } } }), fp.ResourceLimitError);
  throwsError(() => new fp.Variants(), fp.InvalidInputError);
  throwsError(() => new fp.Variants(fp.Bitmap.fromText('#')), fp.InvalidInputError, { message: /got a Bitmap handle/ });
  throwsError(() => fp.variants.encode(font('mini')), fp.InvalidInputError, { message: /Variants handle/ });
});
