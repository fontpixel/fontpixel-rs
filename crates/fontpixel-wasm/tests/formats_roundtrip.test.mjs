// Every catalogue format: write -> read back through the WASM binding,
// compared with the native CLI.
//
// Expected values are the SHA-256 hashes in
// crates/fontpixel-py/tests/data/formats/cli_golden.json, written by the
// frozen fontpixel CLI for the same source font and options (`convert SRC
// --to ID <flags>`, then `convert OUT --from ID <read flags> -o back.yaff`).
// Each test performs the operation through the binding and requires
// byte-identical assets and an identical read-back model (compared as the
// YAFF both sides write for it). Builds without the YAFF writer (the `merge`
// profile) compare the read-back model as BDF bytes against a live CLI
// conversion instead. Formats compiled out of the loaded build skip with the
// reason and are checked to fail with FeatureDisabledError instead.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { HAS_CLI, PROFILE, REPO, cliFiles, load } from './helpers.mjs';
import {
  CLI_IMPLICIT,
  FIXTURES,
  GOLDEN,
  assetsOf,
  callAdaptive,
  caseOf,
  enabled,
  hashes,
  namespaceOf,
  primaryName,
  sha,
  sourceBytes,
  sourceFile,
} from './formats_support.mjs';

const fp = await load();
const STATUS = Object.fromEntries(fp.listFormats().map((s) => [s.format.id, s]));
const IDS = Object.keys(STATUS);
const HAS_YAFF = enabled(fp, 'yaff');
const SOURCE_FORMAT = { uni3: 'bdf', uni5: 'bdf', leg3: 'pcf', leg5: 'pcf' };
const KIND = { otb: 'open_type', bdat: 'apple', color: 'color' };

/** Why `fid`'s case cannot run in this build (or `false`). */
function missing(fid, ...more) {
  const c = caseOf(fp, fid);
  const needs = [fid, c.source === 'native' ? fid : SOURCE_FORMAT[c.source], ...more];
  const off = needs.filter((f) => !enabled(fp, f));
  return off.length ? `${[...new Set(off)].join(', ')} compiled out of the ${PROFILE} build` : false;
}

function yaffOf(model, report) {
  return fp.write(model, 'yaff', { report }).output.intoSingle();
}

function loadSource(fid) {
  const c = caseOf(fp, fid);
  const data = sourceBytes(fid);
  if (c.source === 'native') {
    return fp.read(data, { format: fid, options: Object.keys(c.sourceRead).length ? c.sourceRead : undefined, name: primaryName(STATUS[fid]) });
  }
  return fp.read(data);
}

function writeCase(fid) {
  const c = caseOf(fp, fid);
  const loaded = loadSource(fid);
  return fp.write(loaded.fonts, fid, {
    options: Object.keys(c.write).length ? c.write : undefined,
    primaryName: primaryName(STATUS[fid]),
    report: loaded.report,
  });
}

/** Read-back model check: YAFF hash against the golden file, else BDF bytes against a live CLI run. */
function assertReadback(t, fid, back, written) {
  const g = GOLDEN.formats[fid];
  if (HAS_YAFF) {
    assert.equal(sha(yaffOf(back.fonts, back.report)), g.readback.yaff);
    return;
  }
  if (!HAS_CLI) {
    t.diagnostic('no YAFF writer in this build and no native CLI: read-back model not compared');
    return;
  }
  const c = caseOf(fp, fid);
  const primary = written.output.primary;
  const cli = cliFiles(assetsOf(written), ['convert', primary, '--from', fid, ...c.cliRead, '-o', 'back.bdf']);
  assert.equal(cli.code, 0, cli.stderr);
  const ours = fp.write(back.fonts, 'bdf', { report: back.report }).output.intoSingle();
  assert.deepEqual(ours, cli.files['back.bdf']);
}

test('catalogue matches cli listing', () => {
  assert.equal(IDS.length, 99);
  assert.deepEqual(new Set(Object.keys(GOLDEN.formats)), new Set(IDS));
  const listing = fp.listFormats();
  if (PROFILE === 'full') {
    assert.deepEqual(listing, GOLDEN.listing);
    assert.ok(listing.every((s) => s.read && s.write));
    return;
  }
  // Smaller builds: the same catalogue; compiled-out formats are honest
  // entries without codecs.
  for (const [i, s] of listing.entries()) {
    const g = GOLDEN.listing[i];
    assert.deepEqual({ ...s.format, feature_enabled: true }, g.format);
    if (s.format.feature_enabled) assert.deepEqual(s, g);
    else assert.deepEqual([s.detect, s.read, s.write], [false, false, false]);
  }
});

