import type { Report } from "./fontpixel_wasm.js";

/** Rust `fontpixel::Error` variant names, plus `Unsupported` and `Other`. */
export type ErrorKind =
  | "UnknownFormat"
  | "FeatureDisabled"
  | "NotImplemented"
  | "AmbiguousFormat"
  | "UnrecognizedFormat"
  | "InvalidInput"
  | "BdfParse"
  | "ResourceLimit"
  | "MissingResource"
  | "MultipleAssets"
  | "StrictLoss"
  | "ReportedErrors"
  | "Io"
  | "Unsupported"
  | "Other";

/** Base class of every error fontpixel throws on purpose. */
export class FontpixelError extends Error {
  /** Rust variant name. */
  kind: ErrorKind;
  /** CLI exit convention: 3 strict loss, 2 unusable request, else 1. */
  exitCode: number;
  /** Full diagnostics of strict-loss and reported-error failures. */
  report?: Report;
}
/** Unknown format identifier. */
export class UnknownFormatError extends FontpixelError {
  format: string;
}
/** Format compiled out of this build. */
export class FeatureDisabledError extends FontpixelError {
  format: string;
}
/** Capability not implemented for a format. */
export class NotImplementedFormatError extends FontpixelError {
  format: string;
  /** `"Read"` or `"Write"`. */
  operation: string;
}
/** Several formats match; pass an explicit format. */
export class AmbiguousFormatError extends FontpixelError {
  candidates: string[];
}
/** No detector recognized the input. */
export class UnrecognizedFormatError extends FontpixelError {}
/** Invalid options, plain data, handles, paths or data. */
export class InvalidInputError extends FontpixelError {}
/** Fatal BDF syntax error. */
export class BdfParseError extends FontpixelError {
  /** One-based source line. */
  line: number;
}
/** A resource limit was exceeded. */
export class ResourceLimitError extends FontpixelError {
  resource: string;
  limit: number;
}
/** A referenced sidecar was not supplied. */
export class MissingResourceError extends FontpixelError {
  path: string;
}
/** Several output assets where one was expected. */
export class MultipleAssetsError extends FontpixelError {
  count: number;
}
/** Strict mode rejected an information loss. */
export class StrictLossError extends FontpixelError {
  report: Report;
}
/** A codec reported error diagnostics. */
export class ReportedErrorsError extends FontpixelError {
  report: Report;
}
/** Caller resource or host I/O error. */
export class FontpixelIoError extends FontpixelError {
  /** Rust `std::io::ErrorKind` name. */
  ioKind: string;
}
/** Capability unavailable on this platform or build; see `capabilities()`. */
export class UnsupportedError extends FontpixelError {
  /** Capability name (absent for an `Io` error of kind `Unsupported`). */
  capability?: string;
  reason?: string;
  evidence?: string;
  alternative?: string;
  ioKind?: string;
}
