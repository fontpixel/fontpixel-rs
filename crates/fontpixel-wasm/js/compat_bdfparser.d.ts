// Type declarations of `fontpixel/compat/bdfparser`: the bdfparser-js 3.0
// compatible API (SPEC-v3) on the fontpixel Rust core. Names, argument order,
// defaults and option objects are bdfparser-js's; see compat_bdfparser.js.
import type {
  BdfparserRawBitmap,
  BdfparserRawFont,
  BdfparserRawGlyph,
  Font as NativeFont,
} from "./fontpixel_wasm.js";

/** bdfparser API version this module implements. */
export declare const VERSION: "3.0.0";
/** VVECTOR delimiter: whitespace and/or comma */
export declare const PATTERN_VVECTOR_DELIMITER: RegExp;
/** The 17 glyph meta keys, in slot order. */
export declare const META_TITLES: readonly [
  "glyphname", "codepoint", "bbw", "bbh", "bbxoff", "bbyoff",
  "swx0", "swy0", "dwx0", "dwy0", "swx1", "swy1", "dwx1", "dwy1",
  "vvectorx", "vvectory", "hexdata",
];
/** The empty glyph drawn for a missing codepoint. */
export declare const EMPTY_GLYPH: Readonly<GlyphMeta>;
export declare const DIRE_SHORTCUT_MAP: {
  readonly lr: "lrtb"; readonly rl: "rltb"; readonly tb: "tbrl"; readonly bt: "btrl";
  readonly lrtb: undefined; readonly lrbt: undefined; readonly rltb: undefined; readonly rlbt: undefined;
  readonly tbrl: undefined; readonly tblr: undefined; readonly btrl: undefined; readonly btlr: undefined;
};
export declare const DIRE_MAP: { readonly lr: 1; readonly rl: 2; readonly tb: 0; readonly bt: -1 };

/* ------------------------------------------------------------------ types */

export type BdfHeaders = {
  bdfversion: number;
  fontname: string;
  pointsize: number;
  xres: number;
  yres: number;
  fbbx: number;
  fbby: number;
  fbbxoff: number;
  fbbyoff: number;
  swx0?: number;
  swy0?: number;
  dwx0?: number;
  dwy0?: number;
  swx1?: number;
  swy1?: number;
  dwx1?: number;
  dwy1?: number;
  vvectorx?: number;
  vvectory?: number;
  metricsset?: number;
  contentversion?: number;
  comment?: string[];
};
export type BdfHeadersTemp = Partial<BdfHeaders>;
/** A property value: `number` (unquoted integer), `string` or `null` (key-only line); integers beyond 2^53 - 1 are `bigint`. */
export type PropValue = string | number | bigint | null;
export type Props = Record<string, PropValue> & { comment?: string[] };
export type GlyphMetaInFont = [
  string, number, number, number, number, number,
  number | null, number | null, number | null, number | null,
  number | null, number | null, number | null, number | null,
  number | null, number | null, string[],
];
export type GlyphMetaInFontTemp = { [K in keyof GlyphMetaInFont]: GlyphMetaInFont[K] | null };
export type GlyphMeta = {
  glyphname: string;
  codepoint: number;
  bbw: number;
  bbh: number;
  bbxoff: number;
  bbyoff: number;
  swx0: number | null;
  swy0: number | null;
  dwx0: number | null;
  dwy0: number | null;
  swx1: number | null;
  swy1: number | null;
  dwx1: number | null;
  dwy1: number | null;
  vvectorx: number | null;
  vvectory: number | null;
  hexdata: string[];
};
export type GlyphMetaTemp = Partial<GlyphMeta>;
/** An unencoded glyph (`ENCODING -1 [altcode]`); assigning `altcode` or a `meta` slot edits the font. */
export type UnencodedGlyph = { altcode: number | null; meta: GlyphMetaInFont };
export type TodataFuncRetType<T> = T extends undefined | null ? string[] : T extends 0 ? string : T extends 1 ? string[] : T extends 2 ? number[][] : T extends 3 ? number[] : T extends 4 ? string[] : T extends 5 ? number[] : never;
/** Codepoint range filter: `n` (cp < n), `[a, b]` (a <= cp <= b) or a list of `[a, b]` */
export type CodepointRangeType = number | [number, number] | [number, number][];
export type OrderType = -1 | 0 | 1 | 2;
export type GlyphDrawModeType = -1 | 0 | 1 | 2;
export type DirectionType = keyof typeof DIRE_SHORTCUT_MAP;
export type DirectionPartType = keyof typeof DIRE_MAP;
export type DirectionNumberType = (typeof DIRE_MAP)[keyof typeof DIRE_MAP];

/* --------------------------------------------------------------- warnings */

