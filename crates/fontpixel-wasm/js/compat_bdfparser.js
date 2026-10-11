// `fontpixel/compat/bdfparser`: the bdfparser-js 3.0 compatible API (SPEC-v3)
// on the fontpixel Rust core.
//
// `Font`, `Glyph`, `Bitmap`, the module functions (charsets, PCF, TrueType,
// outlines), the warning handler and `BDFParseError` behave as in
// bdfparser-js 3.0.0. The font data stays in WebAssembly memory
// (`fontpixel::compat::bdfparser`, the model shared with the Python
// `fontpixel.compat.bdfparser`): `Font`/`Glyph`/`Bitmap` hold handles, and
// `font.headers`, `font.props`, `font.glyphs`, `font.unencoded` and
// `font._propnames` are live views that project the model when read and edit
// it in place when written (glyph handles stay attached). Pixels become row
// strings only when `bindata` (or another export) is read.
//
// The module initializes the WebAssembly instance itself (top-level
// `await init()`); it needs the `full` build (capability `compat_bdfparser`),
// other builds throw `UnsupportedError` when it is imported.
//
// Differences from bdfparser-js are listed in docs/bindings-design.md
// ("bdfparser-js compatibility") and
// crates/fontpixel-wasm/tests/compat_bdfparser/KNOWN_DIFFERENCES.md.
import * as glue from './fontpixel_wasm.js';
import { FontpixelError } from './errors.js';
import { init, requireCapability } from './index.js';

await init();
requireCapability('compat_bdfparser');

const {
  BdfparserRawFont: RawFont,
  BdfparserRawGlyph: RawGlyph,
  BdfparserRawBitmap: RawBitmap,
  BdfparserRawBitmapList: RawBitmapList,
  BdfparserRawCodec: RawCodec,
} = glue;

/* ------------------------------------------------------------------------ */
/* Constants                                                                */
/* ------------------------------------------------------------------------ */

/** bdfparser API version this module implements. */
export const VERSION = '3.0.0';

/** VVECTOR delimiter: whitespace and/or comma */
export const PATTERN_VVECTOR_DELIMITER = /[\s,]+/;

/** The empty glyph drawn for a missing codepoint (bdfparser-js `EMPTY_GLYPH`). */
export const EMPTY_GLYPH = Object.freeze({
  glyphname: '.notdef',
  codepoint: 0,
  bbw: 0,
  bbh: 0,
  bbxoff: 0,
  bbyoff: 0,
  swx0: 0,
  swy0: 0,
  dwx0: 0,
  dwy0: 0,
  swx1: 0,
  swy1: 0,
  dwx1: 0,
  dwy1: 0,
  vvectorx: 0,
  vvectory: 0,
  hexdata: Object.freeze([]),
});

/** The 17 glyph meta keys, in slot order (`fontpixel::compat::bdfparser::META_TITLES`). */
export const META_TITLES = Object.freeze(glue.bdfparserMetaTitles());

export const DIRE_SHORTCUT_MAP = Object.freeze({
  lr: 'lrtb',
  rl: 'rltb',
  tb: 'tbrl',
  bt: 'btrl',
  lrtb: undefined,
  lrbt: undefined,
  rltb: undefined,
  rlbt: undefined,
  tbrl: undefined,
  tblr: undefined,
  btrl: undefined,
  btlr: undefined,
});

export const DIRE_MAP = Object.freeze({ lr: 1, rl: 2, tb: 0, bt: -1 });

const META_INDEX = new Map(META_TITLES.map((k, i) => [k, i]));

/* ------------------------------------------------------------------------ */
/* Warnings and errors                                                      */
/* ------------------------------------------------------------------------ */

/** The default warning handler: `console.warn('[bdfparser] ' + message)`. */
export const defaultWarningHandler = (message) => {
  console.warn('[bdfparser] ' + message);
};

let currentHandler = defaultWarningHandler;

/** Emit a warning through the current handler. */
export const warn = (message) => {
  if (currentHandler !== null) {
    currentHandler(message);
  }
};

/**
 * Replace the warning handler: `null` silences warnings, `undefined` restores
 * the default; returns the previous handler.
 */
export const setWarningHandler = (handler) => {
  const previous = currentHandler;
  currentHandler = handler === undefined ? defaultWarningHandler : handler;
  return previous;
};

/**
 * Fatal BDF parse error. The message starts with the 1-based line number when
 * known (`"line 12: ..."`); `lineno` and `msg` hold the parts. Errors raised
 * by the Rust core also carry its structured fields (`kind`, `line`, `cause`).
 */
export class BDFParseError extends Error {
  constructor(message, lineno = null) {
    super(lineno === null ? message : `line ${lineno}: ${message}`);
    this.name = 'BDFParseError';
    this.lineno = lineno;
    this.msg = message;
  }
}

// The Rust core words parser diagnostics as the Python bdfparser does, quoting
// values with Python's `repr`; bdfparser-js quotes the same values with
// `JSON.stringify` (numbers and counts) or verbatim between single quotes
// (keywords and lines). `jsText` rewrites those messages to bdfparser-js's.
const PY_ESCAPES = { n: '\n', r: '\r', t: '\t', '\\': '\\', "'": "'", '"': '"', a: '\x07', b: '\b', f: '\f', v: '\v', 0: '\0' };

/** The Python `repr` string literal at the start of `s`: `[value, rest]`, or null. */
const parsePyRepr = (s) => {
  const q = s[0];
  if (q !== "'" && q !== '"') return null;
  let out = '';
  for (let i = 1; i < s.length; i++) {
    const c = s[i];
    if (c === q) return [out, s.slice(i + 1)];
    if (c !== '\\') {
      out += c;
      continue;
    }
    const n = s[++i];
    if (n === 'x' || n === 'u' || n === 'U') {
      const len = n === 'x' ? 2 : n === 'u' ? 4 : 8;
      out += String.fromCodePoint(parseInt(s.slice(i + 1, i + 1 + len), 16));
      i += len;
    } else {
      out += PY_ESCAPES[n] ?? '\\' + n;
    }
  }
  return null;
};

const JSON_QUOTED =
  /^((?:line \d+: )?(?:malformed number in \S+|malformed version number in STARTFONT|\S+ needs \d+ numbers?, got|malformed property count) )(.*)$/s;
const RAW_QUOTED = [
  [/^(line \d+: unknown header keyword )(.*)$/s, ' ignored'],
  [/^(line \d+: unexpected line )(.*)$/s, ' ignored'],
  [/^(line \d+: property )(.*)$/s, ' treated as a COMMENT line'],
];

/** A diagnostic of the Rust core in bdfparser-js's wording. */
const jsText = (msg) => {
  let m = JSON_QUOTED.exec(msg);
  if (m !== null) {
    const r = parsePyRepr(m[2]);
    return r !== null && r[1] === '' ? m[1] + JSON.stringify(r[0]) : msg;
  }
  for (const [re, suffix] of RAW_QUOTED) {
    m = re.exec(msg);
    if (m !== null) {
      const r = parsePyRepr(m[2]);
      return r !== null && r[1] === suffix ? `${m[1]}'${r[0]}'${suffix}` : msg;
    }
  }
  m = /^(line \d+: unknown keyword )(.*)$/s.exec(msg);
  if (m !== null) {
    const a = parsePyRepr(m[2]);
    if (a !== null && a[1].startsWith(' in glyph ')) {
      const b = parsePyRepr(a[1].slice(' in glyph '.length));
      if (b !== null && b[1] === ' ignored') return `${m[1]}'${a[0]}' in glyph '${b[0]}' ignored`;
    }
  }
  return msg;
};

// Messages of the Rust core that bdfparser-js raises as RangeError.
const RANGE_PATTERNS = [
  /^Bitmap\.crop: negative/,
  /does not fit in (16|32) bits/,
  /has a negative sized bitmap/,
  /^PCF: /,
  /out of range \(int16\)/,
  /^Invalid (ppu|dotsize|timestamp)/,
  /must not be negative$/,
  /^step must be/,
  /^pixelsize must be a positive integer/,
];
const TYPE_PATTERNS = [/must be an integer, not/, /expected a single character/, /^comment must be/];

/**
 * A fontpixel error turned into bdfparser-js's class (`BDFParseError`,
 * `RangeError`, `TypeError` or `Error`) with the same message; the original
 * error is the `cause` and its structured fields (`kind`, `exitCode`, ...)
 * are kept. Other values pass through unchanged.
 */
const translate = (e) => {
  if (!(e instanceof FontpixelError)) {
    return e;
  }
  let out;
  if (e.kind === 'BdfParse') {
    const line = typeof e.line === 'number' ? e.line : null;
    const prefix = `line ${line}: `;
    out = new BDFParseError(jsText(e.message.startsWith(prefix) ? e.message.slice(prefix.length) : e.message), line);
  } else {
    const msg = e.message;
    const Class = RANGE_PATTERNS.some((p) => p.test(msg))
      ? RangeError
      : TYPE_PATTERNS.some((p) => p.test(msg))
        ? TypeError
        : Error;
    out = new Class(msg);
  }
  for (const [k, v] of Object.entries(e)) {
    if (!(k in out)) {
      Object.defineProperty(out, k, { value: v, writable: true, configurable: true });
    }
  }
  Object.defineProperty(out, 'cause', { value: e, writable: true, configurable: true });
  return out;
};

/** Run `fn`, translating fontpixel errors. */
const guard = (fn) => {
  try {
    return fn();
  } catch (e) {
    throw translate(e);
  }
};

/* ------------------------------------------------------------------------ */
/* Helpers                                                                  */
/* ------------------------------------------------------------------------ */

// Internal state lives behind non-enumerable symbol keys, so `toEqual` sees
// only bdfparser-js's public attributes.
const S = Symbol('fontpixel.compat.state');

const hidden = (obj, key, value) => Object.defineProperty(obj, key, { value, writable: true, configurable: true });

const hasOwn = (o, k) => Object.prototype.hasOwnProperty.call(o, k);

const isInt = (v) => typeof v === 'number' && Number.isInteger(v);

/** `true` if the string is exactly one Unicode code point */
const isOneChar = (s) => {
  if (typeof s !== 'string') return false;
  if (s.length === 1) return true;
  if (s.length === 2) {
    const c = s.charCodeAt(0);
    const d = s.charCodeAt(1);
    return c >= 0xd800 && c <= 0xdbff && d >= 0xdc00 && d <= 0xdfff;
  }
  return false;
};

