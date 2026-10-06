// Type declarations of the `fontpixel` package entry point. The classes,
// functions and plain-data types come from the wasm-bindgen declarations
// (`fontpixel_wasm.d.ts`, including the generated `types.d.ts` section).
import type { InitInput, SyncInitInput } from "./fontpixel_wasm.js";

export * from "./fontpixel_wasm.js";
export {
  ErrorKind,
  FontpixelError,
  UnknownFormatError,
  FeatureDisabledError,
  NotImplementedFormatError,
  AmbiguousFormatError,
  UnrecognizedFormatError,
  InvalidInputError,
  BdfParseError,
  ResourceLimitError,
  MissingResourceError,
  MultipleAssetsError,
  StrictLossError,
  ReportedErrorsError,
  FontpixelIoError,
  UnsupportedError,
  CancelledError,
} from "./errors.js";
export { withAsyncResources, AsyncResourceOptions } from "./resources.js";

/** URL of the WebAssembly module shipped next to the package entry point. */
export declare const wasmUrl: URL;

/**
 * Load and instantiate the WebAssembly module (idempotent). `source`
 * defaults to the `.wasm` next to the module (fetched in browsers and
 * workers, read with node:fs in Node).
 */
export declare function init(source?: InitInput | Promise<InitInput>): Promise<void>;

/**
 * Instantiate synchronously from bytes or a compiled `WebAssembly.Module`
 * (idempotent). Without an argument it reads the `.wasm` next to the module
 * where the runtime offers synchronous file access.
 */
export declare function initSync(source?: SyncInitInput): void;