/** A function receiving the warning message (without the `[bdfparser]` prefix) */
export type WarningHandler = (message: string) => void;
/** The default handler: `console.warn('[bdfparser] ' + message)` */
export declare const defaultWarningHandler: WarningHandler;
/** Emit a warning through the current handler. */
export declare function warn(message: string): void;
/** Replace the handler (`null` silences, `undefined` restores the default); returns the previous one. */
export declare function setWarningHandler(handler?: WarningHandler | null): WarningHandler | null;

/**
 * Fatal BDF parse error; `message` is `"line N: msg"` when the line is known.
 * Errors from the Rust core keep its fields (`kind`, `line`, `cause`, ...).
 */
export declare class BDFParseError extends Error {
  constructor(message: string, lineno?: number | null);
  readonly lineno: number | null;
  readonly msg: string;
}

/* ----------------------------------------------------------------- Bitmap */

export type CanvasContext = { fillStyle: any; fillRect: any };
export type PixelColors = Partial<Record<"0" | "1" | "2", string | null>>;
export type Draw2CanvasOptions = { x?: number; y?: number; pixelsize?: number; pixelcolors?: PixelColors | null };
export type SvgFormat = "path" | "pathOnly" | "rect";
export type SvgShape = "square" | "circle";

/**
 * Rows of `'0'`, `'1'`, `'2'` (row 0 at the top) backed by a native raster;
 * `bindata` builds the row strings on each read (assign to replace).
 * Rows must be rectangular and use only the labels 0, 1 and 2.
 */
export declare class Bitmap {
  constructor(bin_bitmap_list: string[]);
  bindata: string[];
  static fromstring(text: string): Bitmap;
  static blank(w: number, h: number): Bitmap;
  static concatall(
    bitmaplist: Bitmap[],
    options?: { direction?: DirectionNumberType | null; align?: 0 | 1 | null; offsetlist?: number[] | null },
  ): Bitmap;
  toString(): string;
  repr(): string;
  equals(other: Bitmap): boolean;
  width(): number;
  height(): number;
  clone(): Bitmap;
  crop(w: number, h: number, xoff?: number | null, yoff?: number | null): this;
  overlay(bitmap: Bitmap): this;
  concat(bitmap: Bitmap, options?: { direction?: DirectionNumberType | null; align?: 0 | 1 | null; offset?: number | null }): this;
  enlarge(x?: number, y?: number): this;
  replace(substr: string | number, newsubstr: string | number): this;
  shadow(xoff?: number | null, yoff?: number | null): this;
  glow(mode?: 0 | 1 | null): this;
  bytepad(bits?: number | null): this;
  todata<T extends 0 | 1 | 2 | 3 | 4 | 5 | null>(datatype?: T): TodataFuncRetType<T>;
  getpixel(x: number, y: number): number;
  setpixel(x: number, y: number, value?: 0 | 1 | 2): this;
  fliph(): this;
  flipv(): this;
  rotate(angle?: number): this;
  invert(): this;
  shift(dx?: number, dy?: number): this;
  bbox(): [number, number, number, number];
  trim(): this;
  bold(n?: number): this;
  italic(step?: number, base?: number): this;
  paste(bitmap: Bitmap, x?: number, y?: number): this;
  draw2canvas(context: CanvasContext, options?: Draw2CanvasOptions | PixelColors | null, xoff?: number, yoff?: number): this;
  tosvg(pixelsize?: number, format?: SvgFormat, inverted?: boolean, offsets?: { x?: number; y?: number }, shape?: SvgShape): string;
}

/** Python-compatible number text (integral without a fraction, else Python `repr`). */
export declare function fmtnum(n: number | bigint): string;

/* ------------------------------------------------------------------ Glyph */

export type SetbitmapOptions = { mode?: GlyphDrawModeType; bb?: [number, number, number, number] | null; trim?: boolean };

/** `uniXXXX` / `uXXXXX` (Unicode) or `char{cp}` (other charsets). */
export declare function defaultglyphname(cp: number, charsetIsUnicode?: boolean): string;

/**
 * A glyph: `meta` (the 17 keys) and `font`. Attached glyphs (from a font, or
 * passed to `addglyph`) write their edits through to the font.
 */
