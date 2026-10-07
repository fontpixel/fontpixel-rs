// sfnt: container, builder, collections, CFF, subsetting and the table
// codecs, compared with the same Rust calls (expected.json) and the frozen
// CLI (cli.* keys), as the Python binding tests do.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { load, sample } from './helpers.mjs';
import { CIRCLE, E, GOLDEN, assertBytes, h, needs, norm, throwsError, toHex } from './sfnt_support.mjs';

const fp = await load();
const skip = needs(fp, 'opentype');
const skipTtc = skip || needs(fp, 'sfnt_collections');
const skipColor = skip || needs(fp, 'color_bitmaps');
const noCjk = needs(fp, 'cjk_charsets');
const S = skip ? null : new fp.Sfnt(GOLDEN);
const N = E['sfnt.num_glyphs'];

// ---------------------------------------------------------------- container

test('sfnt container', { skip }, () => {
  const s = new fp.Sfnt(GOLDEN);
  assert.deepEqual(fp.Sfnt.parse(GOLDEN).tables(), s.tables());
  assert.equal(s.flavor, 'true_type');
  assertBytes(s.data, GOLDEN);
  assert.deepEqual(s.limits, fp.sfnt.defaults().Limits);
  assert.deepEqual(
    s.tables().map((t) => [t.tag, t.checksum, t.offset, t.data.length]),
    E['sfnt.tables'],
  );
  for (const t of s.tables()) {
    assertBytes(s.table(t.tag), t.data, t.tag);
    assertBytes(s.table(new TextEncoder().encode(t.tag)), t.data, `${t.tag} as bytes`);
    assertBytes(t.data, GOLDEN.subarray(t.offset, t.offset + t.data.length), `${t.tag} slice`);
  }
  assert.equal(s.table('ZZZZ'), undefined);
  s.validateChecksums();
  assert.equal(String(s), `Sfnt(true_type, tables=11, bytes=${GOLDEN.length})`);
  // ArrayBuffer and DataView inputs behave like Uint8Array; the handle owns a copy
  const copy = GOLDEN.slice();
  const fromBuffer = new fp.Sfnt(copy.buffer);
  copy.fill(0);
  assert.deepEqual(fromBuffer.tables(), s.tables());
  assert.deepEqual(new fp.Sfnt(new DataView(GOLDEN.buffer)).tables(), s.tables());
  // results are fresh copies
  const data = s.data;
  data[0] ^= 0xff;
  assertBytes(s.data, GOLDEN);
});

test('sfnt builder and checksum', { skip }, () => {
  // Rust golden_container_reassembly_is_byte_exact
  const s = new fp.Sfnt(GOLDEN);
  const b = new fp.SfntBuilder(s.flavor);
  for (const t of s.tables().reverse()) b.insert(t.tag, t.data);
  assertBytes(b.build(), GOLDEN);
  assert.equal(fp.sfnt.checksum(GOLDEN), 0xb1b0afba);
  assert.equal(fp.sfnt.checksum(GOLDEN), E['sfnt.checksum']);
  // Rust checksum_padding_uses_big_endian_words
  assert.equal(fp.sfnt.checksum(new Uint8Array([1, 2, 3, 4, 5])), 0x06020304);
  assert.equal(fp.sfnt.checksum(new Uint8Array(8).fill(0xff)), 0xfffffffe);
  assert.equal(fp.sfnt.flavorVersion('cff'), E['sfnt.flavor_version.cff']);
  assert.equal(fp.sfnt.flavorVersion('cff'), 0x4f54544f);
  assert.equal(fp.sfnt.flavorVersion('true_type'), 0x10000);
  assert.equal(fp.sfnt.flavorVersion('apple_true_type'), 0x74727565);
  // unknown tables and every flavor rebuild with valid checksums
  for (const flavor of ['true_type', 'apple_true_type', 'cff']) {
    const builder = new fp.SfntBuilder(flavor);
    for (const t of s.tables()) builder.insert(t.tag, t.data);
    builder.insert('ZZZZ', new Uint8Array([1, 2, 3]));
    const out = new fp.Sfnt(builder.build());
    out.validateChecksums();
    assert.equal(out.flavor, flavor);
    assertBytes(out.table('ZZZZ'), new Uint8Array([1, 2, 3]));
  }
});

