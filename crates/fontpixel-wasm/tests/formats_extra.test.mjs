// Format helpers outside the uniform detect/read/write namespaces (a port of
// crates/fontpixel-py/tests/test_formats_extra.py), compared with the golden
// CLI hashes.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { REPO, load } from './helpers.mjs';
import { GOLDEN, NATIVE, PROFILE, enabled, hashes, sha, sourceFile } from './formats_support.mjs';

const fp = await load();
const UNI3 = fp.read(sourceFile('uni3'));
const FULL = PROFILE === 'full';

function needs(...ids) {
  const off = ids.filter((f) => !enabled(fp, f));
  return off.length ? `${off.join(', ')} compiled out of the ${PROFILE} build` : false;
}

function single(written) {
  return written.output.intoSingle();
}

function yaff(model, report) {
  return single(fp.write(model, 'yaff', { report }));
}

/** The glyph of `font` mapped to Unicode `cp`. */
function glyphFor(font, cp) {
  for (const id of font.glyphIds()) {
    const g = font.glyph(id);
    if (g.codepoints.some((c) => c.Unicode === cp)) return g;
  }
  throw new Error(`no glyph for U+${cp.toString(16)}`);
}

test('namespaces are importable modules', () => {
  const modules = fp.formats.modules();
  if (FULL) assert.equal(modules.length, 93);
  assert.deepEqual([...modules].sort(), modules);
  for (const name of modules) {
    const mod = fp.formats[name];
    assert.equal(typeof mod, 'object', name);
    assert.ok(mod.formatIds().length, name);
    for (const fn of ['read', 'write']) assert.ok(typeof mod[fn] === 'function' || name === 'x11', `${name}.${fn}`);
  }
  const served = new Set(modules.filter((n) => n !== 'x11').flatMap((n) => fp.formats[n].formatIds()));
  const compiled = fp.listFormats().filter((s) => s.format.feature_enabled).map((s) => s.format.id);
  assert.deepEqual(served, new Set(compiled));
  // Module-level helpers live on the same namespace object.
  for (const fn of ['catalogue', 'modules', 'normalize', 'optionTypes', 'configurableFormats']) {
    assert.equal(typeof fp.formats[fn], 'function', fn);
  }
  // Minimal and merge builds keep exactly their formats' namespaces.
  if (PROFILE === 'minimal') assert.deepEqual(modules, ['bdf', 'pcf', 'x11', 'yaff']);
  if (PROFILE === 'merge') assert.deepEqual(modules, ['bdf', 'embedded', 'pcf', 'vectorRaster', 'x11']);
});

test('catalogue constant', () => {
  assert.deepEqual(fp.formats.catalogue(), fp.listFormats().map((s) => s.format));
  assert.deepEqual(fp.formats.catalogue()[0], {
    id: 'bdf',
    name: 'Glyph Bitmap Distribution Format',
    extensions: ['bdf', 'bdf.gz'],
    feature_enabled: true,
  });
  assert.equal(fp.formats.catalogue().length, 99);
});

test('direct detectors reject other content', () => {
  const F = fp.formats;
  const junk = new TextEncoder().encode('not a font');
  assert.equal(F.bdf.detect(junk), 'no_match');
  assert.equal(F.pcf.detect(single(fp.write(UNI3.fonts, 'pcf'))), 'certain');
  assert.throws(() => F.bdf.detect('text'), fp.InvalidInputError);
  if (!FULL) return;
  assert.equal(F.hzk.detect(new Uint8Array(64)), 'no_match'); // HZK has no magic
  assert.equal(F.gem.detect(new Uint8Array(8), { name: 'font.gft' }), 'no_match');
  assert.throws(() => F.gem.detect(new Uint8Array(64), { limits: { max_input_bytes: 8 } }), fp.ResourceLimitError);
  assert.equal(F.raw.detect, undefined); // raw has no detector at all
});

test('direct reader shapes and limits', () => {
  const F = fp.formats;
  const data = single(fp.write(UNI3.fonts, 'bdf'));
  const { model: font, report } = F.bdf.read(data);
  assert.ok(font instanceof fp.Font);
  assert.deepEqual(report, { entries: [] });
  assert.throws(() => F.bdf.read(data, { limits: { max_glyphs: 3 } }), fp.ResourceLimitError);
  assert.throws(() => F.bdf.read(data, { options: {} }), /unknown option `options`/);
  if (enabled(fp, 'yaff')) {
    const { model: set } = F.yaff.read(yaff(UNI3.fonts));
    assert.ok(set instanceof fp.FontSet);
  }
  // Font-only writers take a one-strike set or a Font, never several strikes.
  const two = UNI3.fonts.copy();
  two.append(UNI3.fonts.font(0).copy());
  assert.throws(() => F.bdf.write(two), fp.InvalidInputError);
  const out = F.bdf.write(UNI3.fonts.font(0));
  assert.deepEqual(out.data, data);
  assert.deepEqual(out.report, { entries: [] });
  assert.deepEqual(F.bdf.write(UNI3.fonts).data, data);
  assert.throws(() => F.bdf.write(new Uint8Array(3)), (e) => e instanceof fp.InvalidInputError && /expected a FontSet or Font/.test(e.message));
});