const assertOneChar = (char) => {
  if (!isOneChar(char)) {
    throw new TypeError(`char2cp() expected a single character, got ${JSON.stringify(char)}`);
  }
};

const isBinary = (s) => ArrayBuffer.isView(s) || Object.prototype.toString.call(s) === '[object ArrayBuffer]';

/** `Uint8Array` view of bytes (works across realms) */
const toBytes = (data, what = 'Expected a Uint8Array, an ArrayBuffer or a typed array') => {
  if (ArrayBuffer.isView(data)) {
    return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  }
  if (Object.prototype.toString.call(data) === '[object ArrayBuffer]') {
    return new Uint8Array(data);
  }
  throw new TypeError(what);
};

const isGzip = (b) => b.length >= 2 && b[0] === 0x1f && b[1] === 0x8b;

const magicOf = (b) => String.fromCharCode(...b.subarray(0, 4));

/** Range filter → plain data (`null` for none). */
const rangeArg = (r) => (r === undefined ? null : r);

/** A value copied out of a cached projection (arrays are copied). */
const fresh = (v) => (Array.isArray(v) ? [...v] : v);

/** Deep equality of two meta slot values. */
const slotEquals = (a, b) => {
  if (Array.isArray(a) && Array.isArray(b)) {
    return a.length === b.length && a.every((v, i) => v === b[i]);
  }
  return a === b;
};

/** 17-slot list → meta object */
const listToMeta = (list) => {
  const d = {};
  META_TITLES.forEach((k, i) => {
    d[k] = Array.isArray(list[i]) ? [...list[i]] : list[i];
  });
  return d;
};

/** Meta fields (only the 17 keys) of an object, copying `hexdata`. */
const metaFields = (obj) => {
  const out = {};
  for (const k of Object.keys(obj)) {
    if (META_INDEX.has(k)) {
      const v = obj[k];
      out[k] = k === 'hexdata' && Array.isArray(v) ? [...v] : v;
    }
  }
  return out;
};

/* ------------------------------------------------------------------------ */
/* Bitmap                                                                   */
/* ------------------------------------------------------------------------ */

const FROMSTRING_MAP = { '#': '1', '.': '0', '&': '2', 0: '0', 1: '1', 2: '2' };

// Python's str.isspace() characters (bdfparser-js `pyStrip`)
const PY_SPACE = '\\t\\n\\v\\f\\r\\x1c-\\x1f \\x85\\xa0\\u1680\\u2000-\\u200a\\u2028\\u2029\\u202f\\u205f\\u3000';
const RE_PY_STRIP = new RegExp(`^[${PY_SPACE}]+|[${PY_SPACE}]+$`, 'g');
const pyStrip = (s) => s.replace(RE_PY_STRIP, '');

const DEFAULT_PIXELCOLORS = { 0: null, 1: 'black', 2: 'red' };

const rawRows = (rows) => {
  if (!Array.isArray(rows) || !rows.every((r) => typeof r === 'string')) {
    throw new TypeError('Bitmap() expects an array of row strings');
  }
  for (let i = 0; i < rows.length; i++) {
    const m = /[^012]/.exec(rows[i]);
    if (m !== null) {
      throw new Error(
        `Invalid character ${JSON.stringify(m[0])} in bitmap row ${i} (only '0', '1' and '2' are pixels)`,
      );
    }
  }
  if (rows.some((r) => r.length !== rows[0].length)) {
    throw new Error('Bitmap rows have different lengths');
  }
  return rows;
};

const checkxy = (b, x, y) => {
  const w = b.width();
  const h = b.height();
  if (!Number.isInteger(x) || !Number.isInteger(y) || y < 0 || y >= h || x < 0 || x >= w) {
    throw new RangeError(`Pixel (${x}, ${y}) is out of range of the ${w}x${h} bitmap`);
  }
};

const bitmapOf = (raw) => {
  const b = Object.create(Bitmap.prototype);
  initBitmap(b, raw);
  return b;
};

const initBitmap = (b, raw) => {
  hidden(b, S, raw);
  Object.defineProperty(b, 'bindata', {
    get: () => b[S].bindata(),
    set: (rows) => {
      guard(() => b[S].setBindata(rawRows(rows)));
    },
    enumerable: true,
    configurable: true,
  });
};

/** The raw handle of a Bitmap (or a new one from a bindata array). */
const rawBitmapOf = (b) => {
  if (b instanceof Bitmap) return b[S];
  if (b !== null && typeof b === 'object' && Array.isArray(b.bindata)) {
    return guard(() => new RawBitmap(rawRows(b.bindata)));
  }
  throw new TypeError('expected a Bitmap');
};

/**
 * `Bitmap` object: rows of `'0'`, `'1'`, `'2'` (row 0 at the top) backed by a
 * native raster. `bindata` builds the row strings on each read (assign to
 * replace the raster). Editing methods return the bitmap for chaining.
 */
export class Bitmap {
  constructor(bin_bitmap_list) {
    initBitmap(this, guard(() => new RawBitmap(rawRows(bin_bitmap_list))));
  }

  static fromstring(text) {
    const lines = String(text).replace(/\r/g, '').split('\n');
    while (lines.length > 0 && pyStrip(lines[0]) === '') lines.shift();
    while (lines.length > 0 && pyStrip(lines[lines.length - 1]) === '') lines.pop();
    const rows = lines.map((line, i) => {
      let row = '';
      for (const c of line) {
        const v = FROMSTRING_MAP[c];
        if (v === undefined) {
          throw new Error(`Invalid character ${JSON.stringify(c)} in bitmap row ${i}`);
        }
        row += v;
      }
      return row;
    });
    if (rows.some((r) => r.length !== rows[0].length)) {
      throw new Error('Bitmap rows have different lengths');
    }
    return bitmapOf(guard(() => RawBitmap.fromstring(rows.join('\n'))));
  }

  static blank(w, h) {
    if (w < 0 || h < 0) {
      throw new RangeError('Bitmap size must not be negative');
    }
    return bitmapOf(guard(() => RawBitmap.blank(w, h)));
  }

  toString() {
    return this[S].toString();
  }

  repr() {
    return `Bitmap(${JSON.stringify(this.bindata, null, 2)})`;
  }

  equals(other) {
    return other instanceof Bitmap ? this[S].equals(other[S]) : slotEquals(this.bindata, other?.bindata);
  }

  width() {
    return this[S].width();
  }

  height() {
    return this[S].height();
  }

  clone() {
    return bitmapOf(this[S].clone());
  }

  crop(w, h, xoff, yoff) {
    if (w < 0 || h < 0) {
      throw new RangeError(`Bitmap.crop: negative width or height (${String(w)}, ${String(h)})`);
    }
    guard(() => this[S].crop(w, h, xoff ?? 0, yoff ?? 0));
    return this;
  }

  overlay(bitmap) {
    const other = rawBitmapOf(bitmap);
    if (this[S].height() !== other.height()) {
      warn('the bitmaps to overlay have different height');
    }
    if (this[S].width() !== other.width()) {
      warn('the bitmaps to overlay have different width');
    }
    guard(() => this[S].overlay(other));
    return this;
  }

  static concatall(bitmaplist, options = {}) {
    const list = new RawBitmapList();
    try {
      for (const b of bitmaplist) list.push(rawBitmapOf(b));
      return bitmapOf(
        guard(() => list.concatall(options.direction ?? 1, options.align ?? 1, options.offsetlist ?? [])),
      );
    } finally {
      list.free();
    }
  }

  concat(bitmap, options = {}) {
    const { direction, align, offset } = options;
    guard(() => this[S].concat(rawBitmapOf(bitmap), direction ?? 1, align ?? 1, offset ?? 0));
    return this;
  }

  enlarge(x, y) {
    guard(() => this[S].enlarge(x ?? 1, y ?? 1));
    return this;
  }

  replace(substr, newsubstr) {
    guard(() => this[S].replace(String(substr), String(newsubstr)));
    return this;
  }

  shadow(xoff, yoff) {
    guard(() => this[S].shadow(xoff ?? 1, yoff ?? -1));
    return this;
  }

  glow(mode) {
    guard(() => this[S].glow(mode ?? 0));
    return this;
  }

  bytepad(bits) {
    guard(() => this[S].bytepad(bits ?? 8));
    return this;
  }

  todata(datatype) {
    const t = datatype ?? 1;
    if (t === 4 || t === 5) {
      const rows = this.bindata;
      for (const s of rows) {
        if (!/^[01]*$/.test(s)) {
          throw new Error(`Invalid binary string: ${s}`);
        }
        if (t === 5 && s.length > 53) {
          // a permanent language difference (SPEC-v3 §11.2)
          throw new RangeError(
            `todata(5): rows wider than 53 pixels cannot be represented exactly as numbers (row width ${s.length})`,
          );
        }
      }
    }
    if (![0, 1, 2, 3, 4, 5].includes(t)) {
      return undefined;
    }
    return guard(() => this[S].todata(t));
  }

  getpixel(x, y) {
    checkxy(this, x, y);
    return this[S].getpixel(x, y);
  }

  setpixel(x, y, value = 1) {
    if (value !== 0 && value !== 1 && value !== 2) {
      throw new RangeError(`Pixel value must be 0, 1 or 2, not ${String(value)}`);
    }
    checkxy(this, x, y);
    this[S].setpixel(x, y, value);
    return this;
  }

  fliph() {
    guard(() => this[S].fliph());
    return this;
  }

  flipv() {
    guard(() => this[S].flipv());
    return this;
  }

  rotate(angle = 90) {
    if (!Number.isInteger(angle) || angle % 90 !== 0) {
      throw new Error(`Rotation angle must be a multiple of 90, not ${String(angle)}`);
    }
    guard(() => this[S].rotate(angle));
    return this;
  }

  invert() {
    this[S].invert();
    return this;
  }

  shift(dx = 0, dy = 0) {
    guard(() => this[S].shift(dx, dy));
    return this;
  }

  bbox() {
    return this[S].bbox();
  }

  trim() {
    guard(() => this[S].trim());
    return this;
  }

  bold(n = 1) {
    if (n < 0) {
      throw new RangeError('n must not be negative');
    }
    guard(() => this[S].bold(n));
    return this;
  }

