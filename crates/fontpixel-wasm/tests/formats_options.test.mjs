// Typed options as plain data: normalization of every serde type, typed
// registry setters and codecs (a port of
// crates/fontpixel-py/tests/test_formats_options.py), plus the generated
// tables' up-to-date checks.
import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { REPO, load } from './helpers.mjs';
import { PROFILE, assetsOf, caseOf, enabled, sourceFile } from './formats_support.mjs';

const fp = await load();
const TYPES = fp.optionTypes();
// Plain-data types whose fields have no defaults, with a valid sample.
const SAMPLES = {
  'formats::WriteOptions': { primary_name: 'font.bdf' },
  WriteOptions: { primary_name: 'font.bdf' },
  'resources::ResourcePath': 'dir/font.bin',
  ResourcePath: 'dir/font.bin',
  'resources::OutputAsset': { path: 'a.bin', bytes: [1, 2] },
  OutputAsset: { path: 'a.bin', bytes: [1, 2] },
  'resources::OutputBundle': { primary: 'a.bin', assets: [{ path: 'a.bin', bytes: [1] }], directory: false },
  'report::Report': { entries: [] },
  Report: { entries: [] },
  'formats::raw::RawLayout': null,
  'formats::hzk::Geometry': null,
};

const plainObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);

test('every format option type is listed', () => {
  assert.equal(new Set(TYPES).size, TYPES.length);
  if (PROFILE !== 'full') {
    // Smaller builds list the types of their formats only.
    assert.ok(TYPES.includes('resources::ResourceLimits') && TYPES.includes('model::FontMetrics'));
    for (const t of TYPES.filter((t) => t.startsWith('formats::') && t.split('::').length > 3)) {
      const mod = t.split('::')[1];
      assert.ok(['bdf', 'pcf', 'yaff', 'embedded', 'vector_raster', 'x11', 'tex_common'].includes(mod) || enabled(fp, mod), t);
    }
    return;
  }
  // Every row of the Python normalize table (formats and resources) is here
  // under the same key; the JS table adds every other serde type and the
  // public re-export paths (SerdeTypes keys).
  const python = [...readFileSync(path.join(REPO, 'crates/fontpixel-py/src/formats_types.rs'), 'utf8').matchAll(/^\s+"([^"]+)" =>/gm)].map((m) => m[1]);
  assert.equal(python.length, 374);
  for (const t of python) assert.ok(TYPES.includes(t), t);
  for (const mod of fp.formats.modules()) {
    if (['x11', 'embedded', 'vectorRaster', 'bbc'].includes(mod)) continue;
    assert.ok(TYPES.some((t) => t.startsWith(`formats::${mod}::`)), mod);
  }
  for (const name of ['ReadOptions', 'WriteOptions', 'Confidence', 'Options', 'EmbeddedWriteOptions']) {
    assert.ok(TYPES.some((t) => t.endsWith(`::${name}`)), name);
  }
  for (const t of ['fontpixel_ops::RenderRequest', 'sfnt::gsub::Gsub', 'model::Font', 'merge::MergeOptions', 'bitmap::ByteLayout']) {
    assert.ok(TYPES.includes(t), t);
  }
  assert.deepEqual(fp.formats.optionTypes(), TYPES);
});