test('sfnt errors', { skip }, () => {
  for (let end = 0; end < 12; end++) {
    throwsError(() => new fp.Sfnt(GOLDEN.subarray(0, end)), fp.InvalidInputError, { kind: 'InvalidInput' });
  }
  throwsError(() => new fp.Sfnt(GOLDEN, { max_tables: 1 }), fp.ResourceLimitError, {
    kind: 'ResourceLimit',
    resource: 'sfnt tables',
    limit: 1,
  });
  throwsError(() => new fp.Sfnt(GOLDEN, { resources: { max_table_bytes: 1 } }), fp.ResourceLimitError, { limit: 1 });
  const bad = GOLDEN.slice();
  bad[16] ^= 1;
  throwsError(() => new fp.Sfnt(bad).validateChecksums(), fp.InvalidInputError);
  const ttc = GOLDEN.slice();
  ttc.set(new TextEncoder().encode('ttcf'), 0);
  throwsError(() => new fp.Sfnt(ttc), fp.InvalidInputError);
  throwsError(() => new fp.Sfnt(GOLDEN, { no_such_limit: 1 }), fp.InvalidInputError, { message: /no_such_limit/ });
  throwsError(() => new fp.Sfnt(GOLDEN).table('toolong'), fp.InvalidInputError);
  throwsError(() => new fp.Sfnt(GOLDEN).table('Āabc'), fp.InvalidInputError);
  throwsError(() => new fp.Sfnt(GOLDEN).table(new Uint8Array(3)), fp.InvalidInputError);
  throwsError(() => new fp.Sfnt([0, 1, 0, 0]), fp.InvalidInputError, { message: /Uint8Array/ });
  // Rust writer_rejects_duplicates_missing_head_and_budgets
  const b = new fp.SfntBuilder('cff');
  throwsError(() => b.build(), fp.InvalidInputError);
  b.insert('head', new Uint8Array(12));
  throwsError(() => b.insert('head', new Uint8Array(0)), fp.InvalidInputError);
  throwsError(() => b.insert(new Uint8Array(4), new Uint8Array(0)), fp.InvalidInputError);
  throwsError(() => b.build({ resources: { max_total_bytes: 20 } }), fp.ResourceLimitError, { limit: 20 });
  throwsError(() => new fp.SfntBuilder('opentype'), fp.InvalidInputError);
  // released handles, as receiver and as argument
  const freed = new fp.Sfnt(GOLDEN);
  freed.free();
  throwsError(() => freed.tables(), fp.InvalidInputError, { message: /Sfnt handle was released/ });
  throwsError(() => fp.sfnt.glyf.fromSfnt(freed), fp.InvalidInputError, { message: /Sfnt handle was released/ });
  throwsError(() => fp.Cff.fromSfnt(freed), fp.InvalidInputError, { message: /released/ });
  throwsError(() => fp.sfnt.glyf.fromSfnt(fp.Bitmap.fromText('#')), fp.InvalidInputError, {
    message: /expected a Sfnt handle or plain data, got a Bitmap handle/,
  });
  throwsError(() => fp.sfnt.glyf.fromSfnt({}), fp.InvalidInputError, { message: /expected an Sfnt handle/ });
});

