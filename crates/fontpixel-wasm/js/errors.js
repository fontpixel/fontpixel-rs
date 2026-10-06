// Error classes of the `fontpixel` package: one subclass of FontpixelError per
// `fontpixel::Error` variant (mirroring the Python exceptions), plus
// UnsupportedError for capabilities this platform or build does not have.
//
// The wasm glue imports `makeError` from this module and `index.js`
// re-exports the classes from the same module, so `instanceof` works for
// errors thrown by any fontpixel call.

/** Base class of every error fontpixel throws on purpose. */
export class FontpixelError extends Error {
  constructor(message, fields) {
    super(message);
    this.name = new.target.name;
    /** Rust variant name (`InvalidInput`, `ResourceLimit`, ...). */
    this.kind = 'Other';
    /** CLI exit convention: 3 for strict loss, 2 for unusable requests, else 1. */
    this.exitCode = 1;
    if (fields) Object.assign(this, fields);
  }
}
/** Unknown format identifier (`format`). */
export class UnknownFormatError extends FontpixelError {}
/** Format compiled out of this build (`format`). */
export class FeatureDisabledError extends FontpixelError {}
/** Capability not implemented for a format (`format`, `operation`). */
export class NotImplementedFormatError extends FontpixelError {}
/** Several formats match; pass an explicit format (`candidates`). */
export class AmbiguousFormatError extends FontpixelError {}
/** No detector recognized the input. */
export class UnrecognizedFormatError extends FontpixelError {}
/** Invalid options, plain data, handles, paths or data. */
export class InvalidInputError extends FontpixelError {}
/** Fatal BDF syntax error (`line`, one-based). */
export class BdfParseError extends FontpixelError {}
/** A resource limit was exceeded (`resource`, `limit`). */
export class ResourceLimitError extends FontpixelError {}
/** A referenced sidecar was not supplied (`path`). */
export class MissingResourceError extends FontpixelError {}
/** Several output assets where one was expected (`count`). */
export class MultipleAssetsError extends FontpixelError {}
/** Strict mode rejected an information loss (`report`). */
export class StrictLossError extends FontpixelError {}
/** A codec reported error diagnostics (`report`). */
export class ReportedErrorsError extends FontpixelError {}
/** Caller resource or host I/O error (`ioKind`). */
export class FontpixelIoError extends FontpixelError {}
/**
 * Capability unavailable on this platform or build (`capability`, `reason`,
 * `evidence`, `alternative`); see `capabilities()`.
 */
export class UnsupportedError extends FontpixelError {}
/** A progress callback cancelled the operation; nothing was written or returned. */
export class CancelledError extends FontpixelError {}

const CLASSES = {
  UnknownFormat: UnknownFormatError,
  FeatureDisabled: FeatureDisabledError,
  NotImplemented: NotImplementedFormatError,
  AmbiguousFormat: AmbiguousFormatError,
  UnrecognizedFormat: UnrecognizedFormatError,
  InvalidInput: InvalidInputError,
  BdfParse: BdfParseError,
  ResourceLimit: ResourceLimitError,
  MissingResource: MissingResourceError,
  MultipleAssets: MultipleAssetsError,
  StrictLoss: StrictLossError,
  ReportedErrors: ReportedErrorsError,
  Io: FontpixelIoError,
  Unsupported: UnsupportedError,
  Cancelled: CancelledError,
};

/**
 * Build the error for `kind` (internal; called by the wasm glue).
 * `fields` is a plain object of structured attributes.
 */
export function makeError(kind, message, fields) {
  const Class = CLASSES[kind] ?? FontpixelError;
  const error = new Class(message);
  error.kind = Class === FontpixelError ? 'Other' : kind;
  if (fields) Object.assign(error, fields);
  return error;
}