export declare class Glyph {
  constructor(meta_obj: GlyphMeta | GlyphMetaTemp, font: Font);
  meta: GlyphMeta;
  font: Font;
  toString(): string;
  repr(): string;
  cp(): number;
  chr(): string | null;
  draw(mode?: GlyphDrawModeType | null, bb?: [number, number, number, number] | null): Bitmap;
  origin(options?: { mode?: GlyphDrawModeType | null; fromorigin?: boolean | null; xoff?: number | null; yoff?: number | null }): [number, number];
  setbitmap(bitmap: Bitmap | string[], mode?: GlyphDrawModeType | SetbitmapOptions | null, bb?: [number, number, number, number] | null, trim?: boolean | null): this;
  setmeta(obj: Partial<GlyphMeta>): this;
  shift(dx?: number, dy?: number): this;
  transform(fn: (bitmap: Bitmap) => Bitmap | null | undefined | void, mode?: GlyphDrawModeType): this;
  clone(): Glyph;
  /** @deprecated Use `setbitmap(bitmap, 1, null, false)`. */
  changebitmap(bitmap: Bitmap): this;
  /** Whether the glyph still writes through to its font. */
  readonly _attached: boolean;
  /** The font's 17-slot list of this glyph, or `null` when detached. */
  readonly _store: GlyphMetaInFont | null;
}

/* --------------------------------------------------------------- charsets */

export type CharsetKind = "sbcs" | "dbcs" | "gb18030";
/** Minimal `iconv-lite`-like interface; accepted for compatibility (the exact tables are built in). */
export interface IconvLike {
  encode(str: string, encoding: string): Uint8Array;
  decode(buf: Uint8Array, encoding: string): string;
  encodingExists?(encoding: string): boolean;
}
/** Character <-> codepoint converter of a charset. */
export interface CharsetCodec {
  readonly charset: string;
  readonly unicode: boolean;
  readonly gl94: boolean;
  readonly gl: boolean;
  readonly known: boolean;
  readonly kind: "unicode" | CharsetKind;
  /** Always `true`: fontpixel's exact tables are compiled into the module. */
  readonly exact: boolean;
  cp2char(cp: number): string | null;
  char2cp(char: string): number | null;
}
export type TableSpec = { [key: string]: unknown };
export type CharsetTables = { tables?: Record<string, unknown>; gb18030?: unknown };

export declare function sanitizeCharsetName(charset: string): string;
export declare function sanitizeCharset(registry?: string | number | null, encoding?: string | number | null): string;
export declare function canonicalCharset(name: string): string;
export declare function charsetProps(key: string): [string, string];
export declare function isUnicodeCharset(key: string): boolean;
export declare function charsetKeys(): string[];
export declare function getCodec(charset: string, options?: { iconv?: IconvLike; gl?: boolean | null }): CharsetCodec;
export declare function detectGL(charsetEncoding: string | number | null | undefined, cps: Iterable<number>): boolean;
export declare function char2cp(char: string, charset?: string, iconv?: IconvLike): number | null;
export declare function cp2char(cp: number, charset?: string, iconv?: IconvLike): string | null;
/** Accepted for compatibility; the exact tables are built in (validates its argument only). */
export declare function registerCharsetTables(tables: CharsetTables): void;

/* ------------------------------------------------------------------- Font */

export type EncodingOptions = {
  iconv?: IconvLike;
  charset?: string | null;
  char2cp?: (char: string) => number | null | undefined;
  cp2char?: (cp: number) => string | null | undefined;
};
export type FontCreateOptions = {
  pixelsize?: number;
  ascent?: number;
  descent?: number;
  fontname?: string;
  familyname?: string;
  xres?: number;
  yres?: number;
};
export type URLLike = { readonly href: string };
export type FetchInit = { [key: string]: unknown };
export type LoadOptions = { pixelsize?: number };
export type NewglyphOptions = { bitmap?: Bitmap | null; mode?: GlyphDrawModeType; name?: string | null; dwidth?: number | null };
export type SvgFontOptions = {
  pathOffsets?: { x?: number; y?: number };
  unicodeAttr?: { controlCharNoSkip?: boolean; format?: "hexncr" | "decncr" | "char" };
};
export type DrawOptions = {
  linelimit?: number | null;
  mode?: 0 | 1 | null;
  direction?: DirectionType | null;
  usecurrentglyphspacing?: boolean | null;
  missing?: Glyph | GlyphMeta | null;
};

/**
 * A font. `headers`, `props`, `glyphs`, `unencoded` and `_propnames` are live
 * views of the native model: reading projects it, writing (assignment, Map
 * and array methods, item assignment into a meta list) edits it in place.
 */