test('sfnt defaults', { skip }, () => {
  const d = fp.sfnt.defaults();
  assert.equal(d.Limits.max_tables, 4095);
  assert.equal(d.Limits.max_cmap_mappings, 0x110000);
  assert.equal(d.Limits.max_name_records, 8192);
  assert.equal(d['gsub.LayoutLimits'].max_records, 1_000_000);
  assert.deepEqual(d['cff.DotOptions'], { cell_size: 100, diameter: 90 });
  assert.deepEqual(d['subset.Options'].codepoints, []);
  const keys = ['Limits', 'glyf.Limits', 'cff.Limits', 'cff.DotOptions', 'bitmap.Limits', 'gsub.LayoutLimits', 'subset.Options'];
  if (fp.Collection) keys.push('collection.Limits');
  assert.deepEqual(Object.keys(d).sort(), keys.sort());
  // every default object is accepted back
  new fp.Sfnt(GOLDEN, d.Limits);
  if (fp.Collection) new fp.Collection(fp.sfnt.collection.encode([GOLDEN], 0x10000, d['collection.Limits']).bytes);
  // integer arguments are exact and in range
  throwsError(() => fp.Cff.parse(new Uint8Array(4), 1000.5), fp.InvalidInputError, { message: /not an exact integer/ });
  throwsError(() => fp.sfnt.cmap.encode({ 65: 1 }, 70000), fp.InvalidInputError, { message: /out of range/ });
});

test('collection', { skip: skipTtc }, () => {
  const { bytes: data, report } = fp.sfnt.collection.encode([GOLDEN, CIRCLE], 0x10000);
  const [expBytes, expReport] = E['collection.encode'];
  assert.equal(toHex(data), expBytes);
  assert.deepEqual(norm(report), expReport);
  const c = new fp.Collection(data);
  assert.equal(fp.Collection.parse(data).version, 0x10000);
  assert.equal(c.version, 0x10000);
  assertBytes(c.data, data);
  assert.deepEqual(
    c.faces.map((f) => [f.flavor, f.offset, f.tables.length]),
    E['collection.faces'],
  );
  assert.equal(c.signature, undefined);
  c.validateChecksums();
  const one = c.extract(1);
  assert.deepEqual([toHex(one.bytes), norm(one.report)], E['collection.extract1']);
  new fp.Sfnt(one.bytes).validateChecksums();
  throwsError(() => c.extract(2), fp.InvalidInputError);
  throwsError(() => new fp.Collection(data, { max_faces: 1 }), fp.ResourceLimitError, { limit: 1 });
  throwsError(() => new fp.Collection(GOLDEN), fp.InvalidInputError);
  throwsError(() => fp.sfnt.collection.encode([new TextEncoder().encode('not a font')], 0x10000), fp.InvalidInputError);
  throwsError(() => fp.sfnt.collection.encode(GOLDEN, 0x10000), fp.InvalidInputError, { message: /array/ });
});

// ---------------------------------------------------------------- CFF

const otf = () => new fp.Sfnt(h('variants.otf'));

test('cff reader', { skip }, () => {
  const info = E['cff.info'];
  const c = fp.Cff.fromSfnt(otf());
  assert.equal(c.length, info.len);
  assert.equal(c.len(), info.len);
  assert.equal(c.isEmpty(), false);
  assert.equal(c.isCid(), info.is_cid);
  assert.equal(toHex(c.glyphName(1)), info.glyph_name1);
  assert.equal(c.glyphName(60000), undefined);
  assert.equal(c.identifier(1), info.identifier1);
  assert.equal(c.globalSubrCount(), info.global_subr_count);
  assert.equal(toHex(c.data), info.data);
  assert.deepEqual(norm(c.glyph(1)), E['cff.glyph1']);
  assert.deepEqual(norm(c.readAll()), E['cff.read_all']);
  assertBytes(c.subset([0, 1]), h('cff.subset01'));
  const raw = fp.Cff.parse(c.data, E['cff.upm']);
  assert.deepEqual(norm(raw.readAll()), E['cff.read_all']);
  throwsError(() => fp.Cff.fromSfnt(new fp.Sfnt(GOLDEN)), fp.InvalidInputError);
  throwsError(() => fp.Cff.parse(new Uint8Array([1, 0, 4]), 1000), fp.InvalidInputError);
  throwsError(() => c.readAll({ max_total_commands: 1 }), fp.ResourceLimitError);
  throwsError(() => c.glyph(60000), fp.InvalidInputError);
  throwsError(() => c.glyph(70000), fp.InvalidInputError, { message: /out of range/ });
});

