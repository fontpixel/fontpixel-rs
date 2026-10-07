// The package entry point: exported classes, functions and error hierarchy
// (mirrors crates/fontpixel-py/tests/test_package.py).
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { test } from 'node:test';
import { PKG, load } from './helpers.mjs';

const fp = await load();

test('package exports the native API, loaders and error hierarchy', () => {
  const manifest = JSON.parse(readFileSync(path.join(PKG, 'package.json'), 'utf8'));
  assert.equal(manifest.name, 'fontpixel');
  assert.equal(manifest.private, true);
  assert.equal(manifest.type, 'module');
  assert.ok(fp.version());
  for (const name of ['init', 'initSync', 'version', 'capabilities', 'requireCapability', 'withAsyncResources']) {
    assert.equal(typeof fp[name], 'function', name);
  }
  assert.ok(fp.wasmUrl instanceof URL);
  // Model, metadata and resource classes (the other areas add theirs).
  const classes = [
    'FontSet', 'Font', 'Glyph', 'Bitmap', 'Codepoint', 'Properties', 'BitmapProjection',
    'ResourceSet', 'Input', 'ReadContext', 'OutputAsset', 'OutputBundle', 'Written', 'Loaded',
  ];
  for (const name of classes) {
    assert.equal(typeof fp[name], 'function', name);
    assert.equal(typeof fp[name].prototype.free, 'function', `${name}.free`);
    assert.equal(typeof fp[name].prototype[Symbol.dispose], 'function', `${name}[Symbol.dispose]`);
  }
  const functions = [
    'read', 'normalize', 'optionTypes', 'applyNames', 'nameOverridesIsEmpty', 'applyBitmapProjection',
    'reportHasLoss', 'reportHasErrors', 'reportPush', 'reportExtend', 'reportToText',
    'resourcePath', 'defaultResourceLimits', 'defaultTransportLimits',
  ];
  for (const name of functions) assert.equal(typeof fp[name], 'function', name);
  // Iteration protocol of the list-like classes.
  for (const name of ['FontSet', 'Font', 'Properties', 'OutputBundle']) {
    assert.equal(typeof fp[name].prototype[Symbol.iterator], 'function', `${name}[Symbol.iterator]`);
  }
  // The error hierarchy: every class derives from FontpixelError and Error.
  assert.ok(fp.ResourceLimitError.prototype instanceof fp.FontpixelError);
  assert.ok(fp.StrictLossError.prototype instanceof fp.FontpixelError);
  assert.ok(fp.FontpixelError.prototype instanceof Error);
  // Internal helpers stay internal.
  for (const name of ['makeError', 'stashHandle', 'installModelProtocols', 'guardHandles', 'encodePlain', 'decodePlain']) {
    assert.equal(fp[name], undefined, name);
  }
});