  italic(step = 2, base = 0) {
    if (!(step > 0)) {
      throw new RangeError('step must be a positive integer');
    }
    guard(() => this[S].italic(step, base));
    return this;
  }

  paste(bitmap, x = 0, y = 0) {
    guard(() => this[S].paste(rawBitmapOf(bitmap), x, y));
    return this;
  }

  /**
   * Draw the bitmap on a canvas-like context (`fillStyle`, `fillRect`):
   * `options = {x, y, pixelsize, pixelcolors}`; the v2 form
   * `draw2canvas(context, pixelcolors, xoff, yoff)` is accepted too.
   */
  draw2canvas(context, options, xoff, yoff) {
    let opts;
    if (options !== null && options !== undefined && ('0' in options || '1' in options || '2' in options)) {
      opts = { pixelcolors: options, x: xoff, y: yoff };
    } else {
      opts = options ?? { x: xoff, y: yoff };
    }
    const colors = opts.pixelcolors ?? DEFAULT_PIXELCOLORS;
    const x0 = opts.x ?? 0;
    const y0 = opts.y ?? 0;
    const ps = opts.pixelsize ?? 1;
    let current;
    this.bindata.forEach((line, row) => {
      for (let col = 0; col < line.length; col++) {
        const s = line[col];
        if (s === '0' || s === '1' || s === '2') {
          const color = colors[s];
          if (color !== null && color !== undefined) {
            if (color !== current) {
              context.fillStyle = color;
              current = color;
            }
            context.fillRect(x0 + col * ps, y0 + row * ps, ps, ps);
          }
        }
      }
    });
    return this;
  }

  tosvg(pixelsize = 1, format = 'path', inverted = false, offsets = {}, shape = 'square') {
    if (format !== 'path' && format !== 'pathOnly' && format !== 'rect') {
      throw new Error(`Unknown SVG format ${JSON.stringify(format)}`);
    }
    if (shape !== 'square' && shape !== 'circle') {
      throw new Error(`Unknown shape ${JSON.stringify(shape)}`);
    }
    if (format === 'rect' && shape !== 'square') {
      throw new Error("format 'rect' only supports shape 'square'");
    }
    return guard(() => this[S].tosvg(pixelsize, format, inverted, offsets.x ?? 0, offsets.y ?? 0, shape));
  }
}

/** `fmtnum(n)`: integral numbers without a fraction, others in Python's `repr` style. */
export const fmtnum = (n) => {
  if (typeof n === 'bigint') {
    return n.toString();
  }
  if (Number.isInteger(n) && !Number.isSafeInteger(n)) {
    // beyond 2^53 the Rust formatter would see the same float; print it exactly
    return BigInt(n).toString();
  }
  return glue.bdfparserFmtnum(n);
};

/* ------------------------------------------------------------------------ */
/* Charsets                                                                 */
/* ------------------------------------------------------------------------ */

const TABLE = glue.bdfparserCharsetTable();

/**
 * Sanitize a charset name (v2 helper): trim, drop one leading `-`, cut at the
 * first `.`, remove `-`, whitespace and non-leading `_`, lower case.
 */
export const sanitizeCharsetName = (charset) => {
  let s = String(charset).trim();
  if (s.startsWith('-')) s = s.slice(1);
  const dot = s.indexOf('.');
  if (dot !== -1) s = s.slice(0, dot);
  return s.replace(/-|\s|(?<!^)_/g, '').toLowerCase();
};

/** Canonical charset key from `CHARSET_REGISTRY` / `CHARSET_ENCODING` (`'iso10646'` without a registry). */
export const sanitizeCharset = (registry, encoding) =>
  glue.bdfparserSanitizeCharset(
    registry === null || registry === undefined ? undefined : String(registry),
    encoding === null || encoding === undefined ? undefined : String(encoding),
  );

/** Whether a canonical key is a Unicode charset. */
export const isUnicodeCharset = (key) => glue.bdfparserIsUnicodeCharset(String(key));

/** Canonical key of any charset spelling (`'GB2312'`, `'Shift-JIS'`, `'microsoft-cp1252'`, ...). */
export const canonicalCharset = (name) => glue.bdfparserCanonicalCharset(String(name));

/** `[CHARSET_REGISTRY, CHARSET_ENCODING]` for a canonical key (used by `charsetconvert`). */
export const charsetProps = (key) => glue.bdfparserCharsetProps(String(key));

/** Every known canonical charset key. */
export const charsetKeys = () => TABLE.map((r) => r.key);

/**
 * Accepted for compatibility: fontpixel's exact charset tables (multi-byte
 * included) are compiled into the module, so there is nothing to register.
 */
export const registerCharsetTables = (tables) => {
  if (tables === null || typeof tables !== 'object') {
    throw new TypeError('registerCharsetTables expects an object');
  }
};

const isValidCp = (cp) => typeof cp === 'number' && Number.isInteger(cp) && cp >= 0;

const codecCache = new Map();

/**
 * Character ⇄ codepoint converter of a charset (canonicalized with
 * `canonicalCharset`); `options.gl` selects the 94×94 GL form. Conversions
 * always use fontpixel's exact tables (`exact` is `true`); `options.iconv`
 * is accepted and not needed.
 */
export const getCodec = (charset, options = {}) => {
  const gl = options.gl === null || options.gl === undefined ? undefined : Boolean(options.gl);
  const cacheKey = `${charset}\u0000${gl}`;
  let codec = codecCache.get(cacheKey);
  if (codec !== undefined) {
    return codec;
  }
  const raw = guard(() => RawCodec.open(String(charset), gl));
  const kind = raw.kind;
  const unicode = kind === 'unicode';
  codec = Object.freeze({
    charset: raw.charset,
    unicode,
    gl94: raw.gl94,
    gl: raw.gl,
    known: raw.known,
    kind,
    exact: true,
    cp2char(cp) {
      if (!isValidCp(cp) || cp > 0xffffffff) return null;
      const u = raw.cp2unicode(cp);
      return u === undefined ? null : String.fromCodePoint(u);
    },
    char2cp(char) {
      assertOneChar(char);
      const u = char.codePointAt(0);
      const code = raw.unicode2cp(u);
      return code === undefined ? null : code;
    },
  });
  codecCache.set(cacheKey, codec);
  return codec;
};

/**
 * GL form detection for 94×94 double-byte sets: `CHARSET_ENCODING` `"0"` or
 * the smallest two-byte codepoint with both bytes in 0x21–0x7E.
 */
export const detectGL = (charsetEncoding, cps) => {
  const enc =
    charsetEncoding === null || charsetEncoding === undefined
      ? undefined
      : String(charsetEncoding).replace(/^[ \t\n\v\f\r]+|[ \t\n\v\f\r]+$/g, '');
  // the rule of a GL-capable 94×94 set (GB 2312), independent of the charset
  return glue.bdfparserDetectGl('gb2312', enc, [...cps].filter(isValidCp));
};

const unknownWarned = new Set();

const moduleCodec = (charset) => {
  const codec = getCodec(charset);
  if (!codec.known && !unknownWarned.has(codec.charset)) {
    unknownWarned.add(codec.charset);
    warn(`Unknown charset '${codec.charset}', characters are mapped to codepoints as Unicode`);
  }
  return codec;
};

/** Character → codepoint in a charset (an unknown charset maps as Unicode, one warning per name). */
export const char2cp = (char, charset = 'iso10646', _iconv) => moduleCodec(charset).char2cp(char);

/** Codepoint in a charset → character (an unknown charset maps as Unicode, one warning per name). */
export const cp2char = (cp, charset = 'iso10646', _iconv) => moduleCodec(charset).cp2char(cp);

/* ------------------------------------------------------------------------ */
/* Glyph                                                                    */
/* ------------------------------------------------------------------------ */

/** `uniXXXX` / `uXXXXX` (Unicode) or `char{cp}` (other charsets). */
export const defaultglyphname = (cp, charsetIsUnicode = true) =>
  glue.bdfparserDefaultGlyphName(cp, Boolean(charsetIsUnicode));

const rawFontOf = (font) => (font instanceof Font ? font[S].raw : new RawFont());

const glyphOf = (raw, font) => {
  const g = Object.create(Glyph.prototype);
  initGlyph(g, raw, font, null);
  return g;
};

const initGlyph = (g, raw, font, meta) => {
  hidden(g, S, { raw, meta, epoch: font instanceof Font ? font[S].epoch : 0 });
  Object.defineProperty(g, 'meta', {
    get: () => {
      const st = g[S];
      if (st.meta === null) st.meta = listToMeta(st.raw.meta());
      return st.meta;
    },
    set: (v) => {
      g[S].meta = v;
    },
    enumerable: true,
    configurable: true,
  });
  g.font = font;
};

/** Direct edits of `glyph.meta` not yet in the handle (`{key: value}`). */
const pending = (g) => {
  const st = g[S];
  if (st.meta === null) return null;
  const now = st.raw.meta();
  let out = null;
  META_TITLES.forEach((k, i) => {
    const mine = st.meta[k];
    if (!slotEquals(mine, now[i]) && !(mine === undefined && now[i] === null)) {
      out ??= {};
      out[k] = Array.isArray(mine) ? [...mine] : (mine ?? null);
    }
  });
  return out;
};

/** Write direct `meta` edits into the handle (and its font). */
const push = (g) => {
  const changed = pending(g);
  if (changed !== null) g[S].raw.setmeta(changed);
};

/** Refresh `meta` in place from the handle. */
const pull = (g) => {
  const st = g[S];
  if (st.meta === null) return;
  const now = st.raw.meta();
  META_TITLES.forEach((k, i) => {
    st.meta[k] = Array.isArray(now[i]) ? [...now[i]] : now[i];
  });
};

/** The handle to read from: with pending direct `meta` edits, a detached copy carrying them. */
const reader = (g) => {
  const changed = pending(g);
  if (changed === null) return g[S].raw;
  const c = g[S].raw.clone();
  c.setmeta(changed);
  return c;
};

const fontOfGlyph = (g) => (g.font instanceof Font ? g.font : null);

const editGlyph = (g, op) => {
  const font = fontOfGlyph(g);
  try {
    push(g);
    op(g[S].raw);
  } catch (e) {
    throw translate(e);
  } finally {
    pull(g);
    if (font !== null) changed(font);
  }
  return g;
};

const isAttached = (g) => {
  const font = fontOfGlyph(g);
  return g[S].raw.isAttached() && font !== null && g[S].epoch === font[S].epoch;
};

