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
  /** The format's typed read options (applied to the detected format when `format` is absent). */
  options?: object;
  /** Filename hint (never content evidence). */
  name?: string;
  /** Sidecars. */
  resources?: ResourcesLike;
  /** Input resource limits. */
  limits?: ResourceLimits;
  /** Directory entry point (no primary bytes; files come from `resources`; needs `format`). */
  directory?: string;
}

/** Options of `Registry.read`. */
export interface RegistryReadArgs {
  /** Format ID; bypasses detection. */
  format?: string;
  /** Filename hint (never content evidence). */
  name?: string;
  /** Sidecars. */
  resources?: ResourcesLike;
  /** Input resource limits. */
  limits?: ResourceLimits;
  /** Directory entry point (no primary bytes; files come from `resources`; needs `format`). */
  directory?: string;
}

/** Options of `detect` / `Registry.detect`. */
export interface DetectArgs {
  /** Filename hint, used only when no content detector matches. */
  name?: string;
  /** Sidecars. */
  resources?: ResourcesLike;
  /** Input resource limits. */
  limits?: ResourceLimits;
  /** Directory entry point (`Registry.detect` only; always an error: directory inputs need a format). */
  directory?: string;
}

/** Common options of every registry write route. */
export interface WriteCommonArgs {
  /** Primary output name (default `font.<first extension>`; a `.gz` suffix selects the gzip envelope). */
  primaryName?: string;
  /** Throw `StrictLossError` on any read or write loss. */
  strict?: boolean;
  /** Common transport limits. */
  transportLimits?: TransportLimits;
}

/** Options of `Registry.write`, `writeReservedWithReport` and `writeOwned`. */
export interface RegistryWriteArgs extends WriteCommonArgs {
  /** Earlier diagnostics (for example a read report) combined before strict checks. */
  report?: Report;
}

/** Options of the module function `write`. */
export interface WriteArgs extends RegistryWriteArgs {
  /** The format's typed write options. */
  options?: object;
}

/** `Loaded.intoSingle()`: the sole strike as a `Font`. */
export interface LoadedFont {
  fonts: Font;
  report: Report;
  format: string;
}

/** One output file: a validated relative path and a fresh copy of its bytes. */
export interface OutputAssetLike {
  path: string;
  data: Uint8Array;
}

/** Result of a direct reader: the model exactly as the Rust function returns it, and its report. */
export interface ModelReport<M> {
  model: M;
  report: Report;
}

/** Result of a direct byte writer. */
export interface BytesReport {
  data: Uint8Array;
  report: Report;
}

/** Options of a direct context reader (`formats.<module>.read`). */
export interface DirectContextReadArgs<O> {
  /** The module's read options. */
  options?: O;
  /** Filename hint. */
  name?: string;
  /** Sidecars. */
  resources?: ResourcesLike;
  /** Input resource limits. */
  limits?: ResourceLimits;
  /** Directory entry point. */
  directory?: string;
}

/** Options of a direct bundle writer (`formats.<module>.write`). */
export interface DirectWriteArgs<O> extends WriteCommonArgs {
  /** The module's write options. */
  options?: O;
}

/** The input of a JS codec hook (a `HookContext`). */
export type CodecContext = HookContext;

/** A JS writer: a function, or an object with `write` and optional transport hooks. */
export type WriterHook =
  | ((fonts: FontSet, options: FormatsWriteOptions) => Written | OutputBundle | [OutputBundle, Report])
  | {
      write(fonts: FontSet, options: FormatsWriteOptions): Written | OutputBundle | [OutputBundle, Report];
      transportLimits?(): TransportLimits;
      validateTransport?(fonts: FontSet, options: FormatsWriteOptions, limits: TransportLimits): void;
    };

/** Hooks of `Registry.register`. */
export interface CodecHooks {
  /** Content detector: `"no_match"`, `"possible"` or `"certain"`. */
  detector?: (ctx: HookContext) => Confidence;
  /** Reader: `[fonts, report]`. */
  reader?: (ctx: HookContext, options: FormatsReadOptions) => [FontSet | Font, Report];
  /** Writer. */
  writer?: WriterHook;
}

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