for (const typeName of TYPES) {
  test(`option type round trips and rejects unknown: ${typeName}`, () => {
    let defaults;
    try {
      defaults = fp.normalize(typeName);
    } catch (e) {
      assert.ok(e instanceof fp.InvalidInputError, `${typeName}: ${e}`);
      let bad;
      try {
        fp.normalize(typeName, 'definitely-not-a-variant');
      } catch (err) {
        bad = err;
      }
      if (bad && /unknown variant/.test(bad.message)) {
        // Enums: every unit variant named in the error round-trips.
        const variants = [...bad.message.split('expected', 2)[1].matchAll(/`([^`]+)`/g)].map((m) => m[1]);
        assert.ok(variants.length, bad.message);
        for (const v of variants) {
          let out;
          try {
            out = fp.normalize(typeName, v);
          } catch (dataVariant) {
            // Variants carrying data need an object, not a name.
            assert.ok(dataVariant instanceof fp.InvalidInputError && /invalid type|expected/.test(dataVariant.message), dataVariant.message);
            continue;
          }
          // Aliases (e.g. raw presets) normalize to a canonical name.
          assert.ok(variants.includes(out), `${v} -> ${out}`);
        }
        return;
      }
      // Required fields: the message names what is missing; a sample round-trips.
      assert.match(e.message, /missing field|invalid type|required arguments were not provided/);
      if (SAMPLES[typeName] == null) return;
      defaults = fp.normalize(typeName, SAMPLES[typeName]);
    }
    // Defaults round-trip and are stable.
    assert.deepEqual(fp.normalize(typeName, defaults), defaults);
    if (plainObject(defaults)) {
      assert.throws(() => fp.normalize(typeName, { ...defaults, not_a_field: 1 }), (e) => e instanceof fp.InvalidInputError && /unknown field `not_a_field`/.test(e.message));
      // Partial objects fill in the documented defaults.
      const keys = Object.keys(defaults);
      if (keys.length && !(typeName in SAMPLES)) {
        assert.deepEqual(fp.normalize(typeName, { [keys[0]]: defaults[keys[0]] }), defaults);
      }
    }
  });
}

test('normalize validation', () => {
  assert.throws(() => fp.formats.normalize('formats::nope::Options'), (e) => e instanceof fp.InvalidInputError && /unknown plain-data type/.test(e.message));
  assert.equal(fp.normalize('resources::ResourceLimits').max_glyphs, 1_000_000);
  assert.equal(fp.normalize('resources::TransportLimits').max_output_bytes, 64 * 1024 * 1024);
  assert.deepEqual(fp.formats.normalize('resources::ResourceLimits'), fp.normalize('resources::ResourceLimits'));
  if (!enabled(fp, 'fzx')) return;
  assert.throws(() => fp.formats.normalize('formats::fzx::FzxWriteOptions', { gray_threshold: 256 }), fp.InvalidInputError);
  assert.throws(() => fp.formats.normalize('formats::fzx::FzxWriteOptions', { height: -1 }), fp.InvalidInputError);
  assert.throws(() => fp.formats.normalize('formats::fzx::FzxWriteOptions', { height: 1.5 }), /floating point/);
  assert.throws(() => fp.formats.normalize('formats::iigs::Version', 'V1_5'), fp.InvalidInputError);
  assert.equal(fp.formats.normalize('formats::iigs::Version', '1.5'), '1.5');
  assert.equal(fp.formats.normalize('formats::pcf::GlyphPad', '4'), '4');
  assert.throws(() => fp.formats.normalize('formats::fbit::FbitReadOptions', { apple_double_resource: '../x' }), fp.InvalidInputError);
});

function setters() {
  return Object.getOwnPropertyNames(fp.Registry.prototype)
    .filter((n) => /^set\w+(Read|Write)Options$/.test(n) && !['setReadOptions', 'setWriteOptions'].includes(n))
    .sort();
}

test('every typed setter is a method', () => {
  const names = setters();
  const expected = { full: 170, minimal: 2, merge: 4 }[PROFILE];
  if (expected !== undefined) assert.equal(names.length, expected);
  assert.ok(names.includes('setPcfWriteOptions'));
  if (PROFILE !== 'minimal') assert.ok(names.includes('setVectorReadOptions'));
  if (PROFILE === 'full') assert.ok(names.includes('setColorWriteOptions'));
});

for (const name of setters()) {
  test(`typed setter accepts defaults and rejects unknown: ${name}`, () => {
    const reg = new fp.Registry();
    reg[name]({});
    assert.throws(() => reg[name]({ not_a_field: true }), (e) => e instanceof fp.InvalidInputError && /unknown field/.test(e.message));
  });
}

const configurable = fp.formats.configurableFormats();

for (const fid of [...configurable.read].sort()) {
  test(`read options by format id: ${fid}`, () => {
    const reg = new fp.Registry();
    reg.setReadOptions(fid, {});
    assert.throws(() => reg.setReadOptions(fid, { not_a_field: 1 }), fp.InvalidInputError);
  });
}

for (const fid of [...configurable.write].sort()) {
  test(`write options by format id: ${fid}`, () => {
    const reg = new fp.Registry();
    reg.setWriteOptions(fid, {});
    assert.throws(() => reg.setWriteOptions(fid, { not_a_field: 1 }), fp.InvalidInputError);
  });
}

test('setter dispatch errors and effects', { skip: !enabled(fp, 'fzx') && `fzx compiled out of the ${PROFILE} build` }, () => {
  assert.equal(configurable.write.length, 98);
  assert.ok(!configurable.write.includes('bdf'));
  assert.equal(configurable.read.length, 85);
  const reg = new fp.Registry();
  assert.throws(() => reg.setReadOptions('nope', {}), fp.UnknownFormatError);
  assert.throws(() => reg.setWriteOptions('bdf', {}), /formats\.bdf\.write/);
  assert.throws(() => reg.setReadOptions('otb', {}), /formats\.embedded\.read/);
  assert.throws(() => reg.setReadOptions('pcf', {}), fp.InvalidInputError);
  // The fallible setters report a missing writer.
  const planned = new fp.Registry({ builtin: false });
  assert.throws(() => planned.setPcfWriteOptions({}), (e) => e instanceof fp.NotImplementedFormatError && e.operation === 'Write');
  assert.throws(() => planned.setWriteOptions('yaff', {}), fp.NotImplementedFormatError);
  // Planned entries ignore infallible setters (nothing to configure).
  planned.setFzxWriteOptions({});
  // Options change the output exactly as the CLI flags do (golden hashes).
  const leg3 = fp.read(sourceFile('leg3'));
  assert.throws(() => reg.write(leg3.fonts, 'fzx'), fp.ReportedErrorsError);
  reg.setOptions('fzx', { write: { tracking: 1 } });
  const configured = reg.write(leg3.fonts, 'fzx');
  assert.deepEqual(assetsOf(configured), assetsOf(fp.write(leg3.fonts, 'fzx', { options: caseOf(fp, 'fzx').write })));
  const reg2 = new fp.Registry();
  reg2.setCodec('fzx', { write_options: { tracking: 1 }, read_options: {} });
  assert.deepEqual(assetsOf(reg2.write(leg3.fonts, 'fzx')), assetsOf(configured));
  assert.throws(() => reg2.setCodec('fzx', { options: {} }), /unknown codec field/);
  assert.throws(() => reg2.setCodec('fzx', 3), fp.InvalidInputError);
  assert.throws(() => reg2.setOptions('fzx', { wrte: {} }), /unknown option `wrte`/);
  // Read options configured on the registry.
  const uni3 = fp.read(sourceFile('uni3'));
  const hexdata = fp.write(uni3.fonts, 'hex', { options: { geometry: { height: 6, descent: 1 } } }).output.intoSingle();
  const reg3 = new fp.Registry();
  assert.throws(() => reg3.read(hexdata, { format: 'hex' }), fp.FontpixelError);
  reg3.setHexReadOptions({ height: 6, descent: 1 });
  assert.equal(reg3.read(hexdata, { format: 'hex' }).format, 'hex');
  // fon shares the FNT options.
  reg3.setReadOptions('fon', {});
  reg3.setWriteOptions('fon', {});
});

const RUSTDOC = process.env.FONTPIXEL_RUSTDOC_TARGET
  ? path.join(process.env.FONTPIXEL_RUSTDOC_TARGET, 'doc')
  : path.join(process.env.CARGO_TARGET_DIR ?? path.join(REPO, 'target'), 'rustdoc-json', 'doc');

test('generated normalize table and coverage match the rustdoc inventory', { skip: !existsSync(path.join(RUSTDOC, 'fontpixel.json')) && 'no rustdoc JSON; run tools/bindings/gate.sh' }, () => {
  const python = process.env.PYTHON ?? 'python3';
  const run = (script, ...args) => spawnSync(python, [path.join(REPO, 'tools/bindings', script), ...args], { encoding: 'utf8' });
  const normalize = run('gen_normalize.py', '--rustdoc', RUSTDOC, '--check');
  assert.equal(normalize.status, 0, normalize.stderr);
  const inventory = path.join(path.dirname(RUSTDOC), 'inventory.json');
  if (existsSync(inventory)) {
    const coverage = run('gen_formats_coverage.py', inventory, '--check');
    assert.equal(coverage.status, 0, coverage.stderr);
  }
});

test('configurable aliases correspond to codecs enabled in this profile', () => {
  const configurable = fp.formats.configurableFormats();
  for (const id of [...configurable.read, ...configurable.write]) {
    assert.ok(enabled(fp, id), `uncompiled alias advertised: ${id}`);
  }
  if (!enabled(fp, 'bdat')) {
    assert.ok(!configurable.write.includes('bdat'));
    const reg = new fp.Registry();
    try {
      assert.throws(() => reg.setWriteOptions('bdat', {}), fp.FeatureDisabledError);
    } finally {
      reg.free();
    }
  }
});