/** A transform callback adapter: `fn(Bitmap)` editing the raw bitmap it receives. */
const bitmapFn = (fn) => (raw) => {
  const bm = bitmapOf(raw);
  const res = fn(bm);
  if (res !== null && typeof res === 'object' && res !== bm && Array.isArray(res.bindata)) {
    if (res instanceof Bitmap) raw.assign(res[S]);
    else raw.setBindata(rawRows(res.bindata));
  }
};

const DRAW_MODES = [0, 1, 2, -1];

/**
 * `Glyph` object: `meta` (the 17 keys) and `font`. A glyph obtained from a
 * font (`glyphbycp`, `glyph`, `iterglyphs`, `newglyph`, or passed to
 * `addglyph`) is attached: editing methods write through to the font.
 * Direct changes of `meta` are written with the next editing method.
 */
export class Glyph {
  constructor(meta_obj, font) {
    const raw = guard(() => new RawGlyph(metaFields(meta_obj ?? {}), rawFontOf(font)));
    initGlyph(this, raw, font, { ...meta_obj });
  }

  toString() {
    return this.draw().toString();
  }

  repr() {
    return 'Glyph(' + JSON.stringify(this.meta, null, 2) + ', ' + 'Font(<' + String(this.font?.headers?.fontname) + '>' + ')';
  }

  cp() {
    return this.meta.codepoint;
  }

  chr() {
    return this.font.cp2char(this.cp());
  }

  draw(mode, bb) {
    const m = mode ?? 0;
    if (!DRAW_MODES.includes(m)) {
      throw new Error(`Unknown draw mode ${String(m)}`);
    }
    if (m === -1 && (bb === null || bb === undefined)) {
      throw new Error('Parameter bb in draw() method must be set when mode=-1');
    }
    const font = fontOfGlyph(this);
    try {
      return bitmapOf(reader(this).draw(m, bb ?? null));
    } catch (e) {
      throw translate(e);
    } finally {
      if (font !== null) drain(font);
    }
  }

  origin(options = {}) {
    const m = options.mode ?? 0;
    const xoff = options.xoff ?? null;
    const yoff = options.yoff ?? null;
    if (!DRAW_MODES.includes(m)) {
      throw new Error(`Unknown origin mode ${String(m)}`);
    }
    if (m === -1 && (xoff === null || yoff === null)) {
      throw new Error('Parameter xoff and yoff in origin() method must be all set when mode=-1');
    }
    return guard(() => reader(this).origin(m, Boolean(options.fromorigin), xoff, yoff));
  }

  setbitmap(bitmap, mode, bb, trim) {
    let m;
    let box;
    let t;
    if (mode !== null && typeof mode === 'object') {
      m = mode.mode ?? 0;
      box = mode.bb ?? null;
      t = mode.trim ?? true;
    } else {
      m = mode ?? 0;
      box = bb ?? null;
      t = trim ?? true;
    }
    if (!DRAW_MODES.includes(m)) {
      throw new Error(`Unknown setbitmap mode ${String(m)}`);
    }
    if (m === -1 && box === null) {
      throw new Error('Parameter bb in setbitmap() method must be set when mode=-1');
    }
    let raw;
    if (Array.isArray(bitmap)) {
      const rows = bitmap.map((row) => String(row).replace(/[^0]/g, '1'));
      if (rows.some((r) => r.length !== rows[0].length)) {
        throw new Error('Bitmap rows have different lengths');
      }
      raw = guard(() => new RawBitmap(rows));
    } else {
      raw = rawBitmapOf(bitmap);
    }
    return editGlyph(this, (g) => g.setbitmap(raw, m, box, Boolean(t)));
  }

  setmeta(obj) {
    for (const k of Object.keys(obj)) {
      if (!META_INDEX.has(k)) {
        throw new Error(`Unknown glyph meta key "${k}"`);
      }
    }
    for (const [k, v] of Object.entries(obj)) {
      this.meta[k] = k === 'hexdata' && Array.isArray(v) ? [...v] : v;
    }
    const cp = this.meta.codepoint;
    if (cp !== null && cp !== undefined && !Number.isInteger(cp) && isAttached(this)) {
      throw new TypeError(`Glyph codepoint must be an integer, not ${String(cp)}`);
    }
    return editGlyph(this, () => undefined);
  }

  shift(dx = 0, dy = 0) {
    return editGlyph(this, (g) => g.shift(dx, dy));
  }

  transform(fn, mode = 0) {
    return editGlyph(this, (g) => g.transform(bitmapFn(fn), mode));
  }

  clone() {
    const raw = guard(() => reader(this).clone());
    const g = glyphOf(raw, this.font);
    const meta = { ...this.meta };
    if (Array.isArray(meta.hexdata)) meta.hexdata = [...meta.hexdata];
    g[S].meta = meta;
    return g;
  }

  /** @deprecated Use `setbitmap(bitmap, 1, null, false)`. */
  changebitmap(bitmap) {
    return this.setbitmap(bitmap, 1, null, false);
  }

  /** Whether the glyph still writes through to its font's slot. */
  get _attached() {
    return isAttached(this);
  }

  /** The 17-slot list stored in the font, or `null` when detached. */
  get _store() {
    if (!isAttached(this)) return null;
    const font = this.font;
    const cp = this[S].raw.cp();
    if (cp >= 0) return font.glyphs.get(cp) ?? null;
    // an unencoded glyph: the entry holding this glyph's values
    const mine = JSON.stringify(this[S].raw.meta());
    for (const e of font.unencoded) {
      if (JSON.stringify([...e.meta]) === mine) return e.meta;
    }
    return null;
  }
}

/* ------------------------------------------------------------------------ */
/* Font: plumbing                                                           */
/* ------------------------------------------------------------------------ */

// bdfparser-js warns about a glyph's missing bitmap rows once per stored hex
// row list (the list is replaced whenever the glyph is edited)
const PADDED_ROWS = / bitmap rows but its BBX height is \d+; padded with empty rows$/;

/** Pass the warnings the Rust model collected since the last call to the handler. */
const drain = (font) => {
  const st = font[S];
  const [messages, count] = st.raw.warningsSince(st.wcur);
  st.wcur = count;
  for (const m of messages) {
    if (PADDED_ROWS.test(m)) {
      if (st.padded.has(m)) continue;
      st.padded.add(m);
    }
    warn(jsText(m));
  }
};

const changed = (font) => {
  font[S].rev++;
  font[S].padded.clear();
  const views = font[S].views;
  if (views !== null) {
    views.headersLive.dirty();
    views.propsLive.dirty();
  }
  drain(font);
};

/** Run `fn(raw)` on a font: errors translated, warnings drained, edits counted. */
const call = (font, fn, edits = false) => {
  try {
    return fn(font[S].raw);
  } catch (e) {
    throw translate(e);
  } finally {
    if (edits) changed(font);
    else drain(font);
  }
};

/** Projections of the model, reused until the next edit. */
const cache = (font) => {
  const st = font[S];
  let c = st.cache;
  if (c === null || c.rev !== st.rev) {
    c = {
      rev: st.rev,
      cps: null,
      cpset: null,
      orders: new Map(),
      metas: new Map(),
      headers: null,
      props: null,
      model: null,
      unencoded: null,
    };
    st.cache = c;
  }
  return c;
};

const cplist = (font) => {
  const c = cache(font);
  if (c.cps === null) {
    c.cps = font[S].raw.itercps(0, null);
    c.cpset = new Set(c.cps);
  }
  return c.cps;
};

/** Encoded codes in an order (0 file, -1 reverse, 1 ascending, 2 descending), cached per revision. */
const cpsInOrder = (font, order) => {
  const c = cache(font);
  let l = c.orders.get(order);
  if (l === undefined) {
    const file = cplist(font);
    if (order === 0) l = file;
    else if (order === -1) l = [...file].reverse();
    else {
      l = [...file].sort((a, b) => a - b);
      if (order === 2) l.reverse();
    }
    c.orders.set(order, l);
  }
  return l;
};

const cpset = (font) => {
  cplist(font);
  return cache(font).cpset;
};

const modelOf = (font) => {
  const c = cache(font);
  if (c.model === null) c.model = font[S].raw.model();
  return c.model;
};

/**
 * Install a freshly loaded model. With `binary`, the notes of the native PCF
 * decoder (`pcf.read.*`: unknown or duplicate tables, format words) stay in
 * the report only: bdfparser's binary readers emit no warnings.
 */
const setRaw = (font, raw, binary = false) => {
  const st = font[S];
  st.raw = raw;
  st.epoch++;
  st.wcur = 0;
  st.short = new Set();
  st.names = new Map();
  let early = [];
  if (binary) {
    const entries = raw.report().entries;
    early = entries.filter((e) => e.level !== 'info' && !e.code.startsWith('pcf.read.')).map((e) => e.message);
    st.wcur = entries.length;
  }
  if (st.charset) raw.encoding(st.charset);
  applyConverters(font);
  for (const m of early) warn(jsText(m));
  changed(font);
  return font;
};

const applyConverters = (font) => {
  const st = font[S];
  if (st.char2cp || st.cp2char) {
    st.raw.setConverters(st.char2cp ?? undefined, st.cp2char ?? undefined);
  }
};

const fontOf = (raw, like, binary = false) => {
  const font = new Font();
  if (like instanceof Font) {
    const a = like[S];
    const b = font[S];
    b.iconv = a.iconv;
    b.charset = a.charset;
    b.char2cp = a.char2cp;
    b.cp2char = a.cp2char;
  }
  return setRaw(font, raw, binary);
};

/* ------------------------------------------------------------------------ */
/* Font: live views                                                         */
/* ------------------------------------------------------------------------ */

const headersData = (font) => {
  const c = cache(font);
  if (c.headers === null) c.headers = font[S].raw.headers();
  return c.headers;
};

/**
 * A live view (`font.headers`, `font.props`) over a projection cached per edit
 * revision (`data(font)`). Writes always go through `writes` (the model is
 * edited in place). Reads go straight to `target`, a plain object mirroring
 * the projection's keys (array values as accessors returning a copy, so a
 * read never hands out the cached array): while the font has edits the view
 * has not seen (`dirty`, called by `changed`), the read traps are installed,
 * and the first read refreshes the mirror and removes them again. A Proxy
 * with no read traps reads its target without calling into JS.
 *
 * Symbol-keyed values live on the target unprojected, as before; once one is
 * set the view keeps the read traps (and no mirror) for good, so symbols stay
 * out of `ownKeys` and property descriptors as before.
 */
