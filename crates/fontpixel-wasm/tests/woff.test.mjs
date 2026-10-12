// woff: WOFF1/WOFF2 containers compared with the same Rust calls
// (expected.json), plus the WOFF2 byte identity with the native CLI on a
// stream large enough for the parallel Brotli layout.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { HAS_CLI, cliFiles, fixture, load } from './helpers.mjs';
import { CIRCLE, E, GOLDEN, assertBytes, h, needs, norm, throwsError, toHex } from './sfnt_support.mjs';

const fp = await load();
const skip = needs(fp, 'opentype');
const skipTtc = skip || needs(fp, 'sfnt_collections');

const AUX = {
  major_version: 1,
  minor_version: 2,
  metadata: new TextEncoder().encode('<?xml version="1.0"?><metadata version="1.0"/>'),
  private_data: new Uint8Array([1, 2, 3]),
};

test('woff1', { skip }, () => {
  const e = fp.woff.woff1.encode(GOLDEN);
  assertBytes(e.bytes, h('woff1.encode'));
  assert.deepEqual(norm(e.report), { entries: [] });
  const d = fp.woff.woff1.decode(e.bytes);
  assert.deepEqual(Object.keys(d).sort(), ['auxiliary', 'report', 'sfnt']);
  new fp.Sfnt(d.sfnt).validateChecksums();
  const ea = fp.woff.woff1.encode(GOLDEN, { compression_level: 6, auxiliary: AUX });
  assertBytes(ea.bytes, h('woff1.encode.aux'));
  const da = fp.woff.woff1.decode(ea.bytes);
  assertBytes(da.sfnt, h('woff1.decode.sfnt'));
  assert.deepEqual(da.auxiliary, AUX);
  // number-array byte blocks (the serde form) are accepted too
  const listed = { ...AUX, metadata: Array.from(AUX.metadata), private_data: Array.from(AUX.private_data) };
  assertBytes(fp.woff.woff1.encode(GOLDEN, { compression_level: 6, auxiliary: listed }).bytes, ea.bytes);
  // absent blocks come back as null
  assert.deepEqual(d.auxiliary, { major_version: 0, minor_version: 0, metadata: null, private_data: null });
});

test('woff2', { skip }, () => {
  const e = fp.woff.woff2.encode(GOLDEN);
  assertBytes(e.bytes, h('woff2.encode'));
  assertBytes(fp.woff.woff2.decode(e.bytes).sfnt, h('woff2.decode'));
  const q5 = fp.woff.woff2.encode(GOLDEN, {
    quality: 5,
    transform_glyf: false,
    transform_hmtx: false,
    compression_threads: 1,
    auxiliary: AUX,
  });
  assertBytes(q5.bytes, h('woff2.encode.q5'));
  assert.deepEqual(fp.woff.woff2.decode(q5.bytes).auxiliary, AUX);
  // jobs = 2: the native worker count; here the jobs run in input order
  const many = fp.woff.woff2.encodeMany([GOLDEN, CIRCLE], undefined, undefined, 2);
  assert.deepEqual(many.map((m) => toHex(m.bytes)), E['woff2.encode_many']);
  assert.deepEqual(
    fp.woff.woff2.encodeMany([GOLDEN, CIRCLE]).map((m) => toHex(m.bytes)),
    many.map((m) => toHex(m.bytes)),
  );
  // Admission bounds. The quality-11 empirical envelope does not depend on
  // the platform; the other bounds include Rust allocation sizes of the
  // target, so on wasm32 (32-bit usize and pointers) they are what this
  // module's encoder admits, at most the 64-bit native values.
  const [hq, hqProven, q5Bound, q5Proven] = E['woff2.brotli_bounds'];
  assert.equal(fp.woff.woff2.brotliEncoderHeapBound(11, 22, 1000, 2), hq);
  for (const [actual, native] of [
    [fp.woff.woff2.brotliEncoderProvenHeapBound(11, 22, 1000, 2), hqProven],
    [fp.woff.woff2.brotliEncoderHeapBound(5, 22, 4096, 1), q5Bound],
    [fp.woff.woff2.brotliEncoderProvenHeapBound(5, 22, 4096, 1), q5Proven],
  ]) {
    assert.ok(actual > 0 && actual <= native, `${actual} vs native ${native}`);
  }
  assert.equal(fp.woff.woff2.brotliEncoderHeapBound(5, 22, 4096, 1), fp.woff.woff2.brotliEncoderProvenHeapBound(5, 22, 4096, 1));
  // arithmetic overflow of usize: undefined (Python None)
  assert.equal(fp.woff.woff2.brotliEncoderHeapBound(11, 22, 2 ** 32 - 1, 2 ** 20), undefined);
  assert.equal(fp.woff.woff2.brotliEncoderProvenHeapBound(11, 22, 2 ** 32 - 1, 2 ** 20), undefined);
  // usize parameters are 32-bit here: larger values are rejected, not wrapped
  throwsError(() => fp.woff.woff2.brotliEncoderHeapBound(11, 22, 2 ** 32, 2), fp.InvalidInputError, { message: /out of range/ });
  throwsError(() => fp.woff.woff2.brotliEncoderHeapBound(11, 22, 2 ** 63, 2), fp.InvalidInputError);
});

test('woff2 collections', { skip: skipTtc }, () => {
  const { bytes: ttc } = fp.sfnt.collection.encode([GOLDEN, CIRCLE], 0x10000);
  const et = fp.woff.woff2.encode(ttc);
  assertBytes(et.bytes, h('woff2.encode.ttc'));
  const dt = fp.woff.woff2.decode(et.bytes);
  assertBytes(dt.sfnt, h('woff2.decode.ttc'));
  assert.equal(new fp.Collection(dt.sfnt).faces.length, 2);
});