test('cff writer', { skip }, () => {
  const e = fp.sfnt.cff.encode(E['cff.font']);
  const exp = E['cff.encode'];
  assert.equal(toHex(e.data), exp.data);
  assert.equal(toHex(e.maxp), exp.maxp);
  assert.deepEqual(e.bounds, exp.bounds);
  const parsed = fp.Cff.parse(e.data, 1000);
  assert.equal(parsed.length, 3);
  assert.deepEqual(Array.from(parsed.glyphName(1)), [0x41]);
  const bad = structuredClone(E['cff.font']);
  bad.glyphs = bad.glyphs.slice(1);
  throwsError(() => fp.sfnt.cff.encode(bad), fp.InvalidInputError);
  throwsError(() => fp.sfnt.cff.encode(E['cff.font'], { max_total_commands: 2 }), fp.ResourceLimitError);
  // outline.rasterize(glyf.contours.2, 64.0, (0, 0), 5, 7), as text (Rust result)
  const bitmap = fp.Bitmap.fromText(E['outline.rasterize']);
  assert.deepEqual(norm(fp.sfnt.cff.glyphFromBitmap('A', 600, bitmap)), E['cff.glyph_from_bitmap']);
  // plain GlyphBitmap data works like the handle
  assert.deepEqual(norm(fp.sfnt.cff.glyphFromBitmap('A', 600, bitmap.toDict())), E['cff.glyph_from_bitmap']);
  throwsError(() => fp.sfnt.cff.glyphFromBitmap('A', 600, bitmap, { units_per_pixel: 0 }), fp.InvalidInputError);
});

// ---------------------------------------------------------------- subset

test('subset', { skip }, () => {
  const [expBytes, expMapping, expReport] = E['subset.text_A'];
  assert.deepEqual(norm(fp.sfnt.subset.optionsText('BA')), E['subset.options_text']);
  const r = fp.sfnt.subset.subset(GOLDEN, fp.sfnt.subset.optionsText('A'));
  assert.equal(toHex(r.bytes), expBytes);
  assertBytes(r.bytes, h('cli.subset-A.ttf'), 'CLI subset --text A');
  assert.deepEqual(norm(r.glyph_mapping), expMapping);
  assert.deepEqual(norm(r.report), expReport);
  // CLI `subset --unicodes U+41-42 -o x.woff2`: subset, then WOFF2 defaults
  const ab = fp.sfnt.subset.subset(GOLDEN, { codepoints: [0x41, 0x42] });
  assertBytes(fp.woff.woff2.encode(ab.bytes).bytes, h('cli.subset-AB.woff2'), 'CLI subset -> woff2');
  // CLI `subset --unicodes U+42 -o x.woff`: subset, then WOFF1 defaults
  const b = fp.sfnt.subset.subset(GOLDEN, { codepoints: [0x42] });
  assertBytes(fp.woff.woff1.encode(b.bytes).bytes, h('cli.subset-B.woff'), 'CLI subset -> woff');
  throwsError(() => fp.sfnt.subset.subset(new TextEncoder().encode('junk'), fp.sfnt.subset.optionsText('BA')), fp.InvalidInputError);
  throwsError(() => fp.sfnt.subset.subset(GOLDEN, { drop_tables: ['toolong'] }), fp.InvalidInputError);
  throwsError(
    () => fp.sfnt.subset.subset(GOLDEN, { codepoints: [65], limits: { sfnt: { resources: { max_total_bytes: 64 } } } }),
    fp.ResourceLimitError,
    { limit: 64 },
  );
  throwsError(() => fp.sfnt.subset.subset(GOLDEN, { text: 'A' }), fp.InvalidInputError, { message: /unknown field `text`/ });
});