const READ_TRAPS = ['get', 'has', 'ownKeys', 'getOwnPropertyDescriptor'];

const liveView = (font, target, data, legacyReads, writes) => {
  let mirrored = -1;
  let legacy = false;
  const refresh = () => {
    const rev = font[S].rev;
    if (legacy || mirrored === rev) return;
    const d = data(font);
    for (const k of Object.getOwnPropertyNames(target)) delete target[k];
    for (const k of Object.keys(d)) {
      const v = d[k];
      Object.defineProperty(
        target,
        k,
        Array.isArray(v)
          ? { get: () => [...v], enumerable: true, configurable: true }
          : { value: v, writable: true, enumerable: true, configurable: true },
      );
    }
    mirrored = rev;
    for (const t of READ_TRAPS) handler[t] = undefined;
  };
  const reads = {
    get(t, k, r) {
      refresh();
      return legacy ? legacyReads.get(t, k, r) : Reflect.get(t, k, r);
    },
    has(t, k) {
      refresh();
      return legacy ? legacyReads.has(t, k) : Reflect.has(t, k);
    },
    ownKeys(t) {
      refresh();
      return legacy ? legacyReads.ownKeys(t) : Reflect.ownKeys(t);
    },
    getOwnPropertyDescriptor(t, k) {
      refresh();
      return legacy ? legacyReads.getOwnPropertyDescriptor(t, k) : Reflect.getOwnPropertyDescriptor(t, k);
    },
  };
  const toLegacy = () => {
    if (legacy) return;
    legacy = true;
    for (const k of Object.getOwnPropertyNames(target)) delete target[k];
    Object.assign(handler, reads);
  };
  const handler = {
    ...reads,
    set(t, k, v) {
      if (typeof k !== 'string') {
        toLegacy();
        return Reflect.set(t, k, v);
      }
      writes.set(k, v);
      return true;
    },
    deleteProperty(t, k) {
      if (typeof k !== 'string') return Reflect.deleteProperty(t, k);
      writes.delete(k);
      return true;
    },
    defineProperty(t, k, desc) {
      if (typeof k !== 'string' || !('value' in desc)) return false;
      writes.set(k, desc.value);
      return true;
    },
  };
  const proxy = new Proxy(target, handler);
  return {
    proxy,
    dirty() {
      if (handler.get === undefined) Object.assign(handler, reads);
    },
  };
};

const headersView = (font) =>
  liveView(
    font,
    {},
    headersData,
    {
      get(t, k, r) {
        if (typeof k === 'string') {
          const d = headersData(font);
          if (hasOwn(d, k)) return fresh(d[k]);
        }
        return Reflect.get(t, k, r);
      },
      has(t, k) {
        return (typeof k === 'string' && hasOwn(headersData(font), k)) || Reflect.has(t, k);
      },
      ownKeys() {
        return Object.keys(headersData(font));
      },
      getOwnPropertyDescriptor(t, k) {
        const d = headersData(font);
        if (typeof k !== 'string' || !hasOwn(d, k)) return undefined;
        return { value: fresh(d[k]), writable: true, enumerable: true, configurable: true };
      },
    },
    {
      set: (k, v) => call(font, (raw) => raw.setHeader(k, v), true),
      delete: (k) => call(font, (raw) => raw.removeHeader(k), true),
    },
  );

const propsData = (font) => {
  const c = cache(font);
  if (c.props === null) c.props = font[S].raw.props();
  return c.props;
};

/** The spelling a property key is written with. */
const propName = (font, k) => {
  if (k === 'comment') return k;
  const st = font[S];
  if (st.names.has(k)) return st.names.get(k);
  for (const [lower, name] of modelOf(font).propnames) {
    if (lower === k) return name;
  }
  return k.toUpperCase();
};

const setProp = (font, k, v) => {
  call(font, (raw) => raw.setprop(propName(font, k), v), true);
};

const propsView = (font) =>
  liveView(
    font,
    Object.create(null),
    propsData,
    {
      get(t, k) {
        if (typeof k === 'string') {
          const d = propsData(font);
          if (hasOwn(d, k)) return fresh(d[k]);
        }
        return undefined;
      },
      has(t, k) {
        return typeof k === 'string' && hasOwn(propsData(font), k);
      },
      ownKeys() {
        return Object.keys(propsData(font));
      },
      getOwnPropertyDescriptor(t, k) {
        const d = propsData(font);
        if (typeof k !== 'string' || !hasOwn(d, k)) return undefined;
        return { value: fresh(d[k]), writable: true, enumerable: true, configurable: true };
      },
    },
    {
      set: (k, v) => setProp(font, k, v),
      delete: (k) => {
        if (hasOwn(propsData(font), k)) call(font, (raw) => raw.delprop(k), true);
      },
    },
  );

/**
 * A 17-slot meta list whose item assignment is written back through `write`
 * (`(fields) => void`); other array operations act on the copy.
 */
const metaList = (values, write) =>
  new Proxy(values, {
    set(t, k, v) {
      const ok = Reflect.set(t, k, v);
      if (typeof k === 'string' && /^(0|[1-9][0-9]?)$/.test(k) && Number(k) < 17) {
        write({ [META_TITLES[Number(k)]]: v });
      }
      return ok;
    },
  });

/** The live meta list of encoded glyph `cp`, or undefined. */
const encodedMeta = (font, cp) => {
  const c = cache(font);
  let m = c.metas.get(cp);
  if (m === undefined) {
    if (!cpset(font).has(cp)) return undefined;
    const raw = font[S].raw.lookup(cp);
    if (raw === undefined) return undefined;
    const values = raw.meta();
    raw.free();
    m = metaList(values, (fields) => {
      const g = font[S].raw.lookup(cp);
      if (g === undefined) return;
      try {
        call(font, () => g.setmeta(fields), true);
      } finally {
        g.free();
      }
    });
    c.metas.set(cp, m);
  }
  return m;
};

/** `font.glyphs`: a Map view, codepoint → 17-slot meta list, file order. */
class GlyphsView extends Map {
  constructor(font) {
    super();
    hidden(this, S, font);
  }
  get size() {
    return cplist(this[S]).length;
  }
  get(cp) {
    return isInt(cp) ? encodedMeta(this[S], cp) : undefined;
  }
  has(cp) {
    return cpset(this[S]).has(cp);
  }
  set(cp, list) {
    const font = this[S];
    const meta = [...list];
    const fields = {};
    META_TITLES.forEach((k, i) => {
      if (i < meta.length) fields[k] = Array.isArray(meta[i]) ? [...meta[i]] : meta[i];
    });
    fields.codepoint = cp;
    call(font, (raw) => raw.addglyph(fields, true), true);
    // the native model always holds 17 slots; remember a shorter list for
    // the PCF writer's check (bdfparser-js stores the list as given)
    if (meta.length < META_TITLES.length) font[S].short.add(cp);
    else font[S].short.delete(cp);
    return this;
  }
  delete(cp) {
    const font = this[S];
    if (!this.has(cp)) return false;
    call(font, (raw) => raw.removeglyph(cp), true);
    font[S].short.delete(cp);
    return true;
  }
  clear() {
    const font = this[S];
    const cps = [...cplist(font)];
    call(
      font,
      (raw) => {
        for (const cp of cps) raw.removeglyph(cp);
      },
      true,
    );
    font[S].short.clear();
  }
  *keys() {
    yield* [...cplist(this[S])];
  }
  *values() {
    for (const [, m] of this.entries()) yield m;
  }
  *entries() {
    const font = this[S];
    for (const cp of [...cplist(font)]) {
      const m = encodedMeta(font, cp);
      if (m !== undefined) yield [cp, m];
    }
  }
  [Symbol.iterator]() {
    return this.entries();
  }
  forEach(callback, thisArg) {
    for (const [cp, m] of this.entries()) callback.call(thisArg, m, cp, this);
  }
}

/** `font._propnames`: a Map view, lower-case key → original spelling. */
class PropnamesView extends Map {
  constructor(font) {
    super();
    hidden(this, S, font);
  }
  #pairs() {
    const font = this[S];
    const out = new Map(modelOf(font).propnames);
    for (const [k, v] of font[S].names) {
      if (!out.has(k)) out.set(k, v);
    }
    return out;
  }
  get size() {
    return this.#pairs().size;
  }
  get(k) {
    return this.#pairs().get(k);
  }
  has(k) {
    return this.#pairs().has(k);
  }
  set(k, name) {
    renameProp(this[S], k, name);
    return this;
  }
  delete(k) {
    const font = this[S];
    const had = this.has(k);
    font[S].names.delete(k);
    if (had && hasOwn(propsData(font), k)) renameProp(font, k, k.toUpperCase());
    return had;
  }
  clear() {
    for (const k of [...this.keys()]) this.delete(k);
  }
  *keys() {
    yield* this.#pairs().keys();
  }
  *values() {
    yield* this.#pairs().values();
  }
  *entries() {
    yield* this.#pairs().entries();
  }
  [Symbol.iterator]() {
    return this.entries();
  }
  forEach(callback, thisArg) {
    for (const [k, v] of this.#pairs()) callback.call(thisArg, v, k, this);
  }
}

/** Give property `k` the spelling `name` (rebuilding the property list in order). */
const renameProp = (font, k, name) => {
  const st = font[S];
  if (!hasOwn(propsData(font), k) || k === 'comment') {
    st.names.set(k, name);
    return;
  }
  const props = propsData(font);
  const names = new Map(modelOf(font).propnames);
  names.set(k, name);
  call(
    font,
    (raw) => {
      for (const key of Object.keys(props)) raw.delprop(key);
      for (const [key, value] of Object.entries(props)) {
        raw.setprop(key === 'comment' ? key : (names.get(key) ?? key.toUpperCase()), value);
      }
    },
    true,
  );
};

/** One `font.unencoded` entry: `altcode` and `meta` write back to the glyph. */
const unencodedEntry = (font, index, altcode, values, handle) => {
  const e = {};
  hidden(e, S, { raw: handle, altcode, index });
  Object.defineProperty(e, 'altcode', {
    get: () => e[S].altcode,
    set: (v) => {
      e[S].altcode = v;
      if (handle.isAttached()) call(font, (raw) => raw.setUnencodedAltcode(e[S].index, v ?? null), true);
    },
    enumerable: true,
    configurable: true,
  });
  e.meta = metaList(values, (fields) => {
    if (handle.isAttached()) call(font, () => handle.setmeta(fields), true);
  });
  return e;
};

