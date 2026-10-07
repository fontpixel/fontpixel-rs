// Node-only helpers (`fontpixel/node-files`): NOT transactional, no
// source-alias protection, no sidecar discovery (see ops-node-files.js).
import type { BytesLike } from "./fontpixel_wasm.js";

/** Read `names` (relative to `root`) as `{name: Uint8Array}`. */
export declare function readFiles(
  names: Iterable<string>,
  options?: { root?: string },
): Promise<Record<string, Uint8Array>>;

/** Read every file below `directory` as `{"<directory>/<relative path>": Uint8Array}`. */
export declare function readDirectory(
  directory: string,
  options?: { root?: string },
): Promise<Record<string, Uint8Array>>;

/** Write `{storage key: bytes}` below `root`, one file after another; returns the paths written. */
export declare function writeOutputs(
  outputs: Record<string, BytesLike> | Map<string, BytesLike>,
  options?: { root?: string; overwrite?: boolean },
): Promise<string[]>;
