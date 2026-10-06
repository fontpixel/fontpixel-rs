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

// ---------------------------------------------------------------- ops (W8.3)

/** A memory-storage provider for `ops` calls, called synchronously while an operation runs. */
export interface FileProviderObject {
  /** `null`/`undefined` (absent), a file length in bytes, or `"directory"`. */
  entry(path: string): number | bigint | "directory" | null | undefined;
  /** The complete contents of a file `entry` reported. */
  read(path: string): BytesLike;
  /** Names (not paths) directly inside `directory` (`""` is the root), in a deterministic order. */
  list(directory: string): string[];
}

/** Memory storage of an `ops` call: named buffers (storage key -> bytes) or a provider. */
export type FilesLike = Record<string, BytesLike> | Map<string, BytesLike> | FileProviderObject;

/** Trailing options of an `ops` call. `files` is required: WebAssembly has no filesystem storage. */
export interface OpsCallOptions {
  files?: FilesLike;
}

/** Trailing options of `ops.convert`/`ops.vectorize`/`ops.merge`. */
export interface OpsProgressOptions extends OpsCallOptions {
  /** Called after each step; returning exactly `false` cancels (`CancelledError`). */
  progress?: ProgressFunction;
}

/** Options of `ops.batchToml`. */
export interface BatchTomlOptions extends OpsCallOptions {
  /** Directory that relative task paths resolve against (default none). */
  base?: string;
  /** Overrides the file's worker ceiling. */
  jobs?: number;
}

/**
 * A typed batch task: a string `id`, the task `kind` and the flat request
 * keys of that operation (`ConvertRequest`, `MergeRequest`, ...).
 */
export type BatchTaskSpec = {
  id: string;
  kind: "convert" | "vectorize" | "variants" | "subset" | "merge" | "charset" | "validate" | "diff";
  [key: string]: unknown;
};

/** The request of `ops.batch`. */
export interface BatchSpec {
  tasks: BatchTaskSpec[];
  jobs?: number | null;
  max_working_bytes?: number;
  max_pending_bytes?: number;
  max_report_bytes?: number;
  max_task_report_bytes?: number;
  protected_paths?: string[];
}

/** One task outcome of a `BatchResult`. */
export interface BatchTaskSummary {
  index: number;
  id: string;
  kind: string;
  success: boolean;
  exit_code: number;
  committed: boolean;
  diagnostics_truncated: boolean;
  error: string | null;
}

/** `ops.parseBatchToml` result. */
export interface ParsedBatch {
  jobs: number | null;
  max_working_bytes: number;
  max_pending_bytes: number;
  max_report_bytes: number;
  max_task_report_bytes: number;
  tasks: {
    id: string;
    kind: string;
    declared_inputs: string[];
    /** The normalized request: `kind` plus the operation's flat keys. */
    request?: { kind: BatchTaskSpec["kind"]; [key: string]: unknown };
    weight?: number;
    error?: string;
  }[];
}

/** `ops.capabilities()`: the operations layer's booleans. */
export interface OpsCapabilities {
  filesystem: boolean;
  threads: boolean;
  toml_batch: boolean;
}

/** `ops.defaults()`: the batch budget defaults. */
export interface BatchDefaults {
  max_working_bytes: number;
  max_pending_bytes: number;
  max_report_bytes: number;
  max_task_report_bytes: number;
}

/** `ops.parseArgs` / `ops.inspectRequest` result. */
export interface RequestDescription {
  command: string;
  request: any;
  report_style: "text" | "json";
  source_identity_total_bytes: number;
  declared_inputs?: string[];
  weight?: number;
}

/** The `info --json` document returned by `info(bytes)`. */
export interface InfoDocument {
  format: string;
  font_count: number;
  fonts: any[];
  report: Report;
}

/** Options of `info(bytes, options)`: `name` plus `InfoRequest` keys other than `input`/`json`. */
export type InfoArgs = { name?: string } & Omit<Partial<InfoRequest>, "input" | "json">;

/** One part of `merge(parts, options)`, in priority order. */
export interface MergePart {
  /** The font file. */
  bytes: BytesLike;
  /** Selection expression, e.g. `"cjk"`, `"western,geometric"`, `"U+2190-21FF,-U+21A0"`. */
  take: string;
  /** Vertical offset in pixels (positive moves glyphs up). */
  dy?: number;
  /** File name: storage key, format hint, diagnostic asset name (default `part-<n>`). */
  name?: string;
  /** Explicit input format. */
  from?: string | null;
  /** Strike of a multi-font input. */
  font_index?: number | null;
}

/** Options of `merge(parts, options)`: the command line's merge options. */
export type MergeArgs = Omit<MergeRequest, "parts" | "outputs"> & {
  /** Output file names (format from the extension) or `{destination, format}`. */
  outputs: (string | MergeOutput)[];
  /** Progress observer; returning exactly `false` cancels. */
  progress?: ProgressFunction;
};

/** Result of `merge(parts, options)`. */
export interface MergeFilesResult {
  /** Every output asset by name. */
  files: Record<string, Uint8Array>;
  /** The parsed `--report json` document. */
  report: any;
  /** The `--report json` document exactly as the command line prints it. */
  json: string;
  /** The text report exactly as the command line prints it. */
  text: string;
}

/** Options of `render(fontBytes, text, options)`. */
export type RenderArgs = { name?: string } & Omit<Partial<PreviewRequest>, "input" | "text" | "chars"> &
  (
    | { format?: "mono" }
    | {
        format: "rgba";
        /** Integer nearest-neighbor scale (default 1). */
        scale?: number;
        /** Ink color (default opaque black). */
        color?: Rgba;
        /** Paper color (default opaque white). */
        background?: Rgba;
      }
  );

/** Result of `render(fontBytes, text, options)`. */
export interface RenderResult {
  format: "mono" | "rgba";
  /** Image width in pixels (scaled for `rgba`). */
  width: number;
  /** Image height in pixels (scaled for `rgba`). */
  height: number;
  /** Bytes per row. */
  stride: number;
  /** Pixel scale (1 for `mono`). */
  scale: number;
  /** Packed 1-bit rows (`mono`) or RGBA pixels (`rgba`). */
  data: Uint8Array | Uint8ClampedArray;
  /** `#`/`.` character art (`fontpixel preview` output without the final newline). */
  art: string;
  /** Read and render diagnostics. */
  report: Report;
}