for (const fid of IDS) {
  test(`roundtrip matches cli: ${fid}`, { skip: missing(fid) }, (t) => {
    const g = GOLDEN.formats[fid];
    const c = caseOf(fp, fid);
    if (g.source && HAS_YAFF) {
      const src = loadSource(fid);
      assert.equal(sha(yaffOf(src.fonts, src.report)), g.source.yaff);
    }
    const written = writeCase(fid);
    assert.ok(written instanceof fp.Written);
    assert.deepEqual(hashes(written), g.write.assets);
    const back = fp.read(written, { format: fid, options: Object.keys(c.read).length ? c.read : undefined });
    assert.equal(back.format, fid);
    assertReadback(t, fid, back, written);
  });
}

for (const fid of IDS) {
  // Flag-free conversion of the Unicode 3x5 font: same assets, or both fail.
  test(`plain conversion matches cli: ${fid}`, { skip: missing(fid, 'bdf') }, () => {
    const g = GOLDEN.formats[fid].plain;
    const loaded = fp.read(sourceFile('uni3'));
    const args = { options: CLI_IMPLICIT[fid], primaryName: primaryName(STATUS[fid]), report: loaded.report };
    if (g.assets) {
      assert.deepEqual(hashes(fp.write(loaded.fonts, fid, args)), g.assets);
    } else {
      assert.throws(
        () => fp.write(loaded.fonts, fid, args),
        (e) => e instanceof fp.FontpixelError && ['InvalidInput', 'ReportedErrors', 'StrictLoss'].includes(e.kind),
      );
    }
  });
}

function entries(report) {
  return report.entries.map((e) => [e.level, e.impact, e.code, e.message]);
}

for (const fid of IDS) {
  // `formats.<module>` detect/read/write agree with the registry.
  test(`direct namespace matches registry: ${fid}`, { skip: missing(fid) || (!HAS_YAFF && 'no YAFF writer to compare models') }, () => {
    const [name, mod] = namespaceOf(fp, fid);
    const c = caseOf(fp, fid);
    const g = GOLDEN.formats[fid];
    const written = writeCase(fid);
    const primary = written.output.primary;
    const assets = assetsOf(written);
    const data = assets[primary];
    let resources = Object.fromEntries(Object.entries(assets).filter(([k]) => k !== primary));
    if (written.output.isDirectory) {
      resources = Object.fromEntries(Object.entries(assets).map(([k, v]) => [k.slice(primary.length + 1), v]));
    }
    const readOptions = Object.keys(c.read).length ? c.read : undefined;
    const writeOptions = Object.keys(c.write).length ? c.write : undefined;

    // detect
    if (typeof mod.detect === 'function' && STATUS[fid].detect && data !== undefined) {
      let got;
      if (name === 'embedded') got = mod.detect(data, KIND[fid]);
      else if (name === 'vectorRaster') got = mod.detect(data, fid);
      else got = mod.detect(data);
      assert.ok(['possible', 'certain'].includes(got), `${fid}: ${got}`);
    }

    // read
    let result;
    if (name === 'embedded') result = mod.read(data, KIND[fid], { options: readOptions });
    else if (name === 'vectorRaster') result = mod.read(data, { options: readOptions });
    else {
      const bag = { options: readOptions, resources: Object.keys(resources).length ? resources : undefined };
      if (written.output.isDirectory) bag.directory = primary;
      else bag.name = primary;
      [result] = callAdaptive(fp, mod.read, written.output.isDirectory ? undefined : data, bag);
    }
    const { model, report } = result;
    assert.ok(Array.isArray(report.entries));
    let registryBack = fp.read(written, { format: fid, options: readOptions });
    let registryModel = registryBack.fonts;
    if (model instanceof fp.Font) {
      // Single-strike readers return a Font; the registry wraps it in a set.
      const one = registryBack.intoSingle();
      assert.ok(one.fonts instanceof fp.Font);
      registryModel = one.fonts;
    } else {
      assert.ok(model instanceof fp.FontSet);
    }
    // Registry codecs may add the asset name to entry locations.
    assert.deepEqual(entries(report), entries(registryBack.report));
    assert.deepEqual(yaffOf(model, report), yaffOf(registryModel, registryBack.report));

    // write: the registry wraps the same codec, so the bytes agree
    const loaded = loadSource(fid);
    if (name === 'embedded') {
      const out = fid === 'color' ? mod.writeColor(loaded.fonts, { options: writeOptions }) : mod.write(loaded.fonts, KIND[fid], { options: writeOptions });
      assert.equal(sha(out.data), g.write.assets[primary]);
      return;
    }
    if (name === 'vectorRaster') {
      const out = mod.write(loaded.fonts, fid, { options: writeOptions });
      assert.equal(sha(out.data), g.write.assets[primary]);
      return;
    }
    const [out] = callAdaptive(fp, mod.write, loaded.fonts, { options: writeOptions, primaryName: primary });
    if (out instanceof fp.Written) {
      assert.deepEqual(hashes(out), g.write.assets);
    } else {
      assert.ok(out.data instanceof Uint8Array && Array.isArray(out.report.entries));
      assert.equal(sha(out.data), g.write.assets[primary]);
    }
  });
}

