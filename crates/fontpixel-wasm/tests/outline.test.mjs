// Outlines: bitmap contours, pixel shapes, segments, contour and path
// rasterization, the SPEC adapter (ported from crates/fontpixel-py/tests/
// test_outline.py). Expectations are direct Rust results of the same calls
// (crates/fontpixel-py/tests/data/p3/expected.json, see sfnt_p3_support.py).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { REPO, load } from './helpers.mjs';

const fp = await load();
const E = JSON.parse(readFileSync(path.join(REPO, 'crates', 'fontpixel-py', 'tests', 'data', 'p3', 'expected.json'), 'utf8'));
const DOT = { shape: 'dot', dot_size: 0.9 };
const isError = (Class, check) => (e) => {
  assert.ok(e instanceof Class, `expected ${Class.name}, got ${e?.constructor?.name}: ${e?.message}`);
  if (check) check(e);
  return true;
};

/** The Mono bitmap of glyph A, rasterized from its TrueType contours. */
const glyphA = () => fp.outlineRasterize(E['glyf.contours.2'], 64, [0, 0], 5, 7);

test('outline contours, pixel shapes, segments and rasterization equal Rust', () => {
  const bm = glyphA();
  assert.ok(bm instanceof fp.Bitmap);
  assert.equal(bm.depth, 'mono');
  assert.equal(String(bm), E['outline.rasterize']);
  assert.equal(String(fp.pathRasterize(E['path.edges'], 0.01, [0, 0], 4, 4)), E['path.rasterize']);
  assert.deepEqual(fp.outlineContours(bm), E['outline.contours']);
  const defaults = fp.outlineDefaults();
  assert.deepEqual(fp.outlineContours(bm, defaults.Options, defaults.Limits), E['outline.contours']);
  assert.deepEqual(fp.outlineContours(bm, null, {}), E['outline.contours']);
  assert.deepEqual(fp.outlinePixelOrigins(bm, DOT), E['outline.pixel_origins.dot']);
  assert.deepEqual(fp.outlinePixelContour(DOT), E['outline.pixel_contour.dot']);
  assert.deepEqual(fp.outlineSegments(fp.outlinePixelContour(DOT)), E['outline.segments']);
  assert.deepEqual(fp.specBitmapContours(['0110', '1001'], 'circle', 0.8, 10, 1, -2), E['outline.spec']);
  assert.deepEqual(fp.specBitmapContours(['0110', '1001'], 'circle', 0.8, 10n, 1n, -2n), E['outline.spec']);
  // Rasterizing the traced contours reproduces the same raster.
  const again = fp.outlineRasterize(fp.outlineContours(bm), 100, [0, 0], 5, 7);
  assert.ok(again.equals(bm));
  assert.deepEqual(fp.outlineContours(again), E['outline.contours']);
  // Empty inputs: nothing to trace, a blank raster.
  assert.deepEqual(fp.outlineContours(new fp.Bitmap(3, 3)), []);
  assert.deepEqual(fp.outlineSegments([]), []);
  assert.ok(fp.outlineRasterize([], 100, [0, 0], 0, 0).equals(new fp.Bitmap()));
  assert.ok(!fp.outlineRasterize([], 100, [0, 0], 3, 2).hasInk());
});

test('outline path edges equal Rust', () => {
  const edges = E['path.edges'];
  const bm = fp.pathRasterize(edges, 0.01, [0, 0], 4, 4);
  assert.deepEqual(fp.outlineContours(bm), E['path.rasterize.contours']);
  assert.deepEqual(
    edges.map((e) => [fp.edgePoints(e), fp.edgeMaxY(e)]),
    E['path.points_max_y'],
  );
  assert.throws(() => fp.pathRasterize(edges, 0, [0, 0], 4, 4), fp.InvalidInputError);
  assert.throws(() => fp.pathRasterize(edges, 0.01, [Infinity, 0], 4, 4), fp.InvalidInputError);
  assert.throws(
    () => fp.pathRasterize(edges, 0.01, [0, 0], 4, 4, { max_edges: 1 }),
    isError(fp.ResourceLimitError, (e) => {
      assert.equal(e.resource, 'path edges');
      assert.equal(e.limit, 1);
    }),
  );
  assert.throws(() => fp.pathRasterize(edges, 0.01, [0, 0], 1 << 20, 1 << 20), fp.ResourceLimitError);
  assert.throws(() => fp.edgePoints({ arc: [[0, 0]] }), fp.InvalidInputError);
  assert.throws(() => fp.edgeMaxY({ line: [[0, 0]] }), fp.InvalidInputError);
});

test('outline errors and limits', () => {
  const bm = glyphA();
  assert.throws(() => fp.outlineContours(new fp.Bitmap(2, 2, 'gray8')), isError(fp.InvalidInputError, (e) => assert.match(e.message, /mono bitmap/)));
  assert.throws(() => fp.outlinePixelOrigins(new fp.Bitmap(2, 2, 'gray8')), fp.InvalidInputError);
  assert.throws(() => fp.outlineContours(bm, { dot_size: 0.5 }), fp.InvalidInputError);
  assert.throws(() => fp.outlineContours(bm, { units_per_pixel: 0 }), fp.InvalidInputError);
  assert.throws(
    () => fp.outlineContours(bm, null, { max_pixels: 4 }),
    isError(fp.ResourceLimitError, (e) => {
      assert.equal(e.resource, 'outline pixels');
      assert.equal(e.limit, 4);
    }),
  );
  assert.throws(() => fp.outlineRasterize(E['glyf.contours.2'], 64, [0, 0], 5, 7, { max_points: 3 }), fp.ResourceLimitError);
  assert.throws(() => fp.outlineRasterize(E['glyf.contours.2'], -1, [0, 0], 5, 7), fp.InvalidInputError);
  assert.throws(() => fp.outlineRasterize(E['glyf.contours.2'], 64, [0.5, 0], 5, 7), fp.InvalidInputError);
  assert.throws(() => fp.outlineRasterize([[[0, 0]]], 64, [0, 0], 5, 7), fp.InvalidInputError);
  assert.throws(() => fp.specBitmapContours(['01'], 'hexagon', 1, 10, 0, 0), fp.InvalidInputError);
  assert.throws(() => fp.outlineContours(bm, { shape: 'triangle' }), isError(fp.InvalidInputError, (e) => assert.match(e.message, /unknown variant `triangle`/)));
  assert.throws(() => fp.outlineContours(bm, { bogus: 1 }), fp.InvalidInputError);
  assert.throws(() => fp.outlineSegments([[NaN, 0, true]]), fp.InvalidInputError);
  const d = fp.outlineDefaults();
  assert.deepEqual(d.Options, { shape: 'square', units_per_pixel: 100, dot_size: 1, origin: [0, 0] });
  assert.equal(d.Limits.max_points, 1024 * 1024);
});