const unencodedEntries = (font) => {
  const c = cache(font);
  if (c.unencoded === null) {
    const model = modelOf(font);
    const handles = font[S].raw.unencodedHandles();
    c.unencoded = model.unencoded.map(([alt, meta], i) => unencodedEntry(font, i, alt, meta, handles[i]));
  }
  return c.unencoded;
};

/** Make the model's unencoded glyphs match `next` (entries kept by identity). */
const syncUnencoded = (font, next) => {
  const cur = unencodedEntries(font);
  let p = 0;
  while (p < cur.length && p < next.length && next[p] === cur[p]) p++;
  call(
    font,
    (raw) => {
      for (let i = cur.length - 1; i >= p; i--) raw.removeUnencoded(i);
      for (const entry of next.slice(p)) {
        if (entry === undefined || entry === null) continue;
        const isEntry = typeof entry === 'object' && !Array.isArray(entry) && 'meta' in entry;
        const meta = [...(isEntry ? entry.meta : entry)];
        const fields = {};
        META_TITLES.forEach((k, i) => {
          if (i < meta.length) fields[k] = Array.isArray(meta[i]) ? [...meta[i]] : meta[i];
        });
        fields.codepoint = -1;
        raw.putWithAlt(fields, isEntry ? (entry.altcode ?? null) : null);
      }
    },
    true,
  );
};

/** `font.unencoded`: an array view whose mutations edit the font. */
const unencodedView = (font) => {
  const target = [...unencodedEntries(font)];
  const sync = () => {
    syncUnencoded(font, [...target]);
    target.splice(0, target.length, ...unencodedEntries(font));
  };
  return new Proxy(target, {
    set(t, k, v) {
      const ok = Reflect.set(t, k, v);
      if (k === 'length' || (typeof k === 'string' && /^(0|[1-9][0-9]*)$/.test(k))) sync();
      return ok;
    },
    deleteProperty(t, k) {
      const ok = Reflect.deleteProperty(t, k);
      if (typeof k === 'string' && /^(0|[1-9][0-9]*)$/.test(k)) sync();
      return ok;
    },
  });
};

/* ------------------------------------------------------------------------ */
/* Font                                                                     */
/* ------------------------------------------------------------------------ */

const ORDERS = [-1, 0, 1, 2];

const drawKw = (font, options) => {
  const kw = {
    linelimit: options.linelimit ?? 512,
    mode: options.mode ?? 1,
    direction: options.direction ?? 'lrtb',
    ucgs: Boolean(options.usecurrentglyphspacing ?? false),
  };
  const missing = options.missing ?? null;
  if (missing) {
    kw.missing = missing instanceof Glyph ? listToMeta(reader(missing).meta()) : metaFields(missing);
  }
  if (!hasOwn(headersData(font), 'fbbx')) {
    throw new Error('Font is not loaded');
  }
  return kw;
};

/**
 * `Font` object. `headers`, `props`, `glyphs`, `unencoded` and `_propnames`
 * are live views of the native model.
 */
export class Font {
  constructor() {
    hidden(this, S, {
      raw: new RawFont(),
      wcur: 0,
      rev: 0,
      epoch: 0,
      cache: null,
      short: new Set(),
      names: new Map(),
      padded: new Set(),
      iconv: undefined,
      charset: null,
      char2cp: null,
      cp2char: null,
      views: null,
    });
    const headersLive = headersView(this);
    const propsLive = propsView(this);
    const views = {
      headers: headersLive.proxy,
      props: propsLive.proxy,
      headersLive,
      propsLive,
      glyphs: new GlyphsView(this),
      propnames: new PropnamesView(this),
    };
    this[S].views = views;
    Object.defineProperties(this, {
      headers: {
        get: () => views.headers,
        set: (value) => call(this, (raw) => raw.setHeaders({ ...value }), true),
        enumerable: true,
        configurable: true,
      },
      props: {
        get: () => views.props,
        set: (value) => {
          const entries = Object.entries(value);
          // the remembered spellings stay (bdfparser-js keeps `_propnames`)
          const names = new Map(modelOf(this).propnames);
          call(
            this,
            (raw) => {
              for (const k of Object.keys(raw.props())) raw.delprop(k);
              for (const [k, v] of entries) raw.setprop(k === 'comment' ? k : (names.get(k) ?? k.toUpperCase()), v);
            },
            true,
          );
        },
        enumerable: true,
        configurable: true,
      },
      glyphs: {
        get: () => views.glyphs,
        set: (value) => {
          const items = [...value].map(([cp, m]) => [cp, [...m]]);
          views.glyphs.clear();
          for (const [cp, m] of items) views.glyphs.set(cp, m);
        },
        enumerable: true,
        configurable: true,
      },
      unencoded: {
        get: () => unencodedView(this),
        set: (value) => syncUnencoded(this, [...value].map((e) => ({ altcode: e.altcode ?? null, meta: [...e.meta] }))),
        enumerable: true,
        configurable: true,
      },
      _propnames: {
        get: () => views.propnames,
        set: (value) => {
          for (const [k, v] of value) views.propnames.set(k, v);
        },
        enumerable: true,
        configurable: true,
      },
    });
  }

  // ---------------------------------------------------------------- loading

  async load_filelines(filelines) {
    const parts = [];
    for await (const line of filelines) {
      const s = String(line);
      parts.push(/[\r\n]$/.test(s) ? s : s + '\n');
    }
    return this.load_string(parts.join(''));
  }

  load_string(text) {
    // the Rust loader skips one leading BOM, as bdfparser-js
    const t = String(text);
    const raw = new RawFont();
    guard(() => raw.loadString(t));
    return setRaw(this, raw);
  }

  load_bytes(data, options = {}) {
    const b = toBytes(data);
    const magic = magicOf(b);
    if (magic === '\x01fcp') {
      return setRaw(this, pcfRaw(b), true);
    }
    if (isGzip(b)) {
      throw new Error(
        'gzip-compressed data cannot be loaded synchronously: decompress it first, or use load_url() / $Font()',
      );
    }
    if (magic === 'OTTO') {
      throw new Error('CFF-based OpenType is not supported');
    }
    if (magic === '\x00\x01\x00\x00' || magic === 'true') {
      return setRaw(this, ttfRaw(b, { pixelsize: options.pixelsize }));
    }
    // BDF text (UTF-8, invalid sequences replaced; one leading BOM skipped)
    const raw = new RawFont();
    guard(() => raw.loadBytes(b, options.pixelsize ?? undefined));
    return setRaw(this, raw);
  }

  async load_url(url, init, options = {}) {
    const href = typeof url === 'string' ? url : url.href;
    const res = await fetch(href, init);
    if (!res.ok) {
      throw new Error(`Failed to fetch ${href}: ${res.status} ${res.statusText}`);
    }
    return this.load_bytes_async(new Uint8Array(await res.arrayBuffer()), options);
  }

  async load_bytes_async(data, options = {}) {
    const b = toBytes(data);
    if (!isGzip(b)) {
      return this.load_bytes(b, options);
    }
    // gzip transport is decoded by the Rust loader (bounded, no DecompressionStream)
    return setRaw(this, gzipRaw(b, options), true);
  }

  static create(options = {}) {
    const o = {};
    for (const k of ['pixelsize', 'ascent', 'descent', 'fontname', 'familyname', 'xres', 'yres']) {
      if (options[k] !== undefined && options[k] !== null) o[k] = options[k];
    }
    return fontOf(guard(() => RawFont.create(o)));
  }

  // --------------------------------------------------------------- encoding

  encoding(options = {}) {
    const st = this[S];
    if (options.iconv !== undefined) st.iconv = options.iconv;
    if (options.charset !== undefined) {
      st.charset = options.charset ? canonicalCharset(options.charset) : null;
      st.raw.encoding(st.charset ?? '');
    }
    if (options.char2cp !== undefined) st.char2cp = options.char2cp;
    if (options.cp2char !== undefined) st.cp2char = options.cp2char;
    if (options.char2cp !== undefined || options.cp2char !== undefined) {
      st.raw.setConverters(st.char2cp ?? undefined, st.cp2char ?? undefined);
    }
    changed(this);
    return this;
  }

  get charset() {
    return this[S].raw.charset();
  }

  set charset(value) {
    this.encoding({ charset: value });
  }

  char2cp(char) {
    const f = this[S].char2cp;
    if (f) return f(char) ?? null;
    assertOneChar(char);
    const code = call(this, (raw) => raw.unicode2cp(char.codePointAt(0)));
    return code === undefined ? null : code;
  }

  cp2char(cp) {
    const f = this[S].cp2char;
    if (f) return f(cp) ?? null;
    if (!isValidCp(cp)) return null;
    const u = call(this, (raw) => raw.cp2unicode(cp));
    return u === undefined ? null : String.fromCodePoint(u);
  }

  // ------------------------------------------------------------- properties

  getprop(key) {
    const k = String(key);
    // ASCII keys from the projection cached per revision (Rust lower-cases
    // the same way); other keys keep the Rust case mapping
    if (/^[\x00-\x7f]*$/.test(k)) {
      const d = propsData(this);
      const lower = k.toLowerCase();
      return hasOwn(d, lower) ? fresh(d[lower]) : null;
    }
    const v = this[S].raw.getprop(k);
    return v === undefined ? null : v;
  }

  setprop(key, value) {
    const k = String(key);
    this[S].names.delete(k.toLowerCase());
    call(this, (raw) => raw.setprop(k, value), true);
    return this;
  }

  delprop(key) {
    const k = String(key);
    this[S].names.delete(k.toLowerCase());
    call(this, (raw) => raw.delprop(k), true);
    return this;
  }

  swidth(dwx) {
    return call(this, (raw) => raw.swidth(dwx));
  }

  // --------------------------------------------------------------- querying

  get length() {
    // the encoded codes are cached per edit revision (`Font::length` counts them)
    return cplist(this).length;
  }

  itercps(order, r) {
    const o = order ?? 1;
    if (!ORDERS.includes(o)) {
      throw new Error(`Unknown order ${String(o)}`);
    }
    if (r === undefined || r === null) return [...cpsInOrder(this, o)];
    return call(this, (raw) => raw.itercps(o, rangeArg(r)));
  }

