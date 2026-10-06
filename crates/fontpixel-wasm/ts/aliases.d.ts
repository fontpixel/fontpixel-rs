// Hand-written argument aliases of the fontpixel package (see
// docs/bindings-design.md, "JS/WASM"). Plain-data types are generated into
// types.d.ts by tools/bindings/gen_dts.py.

/** Bytes accepted by every byte parameter (copied once into WASM memory). */
export type BytesLike = Uint8Array | ArrayBuffer | DataView;

/** Bytes inside plain data: arrays of numbers on output; inputs also take buffers. */
export type SerdeBytes = number[];
/** Input form of {@link SerdeBytes}. */
export type SerdeBytesLike = number[] | Uint8Array | ArrayBuffer | DataView;

/** A 64-bit (or wider) Rust integer: `number` when |v| <= 2^53 - 1, else `bigint`. */
export type WideInt = number | bigint;

/** `"mono"` (1 bit per pixel) or `"gray8"` (one byte per pixel). */
export type Depth = "mono" | "gray8";

/** A codepoint argument: a number is Unicode; plain data names the domain. */
export type CodepointLike = number | CodepointDict;

/**
 * A progress observer: receives `Progress` plain data; returning exactly
 * `false` cancels (`CancelledError`), throwing cancels and rethrows.
 */
export type ProgressFunction = (progress: Progress) => unknown;

/**
 * A synchronous sidecar resolver: return the file's bytes, or `null`/`undefined`
 * when it does not exist. Called while the operation runs; it must not await.
 */
export type ResolveFunction = (path: string, maxBytes: number) => BytesLike | null | undefined;

/** A resolver object; `list` enables directory enumeration (`directory` is `null` for the root). */
export interface ResolverObject {
  resolve(path: string, maxBytes: number): BytesLike | null | undefined;
  list?(directory: string | null, maxEntries: number, maxBytes: number): string[];
}

/** Sidecar resources: named files, or a synchronous resolver. */
export type ResourcesLike =
  | Record<string, BytesLike>
  | Map<string, BytesLike>
  | ResolveFunction
  | ResolverObject;

/** Options of `read`. */
export interface ReadArgs {
  /** Format ID; bypasses detection. */
  format?: string;
  /** Filename hint (never content evidence). */
  name?: string;
  /** Sidecars. */
  resources?: ResourcesLike;
  /** Input resource limits. */
  limits?: ResourceLimits;
}

/** A palette for `Bitmap.fromText`: character -> pixel value (object, Map or `[char, value]` pairs). */
export type TextPalette = Record<string, number> | Map<string, number> | [string, number][];
/** A palette for `Bitmap.toText`: pixel value -> character (object, Map or `[value, char]` pairs). */
export type OutputPalette = Record<number, string> | Map<number, string> | [number, string][];
/** A palette for `Bitmap.toPaletteBytes`: pixel value -> bytes per pixel. */
export type BytesPalette =
  | Record<number, BytesLike | number[]>
  | Map<number, BytesLike | number[]>
  | [number, BytesLike | number[]][];

/** A TrueType outline point `[x, y, onCurve]` in upward design units (`fontpixel::outline::Point`). */
export type OutlinePoint = [number, number, boolean];
/** A closed contour without a repeated closing point (`fontpixel::outline::Contour`). */
export type Contour = OutlinePoint[];

/** A capability record of `capabilities()`. */
export interface Capability {
  name: string;
  available: boolean;
  description: string;
  /** Why it is unavailable (empty when available). */
  reason: string;
  /** Evidence for the reason (empty when available). */
  evidence: string;
  /** What to use instead (empty when available). */
  alternative: string;
}