test('woff errors and limits', { skip }, () => {
  const junk = new TextEncoder().encode('wOFFjunk');
  for (const mod of [fp.woff.woff1, fp.woff.woff2]) {
    throwsError(() => mod.decode(junk), fp.InvalidInputError, { kind: 'InvalidInput' });
    throwsError(() => mod.encode(new TextEncoder().encode('not an sfnt')), fp.InvalidInputError);
    throwsError(() => mod.encode(GOLDEN, undefined, { sfnt: { resources: { max_total_bytes: 100 } } }), fp.ResourceLimitError, {
      limit: 100,
    });
    throwsError(() => mod.encode(GOLDEN, { no_such_option: 1 }), fp.InvalidInputError, { message: /no_such_option/ });
  }
  throwsError(() => fp.woff.woff2.encode(GOLDEN, { quality: 12 }), fp.InvalidInputError);
  throwsError(() => fp.woff.woff1.encode(GOLDEN, { compression_level: 10 }), fp.InvalidInputError);
  throwsError(() => fp.woff.woff2.encodeMany([GOLDEN, CIRCLE], undefined, { max_batch_fonts: 1 }), fp.ResourceLimitError, {
    limit: 1,
  });
  throwsError(() => fp.woff.woff1.encode(GOLDEN, { auxiliary: AUX }, { max_auxiliary_bytes: 4 }), fp.ResourceLimitError);
  throwsError(() => fp.woff.woff2.encodeMany(GOLDEN), fp.InvalidInputError, { message: /array/ });
  throwsError(() => fp.woff.woff2.encodeMany([GOLDEN], undefined, undefined, 1.5), fp.InvalidInputError);
});

test('woff defaults', { skip }, () => {
  const d = fp.woff.defaults();
  assert.deepEqual(d['woff1.Options'], { compression_level: 9, auxiliary: d.Auxiliary });
  assert.equal(d['woff2.Options'].quality, 11);
  assert.equal(d['woff2.Options'].compression_threads, 2);
  assert.deepEqual(d.Auxiliary, { major_version: 0, minor_version: 0, metadata: null, private_data: null });
  assert.equal(d.Limits.max_batch_fonts, 1024);
  assertBytes(fp.woff.woff2.encode(GOLDEN, d['woff2.Options'], d.Limits).bytes, h('woff2.encode'));
});

// ------------------------------------------------- WOFF2 identity, large input

// `variants` sets the CJK OS/2 code-page bits only with the charset tables.
const skipTong =
  skip ||
  needs(fp, 'cjk_charsets') ||
  (!HAS_CLI && 'native CLI not found (FONTPIXEL_CLI)');
const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

// Native reference outputs and the WASM results, computed once per run.
let tong;
function tongResults() {
  if (!tong) {
    const bdf = fixture('tong-pixel-sans/TongPixelSans14Large-SC.bdf.gz');
    const native = (out) => {
      const cli = cliFiles({ 'sc.bdf': bdf }, ['variants', '--base', 'sc.bdf', '-o', out]);
      assert.equal(cli.code, 0, cli.stderr.slice(0, 2000));
      return cli.files[out];
    };
    const set = fp.read(bdf, { name: 'sc.bdf' }).fonts;
    let start = Date.now();
    const otf = new fp.Variants(set.font(0)).encode({ flavor: 'cff' });
    const variantsMs = Date.now() - start;
    start = Date.now();
    const woff2 = fp.woff.woff2.encode(otf.bytes);
    const woff2Ms = Date.now() - start;
    tong = { nativeOtf: native('sc.otf'), nativeWoff2: native('sc.woff2'), otf, woff2, variantsMs, woff2Ms };
  }
  return tong;
}

// Native: `fontpixel variants --base SC.bdf -o sc.otf` and `-o sc.woff2`
// (non-dot shapes put CFF outlines in WOFF2; WOFF2 defaults: quality 11, 2
// Brotli threads). The 5.6 MB sfnt is far above PARALLEL_MIN_BYTES
// (128 KiB), so the native encoder splits the stream across two threads and
// WASM runs the same chunks one after another.
test('woff2 on Tong Pixel Sans SC: same sfnt and same decoded font as the native CLI', { skip: skipTong, timeout: 600_000 }, (t) => {
  const { nativeOtf, nativeWoff2, otf, woff2, variantsMs, woff2Ms } = tongResults();
  assertBytes(otf.bytes, nativeOtf, 'variants CFF sfnt');
  assert.ok(otf.bytes.length >= 128 * 1024);
  assertBytes(fp.woff.woff2.decode(woff2.bytes).sfnt, fp.woff.woff2.decode(nativeWoff2).sfnt, 'decoded sfnt');
  t.diagnostic(
    `input sfnt ${otf.bytes.length} bytes sha256 ${sha256(otf.bytes)} (variants ${variantsMs} ms); ` +
      `wasm woff2 ${woff2.bytes.length} bytes sha256 ${sha256(woff2.bytes)} (${woff2Ms} ms); ` +
      `native woff2 ${nativeWoff2.length} bytes sha256 ${sha256(nativeWoff2)}`,
  );
});

// Both targets enable portable-float; compressed bytes must match too.
test(
  'woff2 byte identity with the native CLI on Tong Pixel Sans SC',
  {
    skip: skipTong,
    timeout: 600_000,
  },
  () => {
    const { nativeWoff2, woff2 } = tongResults();
    assertBytes(woff2.bytes, nativeWoff2, 'WOFF2 (2 Brotli threads natively, sequential chunks in WASM)');
  },
);