// ---------------------------------------------------------------- tables

test('cmap', { skip }, () => {
  assert.deepEqual(norm(fp.sfnt.cmap.decode(S.table('cmap'), N)), E['cmap.decode']);
  const mapping = { 32: 1, 65: 2, 66: 3, 0x1f600: 4 };
  const e = fp.sfnt.cmap.encode(mapping, 5);
  assert.deepEqual([toHex(e.data), e.has_bmp_subtable], E['cmap.encode']);
  assert.deepEqual(norm(fp.sfnt.cmap.decode(e.data, 5)), norm(mapping));
  // Maps with number keys are accepted too
  assertBytes(fp.sfnt.cmap.encode(new Map(Object.entries(mapping).map(([k, v]) => [Number(k), v])), 5).data, e.data);
  const bmp = { 32: 1, 65: 2 };
  assertBytes(fp.sfnt.cmap.encodeRecords(bmp, 3, true).data, h('cmap.encode_records.bmp_true'));
  assertBytes(fp.sfnt.cmap.encodeRecords(bmp, 3, false).data, h('cmap.encode_records.bmp_false'));
  assertBytes(fp.sfnt.cmap.encodeWithRecords(bmp, 3, 'when-needed').data, h('cmap.encode_with_records.when_needed'));
  assertBytes(fp.sfnt.cmap.encodeWithRecords(mapping, 5, 'non-bmp').data, h('cmap.encode_with_records.non_bmp'));
  throwsError(() => fp.sfnt.cmap.encodeWithRecords(bmp, 3, 'sometimes'), fp.InvalidInputError);
  const truncated = sample('sfnt/truncated-format4.cmap');
  throwsError(() => fp.sfnt.cmap.decode(truncated, 10), fp.InvalidInputError);
  throwsError(() => fp.sfnt.cmap.encode(mapping, 5, { max_cmap_mappings: 1 }), fp.ResourceLimitError, { limit: 1 });
  throwsError(() => fp.sfnt.cmap.encode({ 65: 9 }, 5), fp.InvalidInputError);
});

test('name', { skip }, () => {
  const t = fp.sfnt.name.decode(S.table('name'));
  assert.deepEqual(norm(t), E['name.decode']);
  assertBytes(fp.sfnt.name.encode(t), h('name.encode'));
  assertBytes(fp.sfnt.name.encodeConfigured(t), h('name.encode_configured'));
  fp.sfnt.name.validateConfigured(t);
  // counts the deserialized copy, spare capacity included (Rust: >= its own decode)
  assert.ok(fp.sfnt.name.retainedBytes(t, 1 << 20) > 0 && E['name.retained_bytes'] > 0);
  throwsError(() => fp.sfnt.name.retainedBytes(t, 8), fp.ResourceLimitError);
  const rec = fp.sfnt.name.recordUnicode(4, 0x409, 'Mini Test');
  assert.deepEqual(norm(rec), E['name.record_unicode']);
  assert.equal(fp.sfnt.name.recordText(rec), 'Mini Test');
  // record values accept Uint8Array as well as the serde number list
  assert.equal(fp.sfnt.name.recordText({ ...rec, value: Uint8Array.from(rec.value) }), 'Mini Test');
  throwsError(() => fp.sfnt.name.validateConfigured({ ...t, records: [...t.records, t.records[0]] }), fp.InvalidInputError);
  assert.equal(fp.sfnt.name.languageTagWellFormed('zh-Hant-HK'), true);
  assert.equal(fp.sfnt.name.languageTagWellFormed('zh_HK'), false);
  throwsError(() => fp.sfnt.name.decode(S.table('name'), { max_name_records: 1 }), fp.ResourceLimitError);
});

