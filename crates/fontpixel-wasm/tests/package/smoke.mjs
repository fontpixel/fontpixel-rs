// Smoke test of the packed npm package, run by scripts/test_npm_package.sh
// from a scratch project that installed the tarball: every import goes
// through the package's `exports` map (bare specifiers), never a path.
//
// Usage (in the scratch project): node smoke.mjs PROFILE...   (the smaller
// profiles the package ships, e.g. bdfparser web minimal)
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const manifest = JSON.parse(readFileSync(require.resolve('fontpixel/package.json'), 'utf8'));
const BDF = `STARTFONT 2.1
FONT -Smoke-Test-Medium-R-Normal--4-40-75-75-C-40-ISO10646-1
SIZE 4 75 75
FONTBOUNDINGBOX 3 3 0 0
STARTPROPERTIES 2
FONT_ASCENT 3
FONT_DESCENT 0
ENDPROPERTIES
CHARS 1
STARTCHAR A
ENCODING 65
SWIDTH 1000 0
DWIDTH 4 0
BBX 3 3 0 0
BITMAP
40
E0
A0
ENDCHAR
ENDFONT
`;
const bytes = new TextEncoder().encode(BDF);

async function native(specifier, { compat }) {
  const fp = await import(specifier);
  await fp.init();
  assert.equal(fp.version(), manifest.version, `${specifier} version`);
  const loaded = fp.read(bytes, { name: 'smoke.bdf' });
  const font = loaded.fonts.font(0);
  assert.equal(font.length, 1);
  // The README's quick start, call for call.
  const written = fp.write(loaded.fonts, 'pcf');
  assert.ok(written.output.intoSingle().length > 0, `${specifier} writes PCF`);
  written.free();
  const drawn = font.draw('A');
  const png = fp.toPng(drawn.bitmap);
  assert.equal(png[0], 137, `${specifier} PNG`);
  drawn.free();
  font.free();
  loaded.fonts.free();
  const caps = new Set(fp.capabilities().filter((c) => c.available).map((c) => c.name));
  assert.equal(caps.has('compat_bdfparser'), compat, `${specifier} compat capability`);
  return fp;
}

async function compat(prefix) {
  const bp = await import(`${prefix}/compat/bdfparser`);
  await import(`${prefix}/compat/bdfparser/cjk`);
  const cli = await import(`${prefix}/compat/bdfparser/cli`);
  const font = new bp.Font().load_string(BDF);
  assert.equal(font.length, 1);
  assert.deepEqual(font.glyph('A').draw().bindata, ['010', '111', '101']);
  assert.equal(typeof font.draw('A').toString(), 'string');
  assert.equal(bp.cp2char(0xb0a1, 'gb2312'), '啊');
  assert.equal(typeof cli.main, 'function');
}

await native('fontpixel', { compat: true });
await compat('fontpixel');
const files = await import('fontpixel/node-files');
assert.equal(typeof files.readFiles, 'function');
for (const profile of process.argv.slice(2)) {
  const hasCompat = `./${profile}/compat/bdfparser` in manifest.exports;
  await native(`fontpixel/${profile}`, { compat: hasCompat });
  if (hasCompat) await compat(`fontpixel/${profile}`);
}
// The ESM-only package: require() of the entry fails with a clear error on
// every supported Node (top-level await in the module graph).
assert.throws(() => require('fontpixel/compat/bdfparser'));
console.log(`fontpixel ${manifest.version} smoke passed on Node ${process.version}: root, ${process.argv.slice(2).join(', ')}`);
