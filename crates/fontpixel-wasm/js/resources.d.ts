import type { BytesLike, ResolverObject } from "./fontpixel_wasm.js";

/** Options of {@link withAsyncResources}. */
export interface AsyncResourceOptions {
  /** Files known up front. */
  files?: Map<string, BytesLike> | Record<string, BytesLike>;
  /** Async directory listing (`null` = the root); enables enumeration. */
  list?: (directory: string | null) => Promise<Iterable<string>> | Iterable<string>;
  /** Maximum number of runs (default 1024). */
  maxRounds?: number;
}

/**
 * Run the synchronous, deterministic `run(resources)` until it asks for no
 * unknown file, awaiting `resolve(path)` for every name a run asked for.
 * Pass the given `resources` object as the `resources` argument of the
 * fontpixel call inside `run`.
 */
export function withAsyncResources<T>(
  run: (resources: ResolverObject) => T,
  resolve: (path: string) => Promise<BytesLike | null | undefined> | BytesLike | null | undefined,
  options?: AsyncResourceOptions,
): Promise<T>;