test('os2', { skip }, () => {
  const t = fp.sfnt.os2.decode(S.table('OS/2'));
  const before = structuredClone(t);
  assert.deepEqual(norm(t), E['os2.decode']);
  assertBytes(fp.sfnt.os2.encode(t), h('os2.encode'));
  assert.deepEqual(norm(fp.sfnt.os2.unicodeRanges([65, 0x4e00, 0x1f600])), E['os2.unicode_ranges']);
  assert.deepEqual(norm(fp.sfnt.os2.unicodeRanges(new Set([65, 0x4e00, 0x1f600]))), E['os2.unicode_ranges']);
  if (noCjk) throwsError(() => fp.sfnt.os2.codePageRanges([65, 0x4e00], [1252, 936]), fp.InvalidInputError);
  else assert.deepEqual(norm(fp.sfnt.os2.codePageRanges([65, 0x4e00], [1252, 936])), E['os2.code_page_ranges']);
  assert.equal(fp.sfnt.os2.averageWidth([500, 600, 0, 700]), E['os2.average_width']);
  assert.deepEqual(norm(fp.sfnt.os2.setUnicodeCoverage(t, new Set([65, 0x4e00]))), E['os2.set_unicode_coverage']);
  assert.deepEqual(t, before); // input object untouched
  const none = (v) => (v === undefined ? null : v);
  assert.deepEqual([0, 2, 3, 0x0100, 0x0302].map((f) => none(fp.sfnt.os2.checkEmbeddingFlags(f))), E['os2.check_embedding_flags']);
  assert.deepEqual(
    [[0x40, 3], [0x80, 3], [0x21, 4], [0x41, 3]].map(([f, v]) => none(fp.sfnt.os2.checkSelectionFlags(f, v))),
    E['os2.check_selection_flags'],
  );
  assert.deepEqual(
    [[0, 0], [1, 0], [0, 0x80000000]].map((r) => none(fp.sfnt.os2.checkCodePageRanges(r))),
    E['os2.check_code_page_ranges'],
  );
  throwsError(() => fp.sfnt.os2.decode(S.table('OS/2').subarray(0, 10)), fp.InvalidInputError);
  throwsError(() => fp.sfnt.os2.encode({ ...t, vendor_id: 'TOOLONG' }), fp.InvalidInputError);
});

test('post metrics gasp', { skip }, () => {
  const p = fp.sfnt.post.decode(S.table('post'), N);
  assert.deepEqual(norm(p), E['post.decode']);
  assertBytes(fp.sfnt.post.encode(p, N), h('post.encode'));
  const { header: hdr, metrics: mets } = fp.sfnt.metrics.decode('horizontal', S.table('hhea'), S.table('hmtx'), N);
  assert.deepEqual([norm(hdr), norm(mets)], E['metrics.decode']);
  const e = fp.sfnt.metrics.encode('horizontal', hdr, mets, true);
  assert.deepEqual([toHex(e.header), toHex(e.metrics), e.num_long_metrics], E['metrics.encode']);
  assert.deepEqual(norm(fp.sfnt.metrics.headerNew('vertical')), E['metrics.header_new.vertical']);
  assert.deepEqual(norm(fp.sfnt.metrics.headerRecompute(hdr, mets, E['metrics.ink'])), E['metrics.header_recompute']);
  throwsError(() => fp.sfnt.metrics.decode('horizontal', S.table('hhea'), S.table('hmtx').subarray(0, 2), N), fp.InvalidInputError);
  throwsError(() => fp.sfnt.metrics.headerNew('diagonal'), fp.InvalidInputError);
  const vt = new fp.Sfnt(h('variants.ttf'));
  const g = fp.sfnt.gasp.decode(vt.table('gasp'));
  assert.deepEqual(norm(g), E['gasp.decode']);
  assertBytes(fp.sfnt.gasp.encode(g), h('gasp.encode'));
  throwsError(() => fp.sfnt.gasp.decode(new Uint8Array([0])), fp.InvalidInputError);
});