  *iterglyphs(order, r) {
    for (const cp of this.itercps(order, r)) {
      yield this.glyphbycp(cp);
    }
  }

  glyphbycp(codepoint) {
    if (!isInt(codepoint)) return null;
    const raw = this[S].raw.lookup(codepoint);
    return raw === undefined ? null : glyphOf(raw, this);
  }

  glyph(character) {
    const cp = this.char2cp(character);
    return cp === null ? null : this.glyphbycp(cp);
  }

  lacksglyphs(str) {
    const l = [];
    for (const c of str) {
      const cp = this.char2cp(c);
      if (cp === null || !cpset(this).has(cp)) l.push(c);
    }
    return l.length !== 0 ? l : null;
  }

  drawcps(cps, options = {}) {
    const kw = drawKw(this, options);
    const codes = cps.map((cp) => (isInt(cp) ? cp : -1));
    return bitmapOf(call(this, (raw) => raw.drawcps(codes, kw)));
  }

  draw(str, options = {}) {
    if (this[S].char2cp) {
      return this.drawcps(
        Array.from(str, (c) => this.char2cp(c) ?? -1),
        options,
      );
    }
    const kw = drawKw(this, options);
    return bitmapOf(call(this, (raw) => raw.draw(String(str), kw)));
  }

  drawall(options = {}) {
    const { order, r, linelimit, mode, direction, usecurrentglyphspacing } = options;
    const o = order ?? 1;
    if (!ORDERS.includes(o)) {
      throw new Error(`Unknown order ${String(o)}`);
    }
    const kw = drawKw(this, { linelimit, mode: mode ?? 0, direction, usecurrentglyphspacing });
    kw.order = o;
    kw.r = rangeArg(r ?? null);
    return bitmapOf(call(this, (raw) => raw.drawall(kw)));
  }

  // ---------------------------------------------------------------- editing

  addglyph(glyph, replace = true) {
    const rep = typeof replace === 'object' && replace !== null ? (replace.replace ?? true) : Boolean(replace);
    const isGlyph = glyph instanceof Glyph;
    const meta = isGlyph ? glyph.meta : glyph;
    for (const k of Object.keys(meta)) {
      if (!META_INDEX.has(k)) {
        throw new Error(`Unknown glyph meta key "${k}"`);
      }
    }
    const cp = meta.codepoint;
    if (!isInt(cp)) {
      throw new Error('The glyph has no codepoint');
    }
    if (cp >= 0 && !rep && this.glyphs.has(cp)) {
      warn(`Glyph with codepoint ${cp} already exists, not replaced`);
      return this;
    }
    if (isGlyph) {
      try {
        push(glyph);
        call(this, (raw) => raw.addHandle(glyph[S].raw, rep), true);
      } finally {
        if (glyph[S].raw.isAttached()) {
          glyph.font = this;
          glyph[S].epoch = this[S].epoch;
          glyph[S].meta = null;
        }
        pull(glyph);
      }
    } else {
      call(this, (raw) => raw.addglyph(metaFields(glyph), rep), true);
    }
    this[S].short.delete(cp);
    return this;
  }

  removeglyph(cpOrGlyph) {
    if (cpOrGlyph instanceof Glyph) {
      if (cpOrGlyph.font === this) {
        call(this, (raw) => raw.removeHandle(cpOrGlyph[S].raw), true);
      } else {
        const cp = cpOrGlyph.cp();
        if (isInt(cp) && cp >= 0) call(this, (raw) => raw.removeglyph(cp), true);
      }
    } else if (isInt(cpOrGlyph)) {
      call(this, (raw) => raw.removeglyph(cpOrGlyph), true);
    }
    return this;
  }

  newglyph(cp, bitmap, options = {}) {
    let opts;
    if (bitmap instanceof Bitmap || bitmap === null || bitmap === undefined) {
      opts = { ...options, bitmap: bitmap ?? options.bitmap ?? null };
    } else {
      opts = { ...bitmap, ...options };
    }
    const bm = opts.bitmap ? rawBitmapOf(opts.bitmap).clone() : undefined;
    this[S].short.delete(cp);
    const raw = call(
      this,
      (r) => r.newglyph(cp, bm, opts.mode ?? 0, opts.name ?? undefined, opts.dwidth ?? null),
      true,
    );
    return glyphOf(raw, this);
  }

  updatebbox() {
    call(this, (raw) => raw.updatebbox(), true);
    return this;
  }

  clone() {
    return fontOf(call(this, (raw) => raw.clone()), this);
  }

  subset(r) {
    return fontOf(
      call(this, (raw) => raw.subset(rangeArg(r ?? null))),
      this,
    );
  }

  merge(other, replace = false) {
    call(this, (raw) => raw.merge(other[S].raw, Boolean(replace)), true);
    return this;
  }

  transform(fn, r = null, mode = 0) {
    call(this, (raw) => raw.transform(bitmapFn(fn), rangeArg(r), mode), true);
    return this;
  }

  charsetconvert(newcharset) {
    const st = this[S];
    try {
      call(this, (raw) => raw.charsetconvert(String(newcharset)), true);
    } finally {
      st.charset = null;
      st.char2cp = null;
      st.cp2char = null;
    }
    return this;
  }

  // ----------------------------------------------------------------- output

  tobdf() {
    const out = call(this, (raw) => raw.tobdf());
    // bdfparser-js writes a non-integral numeric property value unquoted
    // (`fmtnum`); the shared writer quotes it, as the Python bdfparser does
    const floats = new Map();
    for (const [k, v] of Object.entries(propsData(this))) {
      if (k !== 'comment' && typeof v === 'number' && !Number.isInteger(v)) floats.set(propName(this, k), v);
    }
    if (floats.size === 0) return out;
    const lines = out.split('\n');
    const start = lines.findIndex((l) => l.startsWith('STARTPROPERTIES '));
    const end = lines.indexOf('ENDPROPERTIES', start);
    for (let i = start + 1; start >= 0 && i < end; i++) {
      const sp = lines[i].indexOf(' "');
      if (sp < 0 || !lines[i].endsWith('"')) continue;
      const name = lines[i].slice(0, sp);
      const v = floats.get(name);
      if (v !== undefined && Number(lines[i].slice(sp + 2, -1)) === v) {
        lines[i] = `${name} ${fmtnum(v)}`;
      }
    }
    return lines.join('\n');
  }

  topcf(options) {
    return writePcf(this, options);
  }

  tottf(options) {
    return writeTtf(this, options);
  }

  tosvgfont(options = {}) {
    const off = options.pathOffsets ?? {};
    const ua = options.unicodeAttr ?? {};
    return call(this, (raw) =>
      raw.tosvgfont(off.x ?? 0, off.y ?? 0, Boolean(ua.controlCharNoSkip ?? false), ua.format ?? 'hexncr'),
    );
  }
}

/* ------------------------------------------------------------------------ */
/* PCF                                                                      */
/* ------------------------------------------------------------------------ */

/** A compatibility `Font` for bdfparser-js's duck-typed writer input. */
const coerce = (font) => {
  if (font instanceof Font) return font;
  if (font === null || typeof font !== 'object') {
    throw new TypeError('expected a Font');
  }
  const out = new Font();
  out.headers = { ...(font.headers ?? {}) };
  const names = font._propnames instanceof Map ? font._propnames : new Map();
  call(
    out,
    (raw) => {
      for (const [k, v] of Object.entries(font.props ?? {})) {
        raw.setprop(k === 'comment' ? k : (names.get(k) ?? k.toUpperCase()), v);
      }
    },
    true,
  );
  for (const [cp, m] of font.glyphs ?? []) out.glyphs.set(cp, m);
  syncUnencoded(out, [...(font.unencoded ?? [])]);
  return out;
};

const PCF_PADS = [1, 2, 4, 8];
const PCF_SCANS = [1, 2, 4];

/**
 * Write a font as PCF: `byteorder` / `bitorder` (`'msb'` | `'lsb'`, default
 * `'msb'`), `glyphpad` (1, 2, 4, 8; default 4), `scanunit` (1, 2, 4; default
 * 1). With default options the output is byte-identical to `bdftopcf`.
 */
export function writePcf(font, options = {}) {
  const byteorder = options.byteorder ?? 'msb';
  const bitorder = options.bitorder ?? 'msb';
  const pad = options.glyphpad ?? 4;
  const scan = options.scanunit ?? 1;
  if (byteorder !== 'msb' && byteorder !== 'lsb') {
    throw new RangeError(`Invalid PCF byteorder: ${String(byteorder)}`);
  }
  if (bitorder !== 'msb' && bitorder !== 'lsb') {
    throw new RangeError(`Invalid PCF bitorder: ${String(bitorder)}`);
  }
  if (!PCF_PADS.includes(pad)) {
    throw new RangeError(`Invalid PCF glyphpad: ${String(pad)}`);
  }
  if (!PCF_SCANS.includes(scan)) {
    throw new RangeError(`Invalid PCF scanunit: ${String(scan)}`);
  }
  if (bitorder !== byteorder && scan > pad) {
    throw new RangeError('PCF scanunit must not exceed glyphpad when bitorder differs from byteorder');
  }
  const f = coerce(font);
  for (const cp of f[S].short) {
    if (cpset(f).has(cp)) {
      throw new Error('glyph meta must be a 17-slot list');
    }
  }
  const opts = { byte_order: byteorder, bit_order: bitorder, glyph_pad: String(pad), scan_unit: String(scan) };
  return call(f, (raw) => raw.topcf(opts));
}

/** A native handle loaded from PCF bytes (uncompressed). */
const pcfRaw = (data) => {
  const u8 = toBytes(data, 'PCF data must be a Uint8Array or an ArrayBuffer');
  if (isGzip(u8)) {
    throw new Error(
      'PCF data is gzip-compressed (.pcf.gz): use the async readPcfAsync() or decompress it first',
    );
  }
  if (u8.length < 4 || magicOf(u8) !== '\x01fcp') {
    throw new Error('Not a PCF file (bad magic, expected "\\x01fcp")');
  }
  const raw = new RawFont();
  guard(() => raw.loadBytes(knownPcfTables(u8)));
  return raw;
};

