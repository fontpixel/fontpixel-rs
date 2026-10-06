import type {
  BytesLike,
  InfoArgs,
  InfoDocument,
  MergeArgs,
  MergePart,
  MergeFilesResult,
  Progress,
  RenderArgs,
  RenderResult,
} from "./fontpixel_wasm.js";

/** Per-call options of a worker call. */
export interface WorkerCallOptions {
  /** Aborting cancels the call (`CancelledError`); see `FontpixelWorker.cancellation`. */
  signal?: AbortSignal;
}

/** Per-call options of a worker merge. */
export interface WorkerMergeOptions extends WorkerCallOptions {
  /** Receives every `Progress` record of the merge (read, merge, encode steps). */
  onProgress?: (progress: Progress) => void;
}

/** Options of {@link createWorker}. */
export interface CreateWorkerOptions {
  /** Compiled module to instantiate (default: the package's `.wasm`, compiled once and cached). */
  module?: WebAssembly.Module;
  /** Override the worker script URL (default: `ops-worker.js` next to this module). */
  workerUrl?: URL | string;
  /**
   * `"auto"` (default): cooperative where `SharedArrayBuffer` is usable (Node,
   * cross-origin isolated pages), else terminate. `"cooperative"` throws
   * `UnsupportedError` without shared memory.
   */
  cancellation?: "auto" | "cooperative" | "terminate";
}

/** A worker running the W8.3 calls off the main thread. */
export declare class FontpixelWorker {
  private constructor();
  /** How an abort is carried out. */
  readonly cancellation: "cooperative" | "terminate";
  /** Calls sent and not yet settled. */
  readonly pending: number;
  /** `merge(parts, options)` in the worker (`options.progress` is not allowed: use `onProgress`). */
  merge(parts: MergePart[], options: Omit<MergeArgs, "progress">, call?: WorkerMergeOptions): Promise<MergeFilesResult>;
  /** `info(bytes, options)` in the worker. */
  info(bytes: BytesLike, options?: InfoArgs, call?: WorkerCallOptions): Promise<InfoDocument>;
  /** `render(fontBytes, text, options)` in the worker. */
  render(
    fontBytes: BytesLike,
    text: string | null | undefined,
    options?: RenderArgs,
    call?: WorkerCallOptions,
  ): Promise<RenderResult>;
  /** Hard stop: terminate the worker; pending calls reject with `CancelledError`. */
  terminate(): void;
}

/** Compile the WebAssembly module once (cached when `source` is omitted). */
export declare function compileModule(
  source?: WebAssembly.Module | BytesLike | URL | string | Response,
): Promise<WebAssembly.Module>;

/** Start a worker (browser module worker or Node `worker_threads`) and wait until it is ready. */
export declare function createWorker(options?: CreateWorkerOptions): Promise<FontpixelWorker>;

/** Run one merge in a worker (a new one unless `worker` is given, terminated afterwards). */
export declare function mergeInWorker(
  parts: MergePart[],
  options: Omit<MergeArgs, "progress">,
  call?: WorkerMergeOptions & { worker?: FontpixelWorker; module?: WebAssembly.Module },
): Promise<MergeFilesResult>;