test('x11 source rows and validation', () => {
  const F = fp.formats;
  const { model: font } = F.bdf.read(single(fp.write(UNI3.fonts, 'bdf')));
  const glyph = font.glyph(1);
  assert.deepEqual(F.bdf.sourceRows(glyph), ['80', '80', '80', '00', '80']);
  assert.deepEqual(F.x11.sourceRows(glyph), F.bdf.sourceRows(glyph));
  assert.equal(F.x11.validateBitmap(glyph), undefined);
  assert.equal(F.bdf.validateBitmap(glyph), undefined);
  const fresh = new fp.Glyph(glyph.bitmap);
  assert.equal(F.x11.sourceRows(fresh), undefined);
  assert.throws(() => F.x11.sourceRows(font), /expected a Glyph, got Font/);
  assert.deepEqual(F.x11.formatIds(), ['bdf', 'pcf']);
});

test('embedded codecs', { skip: needs('otb') }, () => {
  const F = fp.formats;
  const otb = GOLDEN.formats.otb.write.assets['font.otb'];
  const { data, report } = F.embedded.write(UNI3.fonts, 'open_type');
  assert.equal(sha(data), otb);
  assert.ok(Array.isArray(report.entries));
  assert.equal(F.embedded.detect(data, 'open_type'), 'certain');
  assert.equal(F.embedded.detect(new TextEncoder().encode('junk'), 'open_type'), 'no_match');
  const { model } = F.embedded.read(data, 'open_type');
  const viaRegistry = fp.read(data, { format: 'otb' }).fonts;
  if (enabled(fp, 'yaff')) assert.deepEqual(yaff(model), yaff(viaRegistry));
  else assert.deepEqual(model.toDict(), viaRegistry.toDict());
  assert.throws(() => F.embedded.read(data, 'png'), fp.InvalidInputError);
  const codec = F.embedded.codec('apple');
  assert.equal(codec.kind, 'apple');
  const keys = ['kind', 'read_options', 'write_options', ...(enabled(fp, 'color') ? ['color_write_options'] : [])];
  assert.deepEqual(new Set(Object.keys(codec)), new Set(keys));
  const md = fp.normalize('formats::embedded::metadata::EmbeddedMetadataOptions');
  assert.equal(F.embedded.validateMetadata(md, 'open_type'), undefined);
  assert.equal(F.embedded.metadataRetainedBytes(md), 0);
  assert.equal(F.embedded.metadataRetainedBytes(md, 10), 0);
  assert.throws(() => F.embedded.validateMetadata({ ...md, glyph_order: 'codepoint' }, 'color'), fp.InvalidInputError);
  assert.deepEqual(F.embedded.formatIds(), ['otb', 'bdat', 'color'].filter((f) => enabled(fp, f)));
  if (FULL) {
    const { data: color } = F.embedded.writeColor(UNI3.fonts);
    assert.equal(sha(color), GOLDEN.formats.color.write.assets['font.ttf']);
  } else {
    assert.equal(F.embedded.writeColor, undefined);
  }
});

test('vector raster helpers', { skip: needs('ttf') }, () => {
  const F = fp.formats;
  const { data: ttf } = F.vectorRaster.write(UNI3.fonts, 'ttf');
  assert.equal(sha(ttf), GOLDEN.formats.ttf.write.assets['font.ttf']);
  assert.equal(F.vectorRaster.detectNativePpem(ttf), 6);
  assert.throws(() => F.vectorRaster.detectNativePpem(ttf, { sfnt: { resources: { max_input_bytes: 10 } } }), fp.ResourceLimitError);
  assert.equal(F.vectorRaster.detect(ttf, 'ttf'), 'certain');
  assert.equal(F.vectorRaster.detect(ttf), 'certain');
  const { model } = F.vectorRaster.read(ttf);
  const viaRegistry = fp.read(ttf, { format: 'ttf' }).fonts;
  assert.deepEqual(model.toDict(), viaRegistry.toDict());
  const moved = UNI3.fonts.copy();
  const { data: owned } = F.vectorRaster.writeOwned(moved, 'ttf');
  assert.deepEqual(owned, ttf);
  assert.equal(moved.length, 0);
  assert.throws(() => F.vectorRaster.write(UNI3.fonts, 'bdf'), fp.InvalidInputError);
  const codec = F.vectorRaster.codec('woff2');
  assert.equal(codec.id, 'woff2');
  assert.equal(codec.write_options.woff2.quality, 11);
  const fixture = new Uint8Array(readFileSync(path.join(REPO, 'tests/data/compat/mini-circle-1.0-0.ttf')));
  assert.equal(F.vectorRaster.detectNativePpem(fixture), undefined);
  assert.throws(() => F.vectorRaster.read(fixture), fp.InvalidInputError);
  const { model: circle } = F.vectorRaster.read(fixture, { options: { ppem: 16 } });
  if (enabled(fp, 'yaff')) assert.equal(sha(yaff(circle)), GOLDEN.fixtures['ttf-mini-circle'].yaff);
  assert.deepEqual(F.vectorRaster.formatIds(), ['ttf', 'otf', 'woff', 'woff2', 'ttc', 'vector-raster'].filter((f) => enabled(fp, f)));
});