/** A native handle loaded from gzip-compressed bytes (decoded by the Rust loader). */
const gzipRaw = (u8, options = {}) => {
  const raw = new RawFont();
  guard(() => raw.loadBytes(u8, options.pixelsize ?? undefined));
  return raw;
};

/** Read a PCF font (uncompressed; use `readPcfAsync` for `.pcf.gz`). */
export function readPcf(data) {
  return fontOf(pcfRaw(data), undefined, true);
}

const PCF_TABLE_TYPES = [1, 2, 4, 8, 16, 32, 64, 128, 256];

/**
 * The PCF data with directory entries of unknown table types removed:
 * bdfparser-js reads only the nine PCF tables and ignores the others (the
 * native decoder would keep them as provenance and check their ranges).
 */
const knownPcfTables = (u8) => {
  if (u8.length < 8) return u8;
  const dv = new DataView(u8.buffer, u8.byteOffset, u8.byteLength);
  const n = dv.getUint32(4, true);
  if (n > 1024 || 8 + n * 16 > u8.length) return u8;
  const keep = [];
  for (let i = 0; i < n; i++) {
    if (PCF_TABLE_TYPES.includes(dv.getUint32(8 + i * 16, true))) keep.push(i);
  }
  if (keep.length === n) return u8;
  // table offsets are absolute and stay valid: the directory only shrinks
  const out = u8.slice();
  new DataView(out.buffer).setUint32(4, keep.length, true);
  keep.forEach((i, j) => out.set(u8.subarray(8 + i * 16, 8 + i * 16 + 16), 8 + j * 16));
  return out;
};

/** Read a PCF font, also gzip-compressed (`.pcf.gz`, decoded by the Rust loader). */
export async function readPcfAsync(data) {
  const u8 = toBytes(data, 'PCF data must be a Uint8Array or an ArrayBuffer');
  if (!isGzip(u8)) {
    return readPcf(u8);
  }
  let raw;
  try {
    raw = gzipRaw(u8);
  } catch (e) {
    if (/^Malformed gzip data/.test(e.message)) {
      throw new Error(`Cannot decompress gzip-compressed PCF: ${e.message}`, { cause: e });
    }
    throw e;
  }
  return fontOf(raw, undefined, true);
}

/* ------------------------------------------------------------------------ */
/* TrueType                                                                 */
/* ------------------------------------------------------------------------ */

/**
 * Convert a font to TrueType (SPEC-v3 §8): `shape` (`'square'` | `'circle'`),
 * `dotsize` (0, 1.5], `ppu` (even ≥ 2, default 64), `metadata` (embed the
 * `BDFP` table, default `true`), `timestamp` (Unix seconds, default 0).
 */
export const writeTtf = (font, options = {}) => {
  const shape = options.shape ?? 'square';
  const dotsize = options.dotsize ?? 1;
  const ppu = options.ppu ?? 64;
  const metadata = options.metadata ?? true;
  const timestamp = options.timestamp ?? 0;
  if (shape !== 'square' && shape !== 'circle') {
    throw new Error(`Invalid shape "${String(shape)}": must be "square" or "circle"`);
  }
  if (!(typeof dotsize === 'number' && dotsize > 0 && dotsize <= 1.5)) {
    throw new RangeError(`Invalid dotsize ${String(dotsize)}: must be in (0, 1.5]`);
  }
  if (!Number.isInteger(ppu) || ppu < 2 || ppu % 2 !== 0) {
    throw new RangeError(`Invalid ppu ${String(ppu)}: must be an even integer >= 2`);
  }
  if (!Number.isInteger(timestamp)) {
    throw new RangeError(`Invalid timestamp ${String(timestamp)}`);
  }
  const f = coerce(font);
  const opts = { shape, dotsize, ppu, metadata: Boolean(metadata), timestamp };
  return call(f, (raw) => glue.bdfparserWriteTtf(raw, opts));
};

/**
 * Read a TrueType (`glyf`) font: fonts written by `writeTtf` with `metadata`
 * are restored losslessly from their `BDFP` table, others are rasterized at
 * `pixelsize` pixels per em (required then).
 */
export const readTtf = (data, options = {}) => fontOf(ttfRaw(data, options));

/** A native handle loaded from TrueType bytes (see `readTtf`). */
const ttfRaw = (data, options = {}) => {
  let bytes;
  if (ArrayBuffer.isView(data)) {
    bytes = new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  } else if (Object.prototype.toString.call(data) === '[object ArrayBuffer]') {
    bytes = new Uint8Array(data);
  } else {
    throw new TypeError('TrueType data must be a Uint8Array or an ArrayBuffer');
  }
  if (bytes.length >= 4) {
    const tag = magicOf(bytes);
    if (tag === 'OTTO') {
      throw new Error('CFF-based OpenType is not supported');
    }
    if (bytes.length >= 12 && tag !== '\x00\x01\x00\x00' && tag !== 'true') {
      throw new Error(`Not a TrueType font (sfnt version ${JSON.stringify(tag)})`);
    }
  }
  const pixelsize = options.pixelsize ?? undefined;
  if (pixelsize !== undefined && !(isInt(pixelsize) && pixelsize > 0)) {
    // bdfparser-js checks `pixelsize` only for fonts without a BDFP table
    try {
      return glue.bdfparserReadTtf(bytes, undefined);
    } catch (e) {
      if (!/pixelsize/.test(e?.message ?? '')) throw translate(e);
      throw new RangeError(`pixelsize must be a positive integer, not ${String(pixelsize)}`);
    }
  }
  return guard(() => glue.bdfparserReadTtf(bytes, pixelsize));
};

/* ------------------------------------------------------------------------ */
/* Outlines                                                                 */
/* ------------------------------------------------------------------------ */

/**
 * TrueType-style contours of bitmap rows (top row first; any char other than
 * `'0'` is ink): `shape` `'square'` (merged outline when `dotsize` is 1) or
 * `'circle'`, `ppu` font units per pixel, `(xoff, yoff)` the bottom-left pixel.
 */
export const bitmapContours = (bindata, options = {}) => {
  const shape = options.shape ?? 'square';
  const dotsize = options.dotsize ?? 1;
  const ppu = options.ppu ?? 1;
  const xoff = options.xoff ?? 0;
  const yoff = options.yoff ?? 0;
  if (shape !== 'square' && shape !== 'circle') {
    throw new Error(`Invalid shape "${String(shape)}": must be "square" or "circle"`);
  }
  if (!(typeof dotsize === 'number' && dotsize > 0 && dotsize <= 1.5)) {
    throw new RangeError(`Invalid dotsize ${String(dotsize)}: must be in (0, 1.5]`);
  }
  if (!Number.isInteger(ppu) || ppu < 1) {
    throw new RangeError(`Invalid ppu ${String(ppu)}: must be a positive integer`);
  }
  return guard(() => glue.bdfparserBitmapContours([...bindata], shape, dotsize, ppu, xoff, yoff));
};

/**
 * Rasterize contours (non-zero winding sampled at pixel centres, 16 chords per
 * quadratic curve) into `hPx` rows of `wPx` pixels whose bottom-left pixel is
 * at `(xminPx, yminPx)`; returns the rows (top first).
 */
export const rasterize = (contours, ppu, xminPx, yminPx, wPx, hPx) => {
  if (wPx <= 0 || hPx <= 0) {
    return hPx > 0 ? new Array(hPx).fill('') : [];
  }
  const cs = [...contours].map((c) => [...c].map(([x, y, on]) => [x, y, Boolean(on)]));
  const b = guard(() => glue.bdfparserRasterize(cs, ppu, xminPx, yminPx, wPx, hPx));
  try {
    return b.bindata();
  } finally {
    b.free();
  }
};

/* ------------------------------------------------------------------------ */
/* Shortcuts                                                                */
/* ------------------------------------------------------------------------ */

const isURL = (s) =>
  (typeof URL !== 'undefined' && (s instanceof URL || Object.prototype.toString.call(s) === '[object URL]')) ||
  (typeof s === 'object' &&
    s !== null &&
    typeof s.href === 'string' &&
    !(Symbol.iterator in s) &&
    !(Symbol.asyncIterator in s));

/**
 * Create and load a font in one call: `source` is BDF text, bytes (BDF, PCF,
 * gzip, TrueType), a URL (fetched) or an (async) iterable of lines.
 */
export const $Font = async (source, encodingOptions = {}, loadOptions = {}) => {
  const font = new Font().encoding(encodingOptions);
  if (typeof source === 'string') {
    font.load_string(source);
  } else if (isBinary(source)) {
    await font.load_bytes_async(source, loadOptions);
  } else if (isURL(source)) {
    await font.load_url(source, undefined, loadOptions);
  } else {
    await font.load_filelines(source);
  }
  return font;
};

/**
 * fontpixel extension: load BDF, PCF or TrueType bytes synchronously, also
 * gzip-compressed (decoded by the bounded Rust loader; bdfparser-js's
 * `load_bytes` refuses gzip, its `load_bytes_async` needs DecompressionStream).
 */
export const loadFontBytes = (data, options = {}) => {
  const b = toBytes(data);
  if (!isGzip(b)) {
    return new Font().load_bytes(b, options);
  }
  return fontOf(gzipRaw(b, options), undefined, true);
};

/** `new Glyph(meta_obj, font)` */
export const $Glyph = (meta_obj, font) => new Glyph(meta_obj, font);

/** `new Bitmap(bin_bitmap_list)` */
export const $Bitmap = (bin_bitmap_list) => new Bitmap(bin_bitmap_list);

/* ------------------------------------------------------------------------ */
/* Raw handles (advanced)                                                   */
/* ------------------------------------------------------------------------ */

/**
 * The native handle behind a compatibility `Font`, `Glyph` or `Bitmap`
 * (`BdfparserRawFont` / `BdfparserRawGlyph` / `BdfparserRawBitmap`), e.g. to
 * reach `RawFont.core()` (a native `fontpixel` `Font` copy of the model).
 */
export const rawHandle = (obj) => {
  if (obj instanceof Font) return obj[S].raw;
  if (obj instanceof Glyph) return obj[S].raw;
  if (obj instanceof Bitmap) return obj[S];
  throw new TypeError('expected a compatibility Font, Glyph or Bitmap');
};

/** A compatibility `Font` over a copy of a native `fontpixel` `Font` (`Font::from_core`). */
export const fromCore = (font) => fontOf(guard(() => RawFont.fromCore(font)));
