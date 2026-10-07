// fontpixel/compat/bdfparser: the bdfparser-js compatible API over
// `fontpixel::compat::bdfparser` (BdfparserRaw* handles + js/compat_bdfparser.js).
//
// Expected values come from the Rust compat goldens (tests/data/compat/, the
// same files the Python compat tests use) and bdfparser-js 3.0. The unmodified
// bdfparser-js suite runs through tests/compat_bdfparser/run_reference.mjs.
import assert from 'node:assert/strict';
import { readFileSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { pathToFileURL } from 'node:url';
import { gzipSync } from 'node:zlib';
import { DATA, PKG, has, load, runCli, tempDir, HAS_CLI } from './helpers.mjs';

const fp = await load();
const COMPAT = has(fp, 'compat_bdfparser');
const skip = COMPAT ? false : 'the compat_bdfparser capability is not in this build profile';
const bp = COMPAT ? await import(pathToFileURL(path.join(PKG, 'compat_bdfparser.js')).href) : null;
const cli = COMPAT ? await import(pathToFileURL(path.join(PKG, 'compat_bdfparser_cli.js')).href) : null;

const GOLD = path.join(DATA, 'compat');
const MINI = readFileSync(path.join(GOLD, 'mini.bdf'), 'utf8');
const mini = () => new bp.Font().load_string(MINI);

/** Collect the warnings `fn` emits (the previous handler is restored). */
const warningsOf = (fn) => {
  const out = [];
  const prev = bp.setWarningHandler((m) => out.push(m));
  try {
    fn();
  } finally {
    bp.setWarningHandler(prev);
  }
  return out;
};

test('compat: module surface', { skip }, async () => {
  assert.equal(bp.VERSION, '3.0.0');
  for (const name of ['Font', 'Glyph', 'Bitmap', 'BDFParseError', 'setWarningHandler', 'readPcf', 'writePcf', 'readTtf', 'writeTtf', 'bitmapContours', 'rasterize', 'getCodec', '$Font', 'fmtnum', 'defaultglyphname']) {
    assert.equal(typeof bp[name], 'function', name);
  }
  assert.equal(bp.META_TITLES.length, 17);
  assert.equal(bp.META_TITLES[0], 'glyphname');
  assert.equal(bp.META_TITLES[16], 'hexdata');
  assert.deepEqual([...bp.META_TITLES], fp.bdfparserMetaTitles());
  const pkg = JSON.parse(readFileSync(path.join(PKG, 'package.json'), 'utf8'));
  assert.equal(pkg.exports['./compat/bdfparser'].default, './compat_bdfparser.js');
  assert.equal(pkg.exports['./compat/bdfparser/cjk'].default, './compat_bdfparser_cjk.js');
  assert.equal(pkg.exports['./compat/bdfparser/cli'].default, './compat_bdfparser_cli.js');
  // the raw handles are classes of the package itself
  assert.ok(bp.rawHandle(mini()) instanceof fp.BdfparserRawFont);
  assert.ok(bp.rawHandle(new bp.Bitmap(['1'])) instanceof fp.BdfparserRawBitmap);
});

test('compat: defaultglyphname, fmtnum and fmtver', { skip }, () => {
  assert.equal(bp.defaultglyphname(65), 'uni0041');
  assert.equal(bp.defaultglyphname(0x1f600), 'u1F600');
  assert.equal(bp.defaultglyphname(65, false), 'char65');
  assert.equal(bp.defaultglyphname(-1), 'char-1');
  assert.equal(bp.fmtnum(3), '3');
  assert.equal(bp.fmtnum(-0), '0');
  assert.equal(bp.fmtnum(12.8), '12.8');
  assert.equal(bp.fmtnum(0.1 + 0.2), '0.30000000000000004');
  assert.equal(bp.fmtnum(0.00001), '1e-05');
  assert.equal(bp.fmtnum(2 ** 64), '18446744073709551616');
  assert.equal(bp.fmtnum(10n ** 30n), '1' + '0'.repeat(30));
  assert.equal(fp.bdfparserFmtnum(2.5), '2.5');
  assert.equal(fp.bdfparserFmtver(2), '2.0');
  assert.equal(fp.bdfparserFmtver(2.1), '2.1');
  assert.equal(fp.bdfparserFmtver(1e-7), '1e-07');
});

test('compat: bitmap construction and access', { skip }, () => {
  const b = new bp.Bitmap(['012', '100']);
  assert.deepEqual(b.bindata, ['012', '100']);
  assert.equal(b.width(), 3);
  assert.equal(b.height(), 2);
  assert.equal(b.toString(), '.#&\n#..');
  assert.equal(b.repr(), 'Bitmap([\n  "012",\n  "100"\n])');
  assert.equal(bp.rawHandle(b).repr(), "Bitmap(['012',\n        '100'])");
  assert.ok(b.equals(new bp.Bitmap(['012', '100'])));
  assert.ok(!b.equals(new bp.Bitmap(['000', '000'])));
  assert.deepEqual(Object.keys(b), ['bindata']);
  assert.deepEqual(bp.Bitmap.fromstring('\n.#&\n#..\n').bindata, ['012', '100']);
  assert.deepEqual(bp.Bitmap.blank(3, 2).bindata, ['000', '000']);
  assert.equal(bp.Bitmap.blank(7, 0).width(), 0);
  b.bindata = ['1'];
  assert.deepEqual(b.bindata, ['1']);
  const c = b.clone().setpixel(0, 0, 0);
  assert.deepEqual(b.bindata, ['1']);
  assert.deepEqual(c.bindata, ['0']);
  assert.deepEqual(bp.$Bitmap(['1']).bindata, ['1']);
  // the chaining methods return the bitmap itself
  const d = new bp.Bitmap(['01']);
  assert.equal(d.crop(2, 1), d);
  assert.equal(d.invert(), d);
  assert.equal(d.trim(), d);
});

test('compat: bitmap errors', { skip }, () => {
  assert.throws(() => new bp.Bitmap(['110', '1']), /different lengths/);
  assert.throws(() => new bp.Bitmap(['013']), /only '0', '1' and '2'/);
  assert.throws(() => bp.Bitmap.fromstring('.#\n.x'), { message: 'Invalid character "x" in bitmap row 1' });
  assert.throws(() => bp.Bitmap.blank(-1, 2), RangeError);
  assert.throws(() => new bp.Bitmap(['01']).getpixel(2, 0), RangeError);
  assert.throws(() => new bp.Bitmap(['01']).setpixel(0, 0, 3), { name: 'RangeError', message: 'Pixel value must be 0, 1 or 2, not 3' });
  assert.throws(() => new bp.Bitmap(['101']).crop(-1, 2), { name: 'RangeError', message: 'Bitmap.crop: negative width or height (-1, 2)' });
  const b = new bp.Bitmap(['11', '10']);
  assert.throws(() => b.replace('11', ''), /rectangular/); // a ragged result is refused, unchanged
  assert.deepEqual(b.bindata, ['11', '10']);
  assert.throws(() => b.bold(-1), RangeError);
  assert.throws(() => b.rotate(45), /multiple of 90/);
  assert.throws(() => new bp.Bitmap(['1'.repeat(54)]).todata(5), RangeError);
  assert.throws(() => new bp.Bitmap(['2']).todata(4), /Invalid binary string/);
  // the raw handle reports the native error class
  assert.throws(() => bp.rawHandle(new bp.Bitmap(['2'])).todata(4), fp.InvalidInputError);
  assert.throws(() => bp.rawHandle(new bp.Bitmap(['1'])).todata(9), /unknown bitmap datatype/);
});

test('compat: bitmap geometry', { skip }, () => {
  const T = (s) => bp.Bitmap.fromstring(s);
  assert.deepEqual(T('##.\n#..').crop(2, 1).bindata, ['10']);
  assert.deepEqual(T('##.\n#..').crop(4, 3, -1, 0).bindata, ['0000', '0110', '0100']);
  assert.deepEqual(T('#.\n..').overlay(T('.#\n#.')).bindata, ['11', '10']);
  assert.deepEqual(bp.Bitmap.concatall([T('#'), T('.\n#')]).bindata, ['00', '11']);
  assert.deepEqual(bp.Bitmap.concatall([T('#'), T('#')], { direction: 0 }).bindata, ['1', '1']);
  assert.deepEqual(bp.Bitmap.concatall([T('#'), T('#')], { direction: 1, align: 1, offsetlist: [1] }).bindata, ['101']);
  assert.deepEqual(T('#').concat(T('#'), { direction: 2 }).bindata, ['11']);
  assert.deepEqual(T('#.').enlarge(2, 1).bindata, ['1100']);
  assert.deepEqual(T('#.').replace(1, 2).bindata, ['20']);
  assert.deepEqual(T('#').shadow().bindata, ['10', '02']);
  assert.deepEqual(T('...\n.#.\n...').glow().bindata, ['00000', '00200', '02120', '00200', '00000']);
  assert.deepEqual(T('...\n.#.\n...').glow(1).bindata, ['00000', '02220', '02120', '02220', '00000']);
  assert.deepEqual(T('#').bytepad(4).bindata, ['1000']);
  assert.equal(T('.#&\n#..').getpixel(2, 0), 2);
  assert.deepEqual(T('##.\n#..').fliph().bindata, ['011', '001']);
  assert.deepEqual(T('##.\n#..').flipv().bindata, ['100', '110']);
  assert.deepEqual(T('##.\n#..').rotate().bindata, ['11', '01', '00']);
  assert.deepEqual(T('.#&').invert().bindata, ['100']);
  assert.deepEqual(T('#..\n...').shift(1, -1).bindata, ['000', '010']);
  assert.deepEqual(T('.....\n..#..\n.#&..\n.....').bbox(), [2, 2, 1, 1]);
  assert.deepEqual(T('.....\n..#..\n.#&..\n.....').trim().bindata, ['01', '12']);
  assert.deepEqual(T('#.#..').bold().bindata, ['111100']);
  assert.deepEqual(T('#\n#\n#\n#\n#').italic().bindata, ['001', '010', '010', '100', '100']);
  assert.deepEqual(bp.Bitmap.blank(4, 3).paste(T('#&\n.#'), 1, 1).bindata, ['0000', '0120', '0010']);
  const w = warningsOf(() => new bp.Bitmap(['1']).overlay(new bp.Bitmap(['11'])));
  assert.deepEqual(w, ['the bitmaps to overlay have different width']);
});

test('compat: bitmap data exports', { skip }, () => {
  const b = new bp.Bitmap(['01', '12']);
  assert.equal(b.todata(0), '01\n12');
  assert.deepEqual(b.todata(1), ['01', '12']);
  assert.deepEqual(b.todata(2), [[0, 1], [1, 2]]);
  assert.deepEqual(b.todata(3), [0, 1, 1, 2]);
  assert.deepEqual(new bp.Bitmap(['0101']).todata(4), ['5']);
  assert.deepEqual(new bp.Bitmap(['10011']).todata(5), [19]);
  assert.deepEqual(new bp.Bitmap(['1'.repeat(60)]).todata(4), ['f'.repeat(15)]);
  // beyond 2^53 the raw handle gives exact values (bigint / decimal text)
  const raw = bp.rawHandle(new bp.Bitmap(['1'.repeat(70)]));
  assert.deepEqual(raw.todata(5), [2n ** 70n - 1n]);
  assert.deepEqual(raw.rowNumbers(false), [(2n ** 70n - 1n).toString()]);
  assert.deepEqual(raw.rowNumbers(true), ['3' + 'f'.repeat(17)]);
  assert.deepEqual([...bp.rawHandle(new bp.Bitmap(['012'])).labels()], [0, 1, 2]);
  const one = bp.rawHandle(new bp.Bitmap(['012']));
  assert.deepEqual([...one.tobytes('L', null)], [255, 0, 127]);
  assert.deepEqual([...one.tobytes('1', null)], [0x9f]);
  assert.deepEqual([...one.tobytes('RGB', null)], [255, 255, 255, 0, 0, 0, 255, 0, 0]);
  assert.deepEqual([...one.tobytes('L', [[97], [98], [99]])], [97, 98, 99]);
  // draw2canvas: a canvas-like context
  const calls = [];
  const ctx = {
    set fillStyle(v) {
      calls.push(['style', v]);
    },
    fillRect: (...a) => calls.push(['rect', ...a]),
  };
  new bp.Bitmap(['012']).draw2canvas(ctx, { pixelsize: 2, x: 1 });
  assert.deepEqual(calls, [['style', 'black'], ['rect', 3, 0, 2, 2], ['style', 'red'], ['rect', 5, 0, 2, 2]]);
});

test('compat: bitmap svg', { skip }, () => {
  const b = new bp.Bitmap(['1']);
  assert.equal(b.tosvg(1, 'pathOnly'), 'M0 1V0H1V1Z');
  assert.equal(b.tosvg(), '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><path d="M0 1V0H1V1Z" fill="#000000" /></svg>');
  assert.equal(b.tosvg(1, 'rect'), '<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect x="0" y="0" width="1" height="1" fill="#000000" /></svg>');
  assert.equal(b.tosvg(1, 'pathOnly', false, {}, 'circle'), 'M0 0.5a0.5 0.5 0 1 0 1 0a0.5 0.5 0 1 0 -1 0Z');
  assert.equal(b.tosvg(2, 'pathOnly', true, { x: 1 }), 'M1 0V2H3V0Z');
  assert.equal(new bp.Bitmap([]).tosvg(1, 'pathOnly'), '');
  assert.throws(() => b.tosvg(1, 'png'), /Unknown SVG format/);
  assert.throws(() => b.tosvg(1, 'rect', false, {}, 'circle'), /only supports shape 'square'/);
});

test('compat: outline functions', { skip }, () => {
  assert.deepEqual(bp.bitmapContours([]), []);
  assert.deepEqual(bp.bitmapContours(['1']), [[[0, 0, true], [0, 1, true], [1, 1, true], [1, 0, true]]]);
  assert.deepEqual(bp.bitmapContours(['1'], { ppu: 64, xoff: 2, yoff: -3 })[0][0], [128, -192, true]);
  const circle = bp.bitmapContours(['1'], { shape: 'circle', ppu: 64 });
  assert.equal(circle.length, 1);
  assert.ok(circle[0].some(([, , on]) => !on));
  assert.throws(() => bp.bitmapContours(['1'], { shape: 'triangle' }), /Invalid shape/);
  assert.throws(() => bp.bitmapContours(['1'], { dotsize: 2 }), RangeError);
  const cs = bp.bitmapContours(['101', '111'], { ppu: 4 });
  assert.deepEqual(bp.rasterize(cs, 4, 0, 0, 3, 2), ['101', '111']);
  assert.deepEqual(fp.bdfparserRasterize(cs, 4, 0, 0, 3, 2).bindata(), ['101', '111']);
  assert.deepEqual(bp.rasterize(cs, 4, 0, 0, 0, 2), ['', '']);
});

test('compat: ttf and svg goldens match rust', { skip }, () => {
  const f = mini();
  assert.equal(f.tobdf(), MINI);
  assert.equal(f.tosvgfont(), readFileSync(path.join(GOLD, 'mini.svg'), 'utf8'));
  for (const rec of JSON.parse(readFileSync(path.join(GOLD, 'ttf.json'), 'utf8'))) {
    const o = rec.options;
    const data = f.tottf({ shape: o.shape, dotsize: o.dotsize, metadata: o.metadata });
    assert.deepEqual(data, new Uint8Array(readFileSync(path.join(GOLD, `${rec.name}.ttf`))), rec.name);
    assert.deepEqual(bp.writeTtf(f, { shape: o.shape, dotsize: o.dotsize, metadata: o.metadata }), data);
    const back = bp.readTtf(data, { pixelsize: 8 });
    assert.equal(back.tobdf(), readFileSync(path.join(GOLD, `${rec.name}.bdf`), 'utf8'), rec.name);
  }
  assert.deepEqual(fp.bdfparserWriteTtf(bp.rawHandle(f), { ppu: 64 }), f.tottf());
  assert.equal(fp.bdfparserReadTtf(f.tottf()).tobdf(), MINI);
  assert.throws(() => f.tottf({ ppu: 3 }), { name: 'RangeError', message: 'Invalid ppu 3: must be an even integer >= 2' });
  const plain = f.tottf({ metadata: false });
  assert.throws(() => bp.readTtf(plain, { pixelsize: 0 }), RangeError);
  assert.throws(() => bp.readTtf(plain), /pixelsize/);
  assert.throws(() => bp.readTtf(new Uint8Array([0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])), Error);
  assert.throws(() => bp.readTtf('x'), TypeError);
});

test('compat: load paths', { skip }, async () => {
  const bytes = new TextEncoder().encode(MINI);
  assert.equal(new bp.Font().load_bytes(bytes).tobdf(), MINI);
  assert.equal(new bp.Font().load_bytes(bytes.buffer).tobdf(), MINI);
  assert.equal((await new bp.Font().load_bytes_async(gzipSync(bytes))).tobdf(), MINI);
  assert.throws(() => new bp.Font().load_bytes(gzipSync(bytes)), /gzip-compressed data cannot be loaded synchronously/);
  assert.equal(bp.loadFontBytes(gzipSync(bytes)).tobdf(), MINI);
  assert.equal((await new bp.Font().load_filelines(MINI.split('\n'))).tobdf(), MINI);
  assert.equal((await bp.$Font(MINI)).tobdf(), MINI);
  assert.equal(new bp.Font().load_bytes(mini().tottf(), { pixelsize: 8 }).length, 3);
  assert.equal(new bp.Font().load_bytes(mini().topcf()).length, 3);
  assert.throws(() => new bp.Font().load_bytes(new TextEncoder().encode('OTTO....')), /CFF-based OpenType is not supported/);
  assert.throws(() => new bp.Font().load_bytes('x'), TypeError);
  const raw = new fp.BdfparserRawFont();
  raw.loadTtf(mini().tottf());
  assert.equal(raw.tobdf(), MINI);
  raw.loadLines(MINI.split('\n'));
  assert.equal(raw.tobdf(), MINI);
  let pos = 0;
  raw.loadFileObj({
    read: (n) => {
      const chunk = bytes.subarray(pos, pos + Math.min(n, 7));
      pos += chunk.length;
      return chunk;
    },
  });
  assert.equal(raw.tobdf(), MINI);
  const err = new Error('reader failed');
  assert.throws(() => raw.loadFileObj({ read: () => { throw err; } }), (e) => e === err);
  assert.throws(() => raw.loadFilePath('/tmp/x.bdf'), (e) => e instanceof fp.UnsupportedError && e.capability === 'filesystem');
});

test('compat: parse error translation', { skip }, () => {
  let err;
  try {
    new bp.Font().load_string('STARTFONT 2.1\nSIZE 8 75\n');
  } catch (e) {
    err = e;
  }
  assert.ok(err instanceof bp.BDFParseError && err instanceof Error);
  assert.equal(err.name, 'BDFParseError');
  assert.equal(err.lineno, 2);
  assert.equal(err.msg, 'SIZE needs 3 numbers, got "8 75"');
  assert.equal(err.message, 'line 2: SIZE needs 3 numbers, got "8 75"');
  // the native error is kept as the cause, with its structured fields
  assert.ok(err.cause instanceof fp.BdfParseError);
  assert.equal(err.kind, 'BdfParse');
  assert.equal(err.line, 2);
  assert.equal(new bp.BDFParseError('boom').lineno, null);
});

test('compat: warnings go to the handler', { skip }, () => {
  const text = MINI.replace('ENDPROPERTIES\n', 'ENDPROPERTIES\nFOO 1\n');
  let f;
  const w = warningsOf(() => {
    f = new bp.Font().load_string(text);
  });
  assert.deepEqual(w, ["line 14: unexpected line 'FOO 1' ignored"]);
  const report = bp.rawHandle(f).report();
  assert.ok(report.entries.some((e) => e.code === 'bdf.parse.unexpected_line'));
  assert.deepEqual(warningsOf(() => mini()), []); // info-level native notes are not warnings
  // bdfparser-js's glyphbycp / glyph do not warn; the raw Rust glyphbycp does
  assert.deepEqual(warningsOf(() => mini().glyphbycp(0x20ac)), []);
  const raw = bp.rawHandle(mini());
  assert.equal(raw.glyphbycp(0x20ac), undefined);
  assert.match(raw.warningsSince(0)[0][0], /does not exist in the font/);
  // the default handler writes to console.warn
  const prev = bp.setWarningHandler(undefined);
  assert.equal(typeof prev, 'function');
  assert.equal(bp.setWarningHandler(null), bp.defaultWarningHandler);
  bp.setWarningHandler(prev);
});

test('compat: create and core round trip', { skip }, () => {
  const f = bp.Font.create({ pixelsize: 12, ascent: 10, descent: 3, fontname: '-x-Foo', familyname: 'Foo', xres: 96, yres: 72 });
  const h = f.headers;
  assert.deepEqual([h.fontname, h.pointsize, h.xres, h.yres], ['-x-Foo', 12, 96, 72]);
  assert.deepEqual([h.fbbx, h.fbby, h.fbbxoff, h.fbbyoff], [12, 13, 0, -3]);
  assert.equal(f.props.family_name, 'Foo');
  assert.throws(() => fp.BdfparserRawFont.create({ pixelsize: 8, bogus: 1 }), fp.InvalidInputError);
  const core = bp.rawHandle(mini()).core();
  assert.ok(core instanceof fp.Font);
  assert.equal(fp.BdfparserRawFont.fromCore(core).tobdf(), MINI);
  assert.equal(bp.fromCore(core).tobdf(), MINI);
});

test('compat: headers and props views write through', { skip }, () => {
  const f = mini();
  const h = f.headers;
  assert.equal(h.fbbx, 6);
  assert.deepEqual({ ...h }, bp.rawHandle(f).headers());
  f.headers.fbbx = 9;
  assert.equal(h.fbbx, 9);
  assert.ok(f.tobdf().includes('FONTBOUNDINGBOX 9 8 0 -1'));
  delete f.headers.comment;
  assert.ok(!f.tobdf().includes('COMMENT "mini'));
  assert.equal(f.getprop('Pixel_Size'), 8);
  assert.equal(f.getprop('nope'), null);
  assert.equal(f.setprop('MinSpace', 2), f);
  assert.equal(f._propnames.get('minspace'), 'MinSpace');
  f.props.lower_key = 1;
  assert.ok(f.tobdf().includes('\nLOWER_KEY 1\n'));
  assert.equal(f.delprop('minspace'), f);
  assert.ok(!('minspace' in f.props));
  assert.equal(Object.getPrototypeOf(f.props), null);
  assert.equal(f.swidth(6), 720);
  // a float property is written unquoted, as bdfparser-js does
  f.setprop('X', 1.5);
  assert.ok(f.tobdf().includes('\nX 1.5\n'));
  // renaming through _propnames keeps the property order
  f._propnames.set('lower_key', 'Lower_Key');
  assert.ok(f.tobdf().includes('\nLower_Key 1\n'));
  // assigning a new props object keeps the remembered spellings
  f.props = { ...f.props, added: 'x' };
  assert.ok(f.tobdf().includes('\nLower_Key 1\n'));
  assert.ok(f.tobdf().includes('\nADDED "x"\n'));
});

test('compat: glyphs and unencoded views', { skip }, () => {
  const f = mini();
  assert.deepEqual([...f.glyphs.keys()], [32, 65, 66]);
  assert.equal(f.length, 3);
  assert.ok(f.glyphs instanceof Map);
  assert.equal(f.glyphs.size, 3);
  assert.equal(f.glyphs.get(65)[0], 'A');
  assert.equal(f.glyphs.get(65), f.glyphs.get(65));
  f.glyphs.get(65)[4] = 1; // item assignment is written back
  assert.equal(f.glyphbycp(65).meta.bbxoff, 1);
  f.glyphs.set(70, ['F', 70, 1, 1, 0, 0, null, null, 6, 0, null, null, null, null, null, null, ['80']]);
  assert.deepEqual(f.glyphs.get(70)[16], ['80']);
  assert.ok(f.glyphs.delete(70));
  assert.ok(!f.glyphs.has(70));
  f.unencoded.push({ altcode: 42, meta: ['u', -1, 0, 0, 0, 0, null, null, null, null, null, null, null, null, null, null, []] });
  assert.deepEqual(JSON.parse(JSON.stringify(f.unencoded)), [{ altcode: 42, meta: ['u', -1, 0, 0, 0, 0, null, null, null, null, null, null, null, null, null, null, []] }]);
  assert.ok(f.tobdf().includes('ENCODING -1 42'));
  f.unencoded[0].altcode = 7;
  assert.ok(f.tobdf().includes('ENCODING -1 7'));
  const model = bp.rawHandle(f).model();
  assert.deepEqual(model.glyphs.map(([cp]) => cp), [32, 65, 66]);
  assert.equal(model.unencoded[0][0], 7);
  const u = f.newglyph(-1, new bp.Bitmap(['1']));
  assert.deepEqual([...u._store], [...f.unencoded[1].meta]);
  f.unencoded.length = 0;
  assert.equal(f.unencoded.length, 0);
  assert.equal(u._store, null);
  assert.ok(!f.tobdf().includes('ENCODING -1'));
  f.glyphs.clear();
  assert.equal(f.glyphs.size, 0);
});

test('compat: charset encoding and converters', { skip }, () => {
  const f = mini();
  assert.equal(f.charset, 'iso10646');
  assert.equal(f.char2cp('A'), 65);
  assert.equal(f.cp2char(65), 'A');
  const raw = bp.rawHandle(f);
  assert.equal(raw.unicode2cp(0x41), 65);
  assert.equal(raw.cp2unicode(0xd800), 0xd800);
  assert.equal(raw.cp2char(0xd800), undefined);
  assert.equal(raw.char2cp('A'), 65);
  assert.throws(() => f.char2cp('AB'), TypeError);
  f.encoding({ char2cp: (c) => (c === 'a' ? 65 : null), cp2char: (cp) => (cp === 65 ? 'a' : null) });
  assert.equal(f.glyph('a').cp(), 65);
  assert.equal(f.cp2char(65), 'a');
  assert.equal(f.clone().char2cp('a'), 65);
  assert.deepEqual(f.lacksglyphs('ab'), ['b']);
  assert.equal(f.draw('a').width(), 6);
  const w = warningsOf(() => f.charsetconvert('iso10646'));
  assert.match(w[0], /could not be converted/);
  assert.equal(f.char2cp('a'), 97);
  const err = new Error('converter failed');
  const g = mini().encoding({
    char2cp: () => {
      throw err;
    },
  });
  assert.throws(() => g.draw('A'), (e) => e === err);
  // the Rust converter path: a JS exception is rethrown unchanged
  const r = bp.rawHandle(mini());
  r.setConverters(() => {
    throw err;
  }, undefined);
  assert.throws(() => r.draw('A', {}), (e) => e === err);
  const gb = mini();
  gb.encoding({ charset: 'GB2312' });
  assert.equal(gb.charset, 'gb2312');
  gb.charset = '';
  assert.equal(gb.charset, 'iso10646');
  const u = warningsOf(() => mini().charsetconvert('weird'));
  assert.match(u[0], /^Unknown charset 'weird'/);
});

test('compat: codec functions use the native tables', { skip }, () => {
  assert.equal(bp.canonicalCharset('Shift-JIS'), 'shiftjis');
  assert.equal(bp.sanitizeCharset('GB2312.1980', '0'), 'gb2312');
  assert.equal(bp.sanitizeCharset(), 'iso10646');
  assert.equal(bp.sanitizeCharsetName('-_utf32-'), '_utf32');
  assert.deepEqual(bp.charsetProps('koi8r'), ['KOI8', 'R']);
  assert.ok(bp.isUnicodeCharset('utf16le'));
  assert.equal(bp.charsetKeys().length, fp.bdfparserCharsetTable().length);
  const c = bp.getCodec('gb2312');
  assert.deepEqual([c.charset, c.kind, c.gl94, c.gl, c.known, c.exact], ['gb2312', 'dbcs', true, false, true, true]);
  const ah = String.fromCodePoint(0x554a);
  assert.equal(c.char2cp(ah), 0xb0a1);
  assert.equal(bp.getCodec('gb2312', { gl: true }).char2cp(ah), 0x3021);
  assert.equal(bp.cp2char(0xb0a1, 'GB2312'), ah);
  assert.equal(bp.char2cp(String.fromCodePoint(0x1f600), 'gb18030'), 0x9439fc36);
  const raw = fp.BdfparserRawCodec.open('gb18030');
  assert.equal(raw.cp2unicode(0x9439fc36), 0x1f600);
  assert.equal(raw.unicode2cp(0x1f600), 0x9439fc36);
  assert.equal(raw.cp2char(0x9439fc36), String.fromCodePoint(0x1f600));
  assert.equal(raw.char2cp('A'), 0x41);
  assert.ok(bp.detectGL('0', []));
  assert.ok(bp.detectGL(null, [0x41, 0xb0a1, 0x3021]));
  assert.ok(!bp.detectGL('1', [0x41]));
  assert.ok(fp.bdfparserDetectGl('gb2312', '0', []));
  assert.equal(bp.getCodec('hz').known, false);
  const w = warningsOf(() => bp.cp2char(0x4e2d, 'no-such-codec-name'));
  assert.deepEqual(w, ["Unknown charset 'nosuchcodecname', characters are mapped to codepoints as Unicode"]);
  assert.throws(() => bp.char2cp('ab', 'utf8'), TypeError);
  assert.doesNotThrow(() => bp.registerCharsetTables({ tables: {} }));
  assert.throws(() => bp.registerCharsetTables(null), TypeError);
});

test('compat: query and glyph handles', { skip }, () => {
  const f = mini();
  assert.deepEqual(f.itercps(), [32, 65, 66]);
  assert.deepEqual(f.itercps(2, [33, 70]), [66, 65]);
  assert.deepEqual(f.itercps(-1), [66, 65, 32]);
  assert.throws(() => f.itercps(5), /Unknown order 5/);
  assert.deepEqual(f.lacksglyphs('AxBx'), ['x', 'x']);
  assert.equal(f.lacksglyphs('AB'), null);
  assert.deepEqual([...f.iterglyphs(1, [[65, 65]])].map((g) => g.cp()), [65]);
  assert.equal(f.glyphbycp(0x20ac), null);
  const g = f.glyphbycp(65);
  assert.ok(g._attached);
  assert.equal(g.chr(), 'A');
  assert.equal(g.cp(), 65);
  assert.deepEqual(g.origin(), [0, 1]);
  assert.deepEqual(g.origin({ mode: 1, fromorigin: true }), [0, 0]);
  assert.equal(g.toString().split('\n')[0], '..#...');
  assert.equal(g.draw(1).bindata[0], '00100');
  assert.equal(g.draw(2).bindata[0], '00100000');
  assert.deepEqual(g.draw(-1, [2, 2, 0, 0]).bindata, ['10', '10']);
  g.shift(1, 2);
  assert.deepEqual(f.glyphs.get(65).slice(4, 6), [1, 2]);
  g.meta.dwx0 = 9; // a direct meta edit, written with the next editing method
  g.setmeta({ glyphname: 'A2' });
  assert.equal(f.glyphs.get(65)[0], 'A2');
  assert.equal(f.glyphs.get(65)[8], 9);
  assert.throws(() => g.setmeta({ bogus: 1 }), /Unknown glyph meta key "bogus"/);
  const c = g.clone();
  assert.ok(!c._attached);
  assert.equal(c.font, f);
  c.shift(5);
  assert.equal(f.glyphs.get(65)[4], 1);
  g.setbitmap(new bp.Bitmap(['11']), 1);
  assert.deepEqual(f.glyphs.get(65).slice(2, 4), [2, 1]);
  g.transform((b) => b.invert());
  assert.ok(g.repr().includes('Font(<-Test-Mini-Medium-R-Normal--8-80-75-75-C-60-ISO10646-1>)'));
  const raw = new fp.BdfparserRawGlyph({ codepoint: 1, hexdata: ['80'], bbw: 1, bbh: 1 }, bp.rawHandle(f));
  assert.ok(!raw.isAttached());
  assert.deepEqual(raw.meta()[16], ['80']);
  assert.equal(raw.cp(), 1);
  assert.equal(raw.font().length(), f.length);
  assert.deepEqual(raw.clone().meta(), raw.meta());
  assert.equal(raw.chr(), String.fromCodePoint(1));
  assert.deepEqual(raw.origin(1, true, undefined, undefined), [0, 0]);
  assert.match(raw.toString(), /#/);
  const att = bp.rawHandle(f).lookup(66);
  att.setmeta({ dwx0: 3 });
  att.shift(1, 0);
  att.setbitmap(bp.rawHandle(new bp.Bitmap(['1'])), 1, null, true);
  att.transform((b) => b.invert(), 1);
  assert.equal(f.glyphs.get(66)[8], 3);
});

test('compat: add, remove and new glyphs', { skip }, () => {
  const f = mini();
  const g = new bp.Glyph({ glyphname: 'F', codepoint: 70, bbw: 1, bbh: 1, hexdata: ['80'] }, f);
  assert.ok(!g._attached);
  f.addglyph(g);
  assert.ok(g._attached);
  assert.equal(f.glyphs.get(70)[0], 'F');
  f.removeglyph(g);
  assert.ok(!f.glyphs.has(70));
  assert.ok(!g._attached);
  assert.equal(g._store, null);
  f.addglyph({ codepoint: 71, hexdata: ['80'] });
  assert.equal(f.glyphs.get(71)[0], 'uni0047');
  const w = warningsOf(() => f.addglyph({ codepoint: 71, glyphname: 'x' }, false));
  assert.deepEqual(w, ['Glyph with codepoint 71 already exists, not replaced']);
  f.removeglyph(71).removeglyph(9999);
  const n = f.newglyph(72, new bp.Bitmap(['11', '01']), { mode: 1, name: 'H' });
  assert.ok(n._attached);
  assert.deepEqual(f.glyphs.get(72).slice(0, 4), ['H', 72, 2, 2]);
  assert.equal(f.glyphs.get(72)[8], 2);
  assert.equal(f.updatebbox(), f);
  const h = f.headers;
  assert.deepEqual([h.fbbx, h.fbby, h.fbbxoff, h.fbbyoff], [5, 7, 0, 0]);
  assert.throws(() => f.addglyph({ codepoint: 1, bogus: 2 }), /Unknown glyph meta key "bogus"/);
  assert.throws(() => f.addglyph({ glyphname: 'x' }), /The glyph has no codepoint/);
  const raw = bp.rawHandle(f);
  raw.addglyph({ codepoint: 73 }, true);
  raw.removeglyph(73);
  const rg = new fp.BdfparserRawGlyph({ codepoint: 74 }, raw);
  raw.addHandle(rg, true);
  assert.ok(rg.isAttached());
  raw.removeHandle(rg);
  assert.ok(!rg.isAttached());
  assert.equal(raw.newglyph(75, undefined, 0, undefined, 4).meta()[8], 4);
});

test('compat: clone, subset, merge, transform, charsetconvert', { skip }, () => {
  const f = mini();
  const c = f.clone();
  c.glyphbycp(65).shift(1);
  assert.equal(f.glyphs.get(65)[4], 0);
  assert.equal(c.glyphs.get(65)[4], 1);
  const s = f.subset([65, 66]);
  assert.deepEqual([...s.glyphs.keys()], [65, 66]);
  assert.equal(f.length, 3);
  const m = bp.Font.create({ pixelsize: 8 });
  m.merge(f);
  assert.deepEqual([...m.glyphs.keys()], [32, 65, 66]);
  const calls = [];
  f.transform((b) => {
    calls.push(b.width());
  }, [65, 66]);
  assert.deepEqual(calls, [6, 6]);
  const stop = new Error('stop');
  assert.throws(
    () =>
      f.transform(() => {
        throw stop;
      }),
    (e) => e === stop,
  );
  f.transform(() => new bp.Bitmap(['1']), [65, 65], 1);
  assert.deepEqual(f.glyphs.get(65).slice(2, 4), [1, 1]);
  const gb = mini();
  gb.charsetconvert('ISO8859-1');
  assert.equal(gb.props.charset_registry, 'ISO8859');
  assert.deepEqual([...gb.glyphs.keys()], [32, 65, 66]);
  assert.equal(bp.rawHandle(gb).charset(), 'iso88591');
  bp.rawHandle(gb).encoding('cp1252');
  assert.equal(bp.rawHandle(gb).charset(), 'cp1252');
  assert.equal(bp.rawHandle(gb).subset([65, 65]).length(), 1);
  bp.rawHandle(gb).merge(bp.rawHandle(mini()), true);
  bp.rawHandle(gb).charsetconvert('utf8');
  bp.rawHandle(gb).transform((b) => b.invert(), [65, 65], 0);
  bp.rawHandle(gb).updatebbox();
});

test('compat: drawing', { skip }, () => {
  const f = mini();
  assert.equal(f.draw('AB').bindata[3], '111110011100');
  assert.equal(f.drawcps([65, -1]).width(), 12); // a missing code draws a blank cell
  assert.equal(f.draw('A', { direction: 'tb' }).height(), 8);
  assert.equal(f.drawall().width(), 18);
  assert.equal(f.drawall({ r: [65, 65] }).width(), 6);
  assert.equal(f.draw('Z', { missing: { codepoint: -1, bbw: 1, bbh: 1, hexdata: ['80'], dwx0: 2 } }).width(), 6);
  const raw = bp.rawHandle(f);
  assert.equal(raw.drawcps([65], { mode: 0 }).width(), 6);
  assert.deepEqual(raw.draw('A', {}).bindata(), f.draw('A').bindata);
  assert.deepEqual(raw.drawall({ mode: 0 }).bindata(), f.drawall().bindata);
  assert.throws(() => f.draw('AAAA', { linelimit: 3 }), /`linelimit` \(3\) is too small/);
  assert.throws(() => new bp.Font().draw('A'), /Font is not loaded/);
});

test('compat: output and save', { skip }, () => {
  const f = mini();
  const data = f.topcf();
  assert.equal(String.fromCharCode(...data.subarray(0, 4)), '\x01fcp');
  assert.deepEqual(bp.writePcf(f), data);
  assert.ok(bp.readPcf(data).tobdf().startsWith('STARTFONT'));
  assert.ok(!bp.writePcf(f, { byteorder: 'lsb', bitorder: 'lsb', glyphpad: 1 }).every((b, i) => b === data[i]));
  assert.throws(() => f.topcf({ glyphpad: 3 }), { name: 'RangeError', message: 'Invalid PCF glyphpad: 3' });
  assert.throws(() => bp.readPcf(new TextEncoder().encode('not a pcf')), /bad magic/);
  assert.throws(() => bp.readPcf(gzipSync(data)), /readPcfAsync/);
  assert.deepEqual(bp.rawHandle(f).topcf({ byte_order: 'msb' }), data);
  assert.deepEqual(bp.rawHandle(f).tottf(), f.tottf());
  assert.equal(bp.rawHandle(f).tosvgfont(0, 0, false, 'decncr').includes('unicode="&#65;"'), true);
  assert.ok(f.tosvgfont({ unicodeAttr: { format: 'decncr' } }).includes('unicode="&#65;"'));
  const chunks = [];
  bp.rawHandle(f).saveTo({ write: (b) => chunks.push(b) }, 'bdf');
  assert.equal(new TextDecoder().decode(chunks[0]), MINI);
  const out = [];
  bp.rawHandle(f).saveTo({ write: (b) => out.push(b) }, 'pcf', { glyph_pad: '4' });
  assert.deepEqual(out[0], data);
  assert.throws(() => bp.rawHandle(f).saveTo({ write: () => {} }, 'png'), /Unknown output format/);
  assert.throws(() => bp.rawHandle(f).save('/tmp/x.bdf'), (e) => e instanceof fp.UnsupportedError);
  // the PCF writer's range guards are RangeErrors
  const g = mini();
  g.glyphs.get(65)[2] = -1;
  assert.throws(() => g.topcf(), { name: 'RangeError', message: 'Glyph "A" has a negative sized bitmap, -1x7' });
  g.glyphs.set(66, ['B', 66]);
  g.glyphs.get(65)[2] = 5;
  assert.throws(() => g.topcf(), /17-slot/);
});

test('compat: async readers', { skip }, async () => {
  const f = mini();
  const data = f.topcf();
  assert.equal((await bp.readPcfAsync(gzipSync(data))).length, 3);
  assert.equal((await bp.readPcfAsync(data)).length, 3);
  await assert.rejects(bp.readPcfAsync(gzipSync(data).subarray(0, 40)), /Cannot decompress gzip-compressed PCF/);
});

test('compat: header and unencoded editors keep handles', { skip }, () => {
  const f = mini();
  const g = f.glyphbycp(65);
  f.headers.fbbx = 9;
  delete f.headers.comment;
  const raw = bp.rawHandle(f);
  assert.equal(raw.removeHeader('nope'), false);
  g.shift(1);
  assert.ok(g._attached);
  assert.equal(f.glyphs.get(65)[4], 1);
  f.headers = { ...f.headers, dwx0: 3 };
  assert.equal(f.headers.dwx0, 3);
  assert.equal(f.headers.dwy0, null);
  assert.ok(g._attached);
  raw.setHeaders({ ...raw.headers(), fbbx: 6 });
  raw.setHeader('fbby', 8);
  assert.equal(raw.headers().fbbx, 6);
  assert.equal(raw.lookup(0x20ac), undefined);
  assert.equal(raw.lookup(65).meta()[4], 1);
  assert.deepEqual(raw.prepareMeta({ codepoint: -1 }).slice(0, 6), ['char-1', -1, 0, 0, 0, 0]);
  f.unencoded.push({ altcode: 42, meta: ['u', -1, 1, 1, 0, 0, null, null, null, null, null, null, null, null, null, null, ['80']] });
  assert.equal(raw.unencodedAltcode(0), 42);
  assert.equal(raw.unencodedAltcode(3), undefined);
  assert.equal(raw.unencodedHandle(1), undefined);
  const u = raw.unencodedHandles()[0];
  f.unencoded[0].meta[0] = 'named'; // written back through the slot's handle
  assert.ok(f.tobdf().includes('STARTCHAR named\nENCODING -1 42\n'));
  assert.ok(raw.setUnencodedMeta(0, { bbxoff: 2 }));
  assert.ok(raw.setUnencodedAltcode(0, 7));
  assert.ok(u.isAttached());
  assert.equal(raw.setUnencodedAltcode(3, 1), false);
  assert.equal(raw.setUnencodedMeta(3, {}), false);
  assert.equal(raw.removeUnencoded(3), false);
  raw.putWithAlt({ glyphname: 'v', codepoint: -1 }, 5);
  assert.ok(raw.removeUnencoded(0));
  assert.ok(!u.isAttached());
  assert.equal(raw.unencodedAltcode(0), 5);
  assert.ok(raw.removeUnencoded());
  assert.ok(g._attached); // unencoded edits never detach other handles
});

test('compat: cli main in-process', { skip }, () => {
  const { dir, cleanup } = tempDir('fontpixel-compat-cli-');
  try {
    const p = path.join(dir, 'mini.bdf');
    writeFileSync(p, MINI);
    let stdout = '';
    let stderr = '';
    const io = { stdout: (t) => (stdout += t), stderr: (t) => (stderr += t) };
    assert.equal(cli.main(['info', p], io), 0);
    assert.ok(stdout.includes('glyphs: 3 (+ 0 unencoded)'));
    assert.equal(cli.main(['convert', p, path.join(dir, 'o.ttf'), '--shape', 'circle'], io), 0);
    assert.deepEqual(new Uint8Array(readFileSync(path.join(dir, 'o.ttf'))), mini().tottf({ shape: 'circle' }));
    writeFileSync(path.join(dir, 'mini.pcf.gz'), gzipSync(mini().topcf()));
    assert.equal(cli.loadFontFile(path.join(dir, 'mini.pcf.gz')).length, 3);
    stdout = '';
    assert.equal(cli.main(['preview', p, 'A'], io), 0);
    assert.equal(stdout, mini().draw('A').toString() + '\n');
    assert.equal(cli.main(['info', path.join(dir, 'missing.bdf')], io), 1);
    assert.match(stderr, /ENOENT/);
    assert.equal(cli.main(['bogus'], io), 2);
    rmSync(path.join(dir, 'o.ttf'));
  } finally {
    cleanup();
  }
});

test('compat: native cli agrees on BDF and PCF', { skip: skip || (!HAS_CLI && 'the native CLI is not available') }, () => {
  // the compat model reads what the native CLI writes
  const { dir, cleanup } = tempDir('fontpixel-compat-native-');
  try {
    writeFileSync(path.join(dir, 'mini.bdf'), MINI);
    const r = runCli(['convert', 'mini.bdf', '-o', 'mini.pcf'], dir);
    assert.equal(r.code, 0, r.stderr);
    const native = new Uint8Array(readFileSync(path.join(dir, 'mini.pcf')));
    const f = bp.readPcf(native);
    assert.deepEqual([...f.glyphs.keys()].sort((a, b) => a - b), [32, 65, 66]);
    // the same pixels in the font box (the native writer may tighten glyph boxes)
    assert.deepEqual(f.glyphbycp(65).draw().bindata, mini().glyphbycp(65).draw().bindata);
  } finally {
    cleanup();
  }
});

test('compat: cjk module and capability', { skip }, async () => {
  const cjk = await import(pathToFileURL(path.join(PKG, 'compat_bdfparser_cjk.js')).href);
  assert.deepEqual(Object.keys(cjk.cjkTables), ['tables', 'gb18030']);
  assert.equal(bp.getCodec('cp936').exact, true);
  fp.requireCapability('compat_bdfparser');
});

test('compat: unavailable without the compat_bdfparser capability', { skip: COMPAT && 'this build has the compatibility layer' }, async () => {
  await assert.rejects(import(pathToFileURL(path.join(PKG, 'compat_bdfparser.js')).href), (e) => e instanceof fp.UnsupportedError && e.capability === 'compat_bdfparser');
});