test('gsub', { skip }, () => {
  const vt = new fp.Sfnt(h('variants.ttf'));
  const n = E['variants.ttf.num_glyphs'];
  const g = fp.sfnt.gsub.decode(vt.table('GSUB'), n);
  assert.deepEqual(norm(g), E['gsub.decode']);
  assertBytes(fp.sfnt.gsub.encode(g, n), h('gsub.encode'));
  assertBytes(fp.sfnt.gsub.encode(g, n), vt.table('GSUB'));
  assert.deepEqual(fp.sfnt.gsub.closure(g, new Set(E['gsub.closure.input'])), E['gsub.closure']);
  assert.deepEqual(fp.sfnt.gsub.closure(g, E['gsub.closure.input']), E['gsub.closure']);
  const identity = Object.fromEntries(Array.from({ length: n }, (_, i) => [i, i]));
  assert.deepEqual(norm(fp.sfnt.gsub.subset(g, identity)), E['gsub.subset']);
  throwsError(() => fp.sfnt.gsub.decode(vt.table('GSUB'), 1), fp.InvalidInputError);
  throwsError(() => fp.sfnt.gsub.decode(vt.table('GSUB'), n, { max_records: 1 }), fp.ResourceLimitError);
});

test('glyf', { skip }, () => {
  const t = fp.sfnt.glyf.fromSfnt(S);
  assert.deepEqual(norm(t), E['glyf.from_sfnt']);
  const head = S.table('head');
  const locFormat = new DataView(head.buffer, head.byteOffset).getInt16(50);
  assert.deepEqual(norm(fp.sfnt.glyf.decode(S.table('glyf'), S.table('loca'), N, locFormat)), E['glyf.from_sfnt']);
  const e = fp.sfnt.glyf.encode(t, 'auto');
  const exp = E['glyf.encode'];
  assert.equal(toHex(e.glyf), exp.glyf);
  assert.equal(toHex(e.loca), exp.loca);
  assert.equal(e.index_to_loc_format, exp.index_to_loc_format);
  assert.deepEqual(norm(e.bounds), exp.bounds);
  assert.deepEqual(norm(e.statistics), exp.statistics);
  assertBytes(fp.sfnt.glyf.unhintedMaxp(e.statistics, N), h('glyf.unhinted_maxp'));
  assert.deepEqual(norm(fp.sfnt.glyf.contours(t, 2)), E['glyf.contours.2']);
  assert.deepEqual(norm(fp.sfnt.glyf.componentTranslated(3, 10, -20)), E['glyf.component_translated']);
  assert.deepEqual(
    norm(fp.sfnt.glyf.glyphSimple([[[0, 0, true], [100, 0, true], [100, 100, true]]])),
    E['glyf.glyph_simple'],
  );
  // outline.rasterize(glyf.contours.2, 64.0, (0, 0), 5, 7) as text (Rust result)
  const bm = fp.Bitmap.fromText(E['outline.rasterize']);
  assert.deepEqual(norm(fp.sfnt.glyf.glyphPixels(bm)), E['glyf.glyph_pixels']);
  assert.deepEqual(norm(fp.sfnt.glyf.glyphPixels(bm, { shape: 'dot', dot_size: 0.9 }, 7)), E['glyf.glyph_pixels.dot']);
  throwsError(() => fp.sfnt.glyf.glyphPixels(new fp.Bitmap(2, 2, 'gray8')), fp.InvalidInputError, { message: /Mono/ });
  throwsError(() => fp.sfnt.glyf.contours(t, 999), fp.InvalidInputError);
  throwsError(() => fp.sfnt.glyf.fromSfnt(S, { max_total_points: 1 }), fp.ResourceLimitError);
  throwsError(() => fp.sfnt.glyf.decode(S.table('glyf'), S.table('loca').subarray(0, 4), N, locFormat), fp.InvalidInputError);
});

