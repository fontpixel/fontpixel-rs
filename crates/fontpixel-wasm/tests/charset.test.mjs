// Charsets: CharsetCodec, the registry table, canonicalization, GL
// detection, Windows charsets, kuten/EUC and the Unicode merge categories
// (ported from crates/fontpixel-py/tests/test_charset.py; expectations are
// the core's own fixtures in crates/fontpixel/tests/data).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { REPO, has, load } from './helpers.mjs';

const fp = await load();
const CJK = has(fp, 'cjk_charsets');
const CORE_DATA = path.join(REPO, 'crates', 'fontpixel', 'tests', 'data');
const fixture = (name) => JSON.parse(readFileSync(path.join(CORE_DATA, name), 'utf8'));
const isError = (Class, check) => (e) => {
  assert.ok(e instanceof Class, `expected ${Class.name}, got ${e?.constructor?.name}: ${e?.message}`);
  if (check) check(e);
  return true;
};

test('canonicalization matches the reference fixture', () => {
  const data = fixture('charset_canonical.json');
  for (const [name, sanitized, canonical] of data.names) {
    assert.equal(fp.sanitizeCharset(name, null), sanitized, name);
    assert.equal(fp.canonicalCharset(name), canonical, name);
  }
  for (const [registry, encoding, expected] of data.pairs) {
    const text = (v) => (v === null ? undefined : String(v));
    assert.equal(fp.sanitizeCharset(text(registry), text(encoding)), expected);
  }
  for (const [key, [reg, enc]] of data.props) assert.deepEqual(fp.charsetProps(key), [reg, enc]);
  assert.equal(fp.canonicalCharset('ISO8859-000000000000000000000000000000000001'), 'iso88591');
  const huge = '9'.repeat(1000);
  assert.equal(fp.sanitizeCharset('ISO8859', huge), `iso8859${huge}`);
  assert.equal(fp.canonicalCharset('ISO8859-١'), 'iso88591');
  assert.equal(fp.sanitizeCharset(), fp.sanitizeCharset(null, null));
});