export declare class Font {
  constructor();
  headers: BdfHeaders;
  props: Props;
  glyphs: Map<number, GlyphMetaInFont>;
  unencoded: UnencodedGlyph[];
  _propnames: Map<string, string>;
  load_filelines(filelines: AsyncIterable<string> | Iterable<string>): Promise<this>;
  load_string(text: string): this;
  load_bytes(data: Uint8Array | ArrayBuffer | ArrayBufferView, options?: LoadOptions): this;
  load_url(url: string | URLLike, init?: FetchInit, options?: LoadOptions): Promise<this>;
  load_bytes_async(data: Uint8Array | ArrayBuffer | ArrayBufferView, options?: LoadOptions): Promise<this>;
  static create(options?: FontCreateOptions): Font;
  encoding(options?: EncodingOptions): this;
  charset: string;
  char2cp(char: string): number | null;
  cp2char(cp: number): string | null;
  getprop(key: string): PropValue | string[];
  setprop(key: string, value: string | number | bigint | null): this;
  delprop(key: string): this;
  swidth(dwx: number): number;
  readonly length: number;
  itercps(order?: OrderType | null, r?: CodepointRangeType | null): number[];
  iterglyphs(order?: OrderType | null, r?: CodepointRangeType | null): IterableIterator<Glyph>;
  glyphbycp(codepoint: number): Glyph | null;
  glyph(character: string): Glyph | null;
  lacksglyphs(str: string): null | string[];
  drawcps(cps: number[], options?: DrawOptions): Bitmap;
  draw(str: string, options?: DrawOptions): Bitmap;
  drawall(options?: {
    order?: OrderType;
    r?: CodepointRangeType;
    linelimit?: number | null;
    mode?: 0 | 1 | null;
    direction?: DirectionType | null;
    usecurrentglyphspacing?: boolean | null;
  }): Bitmap;
  addglyph(glyph: Glyph | Partial<GlyphMeta>, replace?: boolean | { replace?: boolean }): this;
  removeglyph(cpOrGlyph: number | Glyph): this;
  newglyph(cp: number, bitmap?: Bitmap | NewglyphOptions | null, options?: NewglyphOptions): Glyph;
  updatebbox(): this;
  clone(): Font;
  subset(r: CodepointRangeType | null): Font;
  merge(other: Font, replace?: boolean): this;
  transform(fn: (bitmap: Bitmap) => Bitmap | null | undefined | void, r?: CodepointRangeType | null, mode?: GlyphDrawModeType): this;
  charsetconvert(newcharset: string): this;
  tobdf(): string;
  topcf(options?: PcfWriteOptions): Uint8Array;
  tottf(options?: WriteTtfOptions): Uint8Array;
  tosvgfont(options?: SvgFontOptions): string;
}

/* -------------------------------------------------------- PCF, TrueType */

export type PcfWriteOptions = { byteorder?: "msb" | "lsb"; bitorder?: "msb" | "lsb"; glyphpad?: 1 | 2 | 4 | 8; scanunit?: 1 | 2 | 4 };
export declare function writePcf(font: Font, options?: PcfWriteOptions): Uint8Array;
export declare function readPcf(data: Uint8Array | ArrayBuffer): Font;
export declare function readPcfAsync(data: Uint8Array | ArrayBuffer): Promise<Font>;

export type WriteTtfOptions = { shape?: DotShape; dotsize?: number; ppu?: number; metadata?: boolean; timestamp?: number };
export type ReadTtfOptions = { pixelsize?: number };
export declare function writeTtf(font: Font, options?: WriteTtfOptions): Uint8Array;
export declare function readTtf(data: Uint8Array | ArrayBuffer, options?: ReadTtfOptions): Font;

/* --------------------------------------------------------------- outlines */

export type ContourPoint = [x: number, y: number, oncurve: boolean];
export type Contour = ContourPoint[];
export type DotShape = "square" | "circle";
export type BitmapContoursOptions = { shape?: DotShape; dotsize?: number; ppu?: number; xoff?: number; yoff?: number };
export declare function bitmapContours(bindata: readonly string[], options?: BitmapContoursOptions): Contour[];
export declare function rasterize(contours: readonly (readonly ContourPoint[])[], ppu: number, xminPx: number, yminPx: number, wPx: number, hPx: number): string[];

/* -------------------------------------------------------------- shortcuts */

export type FontSource = AsyncIterable<string> | Iterable<string> | string | Uint8Array | ArrayBuffer | ArrayBufferView | URLLike;
export declare function $Font(source: FontSource, encodingOptions?: EncodingOptions, loadOptions?: LoadOptions): Promise<Font>;
export declare function $Glyph(meta_obj: GlyphMeta, font: Font): Glyph;
export declare function $Bitmap(bin_bitmap_list: string[]): Bitmap;

/* ----------------------------------------------------- fontpixel extensions */

/** Load BDF, PCF or TrueType bytes synchronously, also gzip-compressed (decoded in Rust). */
export declare function loadFontBytes(data: Uint8Array | ArrayBuffer | ArrayBufferView, options?: LoadOptions): Font;
/** The native handle behind a compatibility object. */
export declare function rawHandle(obj: Font): BdfparserRawFont;
export declare function rawHandle(obj: Glyph): BdfparserRawGlyph;
export declare function rawHandle(obj: Bitmap): BdfparserRawBitmap;
/** A compatibility `Font` over a copy of a native `Font`. */
export declare function fromCore(font: NativeFont): Font;