test('bitmap', { skip }, () => {
  const es = new fp.Sfnt(h('variants.embed'));
  const n = E['bitmap.num_glyphs'];
  const strikes = fp.sfnt.bitmap.decode(es.table('EBLC'), es.table('EBDT'), 'open_type', n);
  assert.deepEqual(norm(strikes), E['bitmap.decode']);
  const t = fp.sfnt.bitmap.encode(strikes, 'open_type', n);
  assert.deepEqual([toHex(t.data), toHex(t.location)], E['bitmap.encode']);
  const s = fp.sfnt.bitmap.encodeSpanning(strikes, 'apple', n);
  assert.deepEqual([toHex(s.data), toHex(s.location)], E['bitmap.encode_spanning.apple']);
  const r = fp.sfnt.bitmap.encodeSharedMetricRuns(strikes, 'open_type', n, 2);
  assert.deepEqual([toHex(r.tables.data), toHex(r.tables.location), r.shared_runs], E['bitmap.runs']);
  assertBytes(fp.sfnt.bitmap.encodeScaling([E['bitmap.scale']]), h('bitmap.encode_scaling'));
  assert.deepEqual(norm(fp.sfnt.bitmap.decodeScaling(h('bitmap.encode_scaling'))), [E['bitmap.scale']]);
  assert.deepEqual(
    [fp.sfnt.bitmap.lineAscender(strikes[0].horizontal), fp.sfnt.bitmap.lineDescender(strikes[0].horizontal)],
    E['bitmap.line_metrics'],
  );
  assert.deepEqual(fp.sfnt.bitmap.kindTags('open_type'), ['EBDT', 'EBLC']);
  assert.deepEqual(fp.sfnt.bitmap.kindTags('apple'), ['bdat', 'bloc']);
  assert.deepEqual(fp.sfnt.bitmap.kindTags('color'), ['CBDT', 'CBLC']);
  throwsError(() => fp.sfnt.bitmap.decode(es.table('EBLC').subarray(0, 8), es.table('EBDT'), 'open_type', n), fp.InvalidInputError);
  throwsError(
    () => fp.sfnt.bitmap.encode(strikes, 'open_type', n, { sfnt: { resources: { max_total_bytes: 16 } } }),
    fp.ResourceLimitError,
  );
});

test('bitmap color', { skip: skipColor }, () => {
  const es = new fp.Sfnt(h('variants.embed'));
  const n = E['bitmap.num_glyphs'];
  assert.deepEqual(norm(fp.sfnt.bitmap.decodeWithColor(es.table('EBLC'), es.table('EBDT'), 'open_type', n, false)), E['bitmap.decode']);
  // integer keys of the serde map: decimal-string object keys (or a Map)
  assertBytes(fp.sfnt.bitmap.encodeSbix(E['sbix.input'], 3), h('sbix.encode'));
  assert.deepEqual(norm(fp.sfnt.bitmap.decodeSbix(h('sbix.encode'), 3)), E['sbix.decode']);
  assert.deepEqual(norm(fp.sfnt.bitmap.decodeSbix(h('sbix.encode'), 3, false)), E['sbix.decode']);
});

test('profile gating of sfnt namespaces', () => {
  // The namespaces exist exactly where the capabilities say so.
  assert.equal(typeof fp.sfnt === 'object', !skip);
  assert.equal(typeof fp.woff === 'object', !skip);
  assert.equal(typeof fp.variants === 'object', !skip);
  assert.equal(typeof fp.Collection === 'function', !skipTtc);
  assert.equal(typeof fp.sfnt?.collection?.encode === 'function', !skipTtc);
  assert.equal(typeof fp.sfnt?.bitmap?.decodeSbix === 'function', !skipColor);
  for (const name of ['opentype', 'sfnt_collections', 'color_bitmaps']) {
    const cap = fp.capabilities().find((c) => c.name === name);
    assert.ok(cap, name);
    if (!cap.available) {
      assert.throws(() => fp.requireCapability(name), (e) => e instanceof fp.UnsupportedError && e.capability === name);
      assert.ok(cap.reason && cap.evidence && cap.alternative);
    }
  }
});