test('svg image helpers', { skip: needs('svg') }, () => {
  const F = fp.formats;
  const exclam = glyphFor(UNI3.fonts.font(0), 0x21);
  const svg = F.svg.image(exclam.bitmap);
  assert.ok(svg.startsWith('<svg') && svg.includes('viewBox="0 0 1 5"'));
  assert.equal(F.svg.image(exclam.bitmap), svg); // deterministic
  const dots = F.svg.image(exclam.bitmap, { shape: 'dot', dot_size: 0.5 });
  assert.ok(dots !== svg && dots.includes('a0.25 0.25 0 1 0')); // true circular dots use arcs
  assert.ok(F.svg.glyphImage(exclam).startsWith('<svg'));
  assert.equal(F.svg.pathData(exclam.bitmap, [0, 0], 10), 'M0 0V10H10V0ZM0 20V50H10V20Z');
  assert.throws(() => F.svg.pathData(exclam.bitmap, [0, 0], 0), fp.InvalidInputError);
  assert.throws(() => F.svg.pathData(exclam.bitmap, [0, 0], 1.5), fp.InvalidInputError);
  assert.throws(() => F.svg.image(exclam.bitmap, { dot_size: 2.0 }), fp.InvalidInputError);
  const gray = new fp.Bitmap(2, 2, 'gray8');
  assert.throws(() => F.svg.image(gray), fp.InvalidInputError);
});

test('fnt fbit nfnt hanfont hzk raw helpers', { skip: needs('fnt', 'fbit', 'nfnt', 'hanfont', 'hzk', 'raw') }, () => {
  const F = fp.formats;
  assert.equal(F.fnt.charsetName(0), 'cp1252');
  assert.equal(F.fnt.charsetName(77), 'macroman');
  assert.equal(F.fnt.charsetName(255), undefined);
  assert.throws(() => F.fnt.charsetName(256), fp.InvalidInputError);
  const fntBytes = single(fp.write(UNI3.fonts, 'fnt'));
  const { model: font } = F.fnt.readDefault(fntBytes);
  assert.deepEqual(yaff(font), yaff(F.fnt.read(fntBytes).model));
  // Hangul component slots (crates/fontpixel/tests/hanfont.rs).
  assert.deepEqual(F.hanfont.componentSlots(0xac00), [1, 161, null]);
  assert.deepEqual(F.hanfont.componentSlots(0xac00 + 1), [101, 205, 249]);
  assert.throws(() => F.hanfont.componentSlots(0xabff), fp.InvalidInputError);
  const presets = F.hzk.presets();
  assert.equal(presets.length, 25);
  assert.equal(presets[0], 'hzk12');
  assert.ok(presets.includes('asc48'));
  for (const p of presets) {
    assert.equal(F.hzk.presetName(p), p);
    const g = F.hzk.presetGeometry(p);
    assert.deepEqual(fp.normalize('formats::hzk::Geometry', g), g);
  }
  assert.deepEqual(F.hzk.presetGeometry('asc16'), { width: 8, height: 16, first_code: 0, last_code: 255, code_space: 'ascii', storage: 'rows' });
  assert.equal(F.raw.rawLayout(3, 6).width, 3);
  assert.equal(F.raw.rawLayout(3, 6).first_codepoint, 0);
  assert.equal(F.raw.presetLayout('vga16').height, 16);
  assert.equal(F.raw.presetLayout('spectrum').first_codepoint, 32);
  assert.throws(() => F.raw.rawLayout(-1, 6), fp.InvalidInputError);
  // fbit / nfnt bundles from the direct writers.
  const fbitSrc = GOLDEN.formats.fbit;
  const { model } = F.fbit.readContext(NATIVE.fbit());
  assert.equal(sha(yaff(model)), fbitSrc.source.yaff);
  assert.deepEqual(hashes(F.fbit.writeBundle(model)), fbitSrc.write.assets);
  const leg3 = fp.read(sourceFile('leg3'));
  const nfnt = F.nfnt.writeBundle(leg3.fonts, { primaryName: 'font.dfont' });
  assert.deepEqual(hashes(nfnt), GOLDEN.formats.nfnt.write.assets);
});