test('charset table and codecs', () => {
  const table = fp.charsetTable();
  assert.equal(table.length, 62);
  for (const entry of table) {
    assert.equal(typeof entry.available, 'boolean');
    if (!entry.available) {
      // Only the CJK tables can be compiled out (minimal profile).
      assert.ok(!CJK, entry.key);
      assert.throws(
        () => fp.CharsetCodec.fromKey(entry.key),
        isError(fp.FeatureDisabledError, (e) => {
          assert.equal(e.kind, 'CharsetFeatureDisabled');
          assert.equal(e.charset, entry.key);
          assert.equal(e.feature, 'cjk-charsets');
          assert.equal(e.exitCode, 1);
        }),
      );
      continue;
    }
    const codec = fp.CharsetCodec.fromKey(entry.key);
    assert.ok(codec.known);
    assert.equal(codec.name, entry.key);
    assert.equal(codec.kind, entry.kind);
    assert.equal(codec.gl, entry.force_gl);
    assert.equal(codec.cp2char(65), 'A');
  }
  assert.equal(table.every((e) => e.available), CJK);
  const keys = new Set(table.map((e) => e.key));
  for (const k of ['iso88591', 'cp1252', 'cp932', 'gb18030']) assert.ok(keys.has(k), k);
  const cp932 = table.find((e) => e.key === 'cp932');
  assert.equal(cp932.kind, 'double_byte');
  assert.equal(cp932.python_codec, 'cp932');
  let codec;
  for (const name of ['HZ', 'iso2022jp', 'FontSpecific', 'KOI8-RU']) {
    codec = new fp.CharsetCodec(name);
    assert.ok(!codec.known);
    assert.equal(codec.cp2unicode(0x1f600), 0x1f600);
  }
  assert.match(String(codec), /^CharsetCodec\(name=/);
  assert.throws(() => codec.cp2unicode(-1), fp.InvalidInputError);
  assert.throws(() => codec.cp2unicode(2 ** 32), fp.InvalidInputError);
  assert.throws(() => codec.char2cp('ab'), fp.InvalidInputError);
});

function fnv(pairs) {
  let h = 0xcbf29ce484222325n;
  let n = 0;
  const buf = new DataView(new ArrayBuffer(8));
  for (const [key, value] of pairs) {
    buf.setUint32(0, key, true);
    buf.setUint32(4, value, true);
    for (let i = 0; i < 8; i++) h = ((h ^ BigInt(buf.getUint8(i))) * 0x100000001b3n) & 0xffffffffffffffffn;
    n++;
  }
  return [n, h.toString(16).padStart(16, '0')];
}

test('decode tables match python-generated digests', () => {
  const maps = fixture('charset_maps.json');
  for (const key of ['cp1252', 'iso88591', 'koi8r', 'cp437']) {
    const codec = fp.CharsetCodec.fromKey(key, false);
    const pairs = [];
    for (let c = 0; c < 0x10000; c++) {
      const u = codec.cp2unicode(c);
      if (u !== undefined) pairs.push([c, u]);
    }
    const expected = maps[key].decode;
    assert.deepEqual(fnv(pairs), [expected.count, expected.fnv1a64], key);
  }
});

test('single-byte strictness and exact variants', () => {
  const latin = new fp.CharsetCodec('ISO-8859-1');
  const windows = new fp.CharsetCodec('cp1252');
  assert.equal(latin.cp2char(0x80), '\x80');
  assert.equal(windows.cp2char(0x80), '€');
  assert.equal(windows.cp2char(0x81), undefined);
  assert.equal(windows.char2cp('\x81'), undefined);
  assert.equal(windows.char2cp('€'), 0x80);
  assert.equal(windows.unicode2cp(0x20ac), 0x80);
  assert.equal(latin.cp2char(256), undefined);
  const jis = new fp.CharsetCodec('jisx0201', true);
  assert.ok(!jis.gl);
  assert.equal(jis.char2cp('¥'), undefined);
  assert.equal(jis.cp2char(0x5c), '\\');
  assert.equal(jis.cp2char(0xa1), '｡');
});

test('cjk tables are compiled in per profile', () => {
  if (!CJK) {
    assert.throws(() => fp.requireCapability('cjk_charsets'), fp.UnsupportedError);
    for (const name of ['cp932', 'gb2312', 'gb18030', 'big5']) {
      assert.throws(() => new fp.CharsetCodec(name), isError(fp.FeatureDisabledError, (e) => assert.equal(e.kind, 'CharsetFeatureDisabled')));
    }
    assert.throws(() => fp.CharsetCodec.forCharset({ name: 'cp932', code_ranges: [] }), fp.FeatureDisabledError);
    return;
  }
  const cp932 = new fp.CharsetCodec('cp932');
  assert.equal(cp932.cp2char(0x8160), '～');
  assert.equal(cp932.char2cp('ⅰ'), 0xeeef);
  const sjis = new fp.CharsetCodec('shiftjis');
  assert.equal(sjis.cp2char(0x8160), '〜');
  const gl = new fp.CharsetCodec('gb2312', true);
  assert.ok(gl.gl);
  assert.equal(gl.cp2char(0x3021), '啊');
  assert.equal(gl.char2cp('啊'), 0x3021);
  const gb = new fp.CharsetCodec('gb18030');
  assert.equal(gb.kind, 'gb18030');
  assert.equal(gb.cp2char(0x81308130), '\x80');
  assert.equal(gb.char2cp('\u{10ffff}'), 0xe3329a35);
  let identity;
  for (const key of ['unicode', 'utf16be', 'unknown']) {
    identity = fp.CharsetCodec.fromKey(key);
    assert.equal(identity.cp2unicode(0xd800), 0xd800);
    assert.equal(identity.cp2char(0xd800), undefined);
  }
  assert.equal(identity.unicode2cp(0x110000), undefined);
});

test('font charset codec and canonical name', () => {
  const dict = new fp.Font().toDict();
  dict.charset = { registry: 'JISX0208.1983', encoding: '0', code_ranges: [] };
  const font = fp.Font.fromDict(dict);
  const name = fp.charsetCanonicalName(dict.charset);
  assert.equal(name, 'jisx0208');
  if (CJK) {
    const codec = fp.CharsetCodec.forFont(font);
    assert.equal(codec.name, name);
    assert.ok(codec.known);
    // A view inside a FontSet works the same.
    const set = new fp.FontSet();
    set.append(font);
    assert.equal(fp.CharsetCodec.forFont(set.font(0)).name, name);
  } else {
    assert.throws(() => fp.CharsetCodec.forFont(font), fp.FeatureDisabledError);
  }
  assert.equal(fp.charsetCanonicalName({ name: 'ISO-8859-1', code_ranges: [] }), 'iso88591');
  const codec = fp.CharsetCodec.forCharset({ name: 'cp1252', code_ranges: [] });
  assert.equal(codec.cp2char(0x80), '€');
  assert.throws(() => fp.charsetCanonicalName({ bogus: 1 }), fp.InvalidInputError);
  assert.throws(() => fp.CharsetCodec.forFont({}), Error);
});

test('gl detection, windows charsets and kuten', () => {
  assert.ok(fp.detectGl('gb2312', null, [0xc4e3, 65, 0x3021]));
  assert.ok(!fp.detectGl('gb2312', undefined, [0x0100, 0x3021]));
  assert.ok(fp.detectGl('gb2312', '0', []));
  assert.ok(!fp.detectGl('gbk', '0', [0x3021]));
  assert.ok(fp.detectGl('iso2022cn', null, []));
  assert.throws(() => fp.detectGl('gb2312', null, [-1]), fp.InvalidInputError);
  assert.ok(fp.isUnicodeCharset('utf32le'));
  assert.ok(!fp.isUnicodeCharset('utf9'));
  assert.deepEqual(fp.windowsCharset(0), { code_page: 'cp1252' });
  assert.equal(fp.windowsCharset(1), 'system_ansi');
  assert.equal(fp.windowsCharset(2), 'symbol');
  assert.equal(fp.windowsCharset(77), 'system_mac');
  assert.equal(fp.windowsCharset(255), 'system_oem');
  assert.deepEqual(fp.windowsCharset(128), { code_page: 'cp932' });
  assert.deepEqual(fp.windowsCharset(42), { unknown: 42 });
  assert.throws(() => fp.windowsCharset(256), fp.InvalidInputError);
  for (let row = 1; row < 95; row++) {
    for (let cell = 1; cell < 95; cell++) assert.deepEqual(fp.eucToKuten(fp.kutenToEuc(row, cell)), [row, cell]);
  }
  assert.equal(fp.kutenToEuc(16, 1), 0xb0a1);
  assert.equal(fp.kutenToEuc(0, 1), undefined);
  assert.equal(fp.kutenToEuc(1, 95), undefined);
  assert.equal(fp.eucToKuten(0x3021), undefined);
  assert.throws(() => fp.eucToKuten(0x10000), fp.InvalidInputError);
});

test('merge categories', () => {
  assert.equal(fp.unicodeVersion(), '17.0.0');
  for (const ranges of [fp.cjkRanges(), fp.geometricRanges(), fp.unicodeBlocks()]) {
    assert.ok(Array.isArray(ranges) && ranges.length > 0);
    for (let i = 1; i < ranges.length; i++) assert.ok(ranges[i - 1].end < ranges[i].start);
  }
  // Fresh arrays every call.
  const blocks = fp.unicodeBlocks();
  blocks.length = 0;
  assert.ok(fp.unicodeBlocks().length > 300);
  for (const r of fp.cjkRanges()) assert.ok(fp.isCjk(r.start) && fp.isCjk(r.end));
  const cps = [];
  for (let cp = 0; cp < 0x3000; cp += 7) cps.push(cp);
  cps.push(0xd800, 0xdfff, 0x110000, 0x4e00, 0x1f600);
  for (const cp of cps) {
    assert.equal(fp.isCjk(cp) || fp.isWestern(cp), fp.isScalar(cp));
    assert.ok(!(fp.isCjk(cp) && fp.isWestern(cp)));
  }
  for (const [start, end] of [[0x2500, 0x259f], [0x2800, 0x28ff], [0xe0a0, 0xe0d4]]) {
    assert.ok(fp.isGeometric(start) && fp.isGeometric(end));
    assert.ok(!fp.isGeometric(start - 1) && !fp.isGeometric(end + 1));
    assert.ok(fp.isWestern(start));
  }
  assert.ok(fp.isCjk(0xff00) && fp.isCjk(0x1f2ff));
  assert.ok(fp.isHan(0x323b0) && fp.isHan(0x3347f) && !fp.isHan(0x33480));
  assert.equal(fp.eastAsianWidth(0xff21), 'fullwidth');
  assert.equal(fp.eastAsianWidth(0xff66), 'halfwidth');
  assert.equal(fp.eastAsianWidth(0x4e00), 'wide');
  assert.equal(fp.eastAsianWidth(65), 'narrow');
  assert.equal(fp.eastAsianWidth(0xe0a0), 'ambiguous');
  assert.equal(fp.eastAsianWidth(0x0300), 'ambiguous');
  assert.equal(fp.eastAsianWidth(0xd800), undefined);
  assert.deepEqual(fp.unicodeBlock(65), { start: 0, end: 0x7f, name: 'Basic Latin' });
  assert.equal(fp.unicodeBlock(0x2fe0), undefined);
  assert.throws(() => fp.isCjk(-1), fp.InvalidInputError);
  assert.throws(() => fp.isScalar(2 ** 32), fp.InvalidInputError);
  assert.equal(fp.isScalar(2 ** 32 - 1), false);
});