for (const [key, [rel, format, options]] of Object.entries(FIXTURES)) {
  const needs = format ? [format] : [];
  // Real fixture files from tests/data and crates/fontpixel/tests.
  test(`fixture reads match cli: ${key}`, { skip: (!HAS_YAFF && 'no YAFF writer in this build') || needs.some((f) => !enabled(fp, f)) && `${format} compiled out of the ${PROFILE} build` }, () => {
    const file = path.join(REPO, rel);
    const name = path.basename(rel);
    let resources;
    if (name.endsWith('.pil')) {
      resources = { [`${name.slice(0, -4)}.pbm`]: new Uint8Array(readFileSync(`${file.slice(0, -4)}.pbm`)) };
    }
    let loaded;
    try {
      loaded = fp.read(new Uint8Array(readFileSync(file)), {
        format: format ?? undefined,
        options: Object.keys(options).length ? options : undefined,
        resources,
        name,
      });
    } catch (e) {
      // A detected format compiled out of this build fails honestly.
      if (PROFILE !== 'full' && e instanceof fp.FontpixelError) return;
      throw e;
    }
    assert.equal(sha(yaffOf(loaded.fonts, loaded.report)), GOLDEN.fixtures[key].yaff);
  });
}

test('compiled-out formats report FeatureDisabledError, never empty success', (t) => {
  const off = IDS.filter((fid) => !STATUS[fid].format.feature_enabled);
  if (PROFILE === 'full') assert.deepEqual(off, []);
  t.diagnostic(`${off.length} formats compiled out of the ${PROFILE} build`);
  const loaded = fp.read(sourceFile('uni3'));
  const served = new Set(fp.formats.modules().filter((n) => n !== 'x11').flatMap((n) => fp.formats[n].formatIds()));
  const reg = new fp.Registry();
  for (const fid of off) {
    const st = STATUS[fid];
    assert.deepEqual([st.detect, st.read, st.write], [false, false, false]);
    assert.throws(
      () => fp.write(loaded.fonts, fid),
      (e) => e instanceof fp.FeatureDisabledError && e.kind === 'FeatureDisabled' && e.format === fid,
    );
    assert.throws(
      () => fp.read(new Uint8Array(8), { format: fid }),
      (e) => e instanceof fp.FeatureDisabledError && e.format === fid,
    );
    assert.throws(() => reg.ensureWritable(fid), fp.FeatureDisabledError);
    assert.throws(() => reg.setWriteOptions(fid, {}), (e) => e instanceof fp.FeatureDisabledError && e.format === fid);
    assert.ok(!served.has(fid), `${fid} has no namespace in this build`);
  }
  // Every compiled format has a namespace.
  for (const fid of IDS.filter((f) => STATUS[f].format.feature_enabled)) assert.ok(served.has(fid), fid);
});

const LIVE = [
  ['bdf -> pcf', 'uni3', 'pcf', 'font.pcf', []],
  ['bdf -> hbf with sidecar', 'uni5', 'hbf', 'font.hbf', []],
  ['pcf -> consoleet directory', 'leg3', 'consoleet', 'font.txt', []],
  ['bdf -> woff2 (cff)', 'uni3', 'woff2', 'font.woff2', []],
  ['bdf -> yaff', 'uni3', 'yaff', 'font.yaff', []],
  ['bdf -> otb', 'uni3', 'otb', 'font.otb', []],
];

for (const [title, source, fid, primary, flags] of LIVE) {
  const srcName = `src.${source.startsWith('leg') ? 'pcf' : 'bdf'}`;
  test(`live cli convert bytes equal the binding: ${title}`, { skip: (!HAS_CLI && 'native CLI not found (FONTPIXEL_CLI)') || missing(fid) }, () => {
    const src = sourceFile(source);
    const cli = cliFiles({ [srcName]: src }, ['convert', srcName, '-o', primary, '--to', fid, ...flags]);
    assert.equal(cli.code, 0, cli.stderr);
    const loaded = fp.read(src, { name: srcName });
    const ours = fp.write(loaded.fonts, fid, { options: CLI_IMPLICIT[fid], primaryName: primary, report: loaded.report });
    assert.deepEqual(Object.keys(assetsOf(ours)).sort(), Object.keys(cli.files).sort());
    for (const [p, bytes] of Object.entries(assetsOf(ours))) assert.deepEqual(bytes, cli.files[p], p);
  });
}
