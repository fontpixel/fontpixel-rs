# WOFF container layer

- `woff1::{encode, decode}` wraps any supported standalone sfnt, with per-table
  zlib and unknown-table preservation.
- `woff2::{encode, decode}` adds glyf/loca preprocessing, optional hmtx, and pure
  Rust Brotli. Source sfnt tables already contain outlines/bitmaps/GSUB; this
  layer does not generate or rasterize them.
- `woff2::encode_many` preserves order and bounds concurrent font jobs. The
  per-font `compression_threads` option separately enables legal intra-font
  Brotli stream parallelism. The product is capped at 64. Byte/work limits are
  per font; callers must choose a worker count that fits aggregate memory.
- Read/write diagnostics are explicit. WOFF2 removes invalidated DSIG signatures
  and reports loss. A strict caller should reject `report.has_loss()` before
  persisting output.
- Keep `Decoded.auxiliary` and pass it into the next encoder's options to retain
  metadata/private data and header version. Metadata is transported opaquely;
  XML validation/display is the caller's responsibility.
- WOFF2 quality is 0–11, default 11; WOFF1 zlib level is 0–9, default 9. Same
  bytes and options produce the same output. Intra-font threading changes
  compression decisions and size, without changing reconstructed font semantics.

See `docs/formats/woff.md`, `docs/formats/woff2.md`, `tests/crosscheck/woff.py`
and `benches/woff.py`. Collections and bitmap-model/outline registry integration
remain explicitly assigned to W2.7, as documented in `docs/decisions.md`.
