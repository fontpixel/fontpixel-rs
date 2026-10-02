# Changelog

## Unreleased

- W0.1: workspace and planned feature catalogue, structured reports/errors,
  generic registry, bounded multi-resource input and multi-asset output contracts,
  CLI dispatch skeleton, infrastructure tests, and required CI checks
- W0.2: exact generated charset tables, strict per-glyph conversion, canonical
  names/properties, GL and GB18030 ranges, optional CJK feature, Windows charset
  resolution, 94×94 grid helpers, and exhaustive Python crosschecks
- W1.1: bounded packed Mono/Gray8 operations, explicit palettes/thresholds,
  wire layouts, loss-preserving Glyph/Font/FontSet with indexed multi-encoding and
  Unicode-sequence lookup, real-model registry wiring, Python-oracle/scalar tests,
  Criterion and five-batch wall-clock benchmarks
- W1.2: SPEC-v3 BDF parsing and deterministic writing, effective/source metadata,
  edited-field reconciliation, raw bitmap provenance, structured repairs/losses,
  bounded gzip transport, live registry capabilities, CLI inspection and safe
  conversion, independent oracle/real-font/property tests and BDF benchmarks
- All other font codecs and Python/WASM bindings remain unimplemented
- W1.3: actual PCF detector, bounded reader, deterministic standalone writer,
  independent PCF/gzip feature, typed layout options and CLI controls; all supplied
  physical/hash goldens, real corpus and cross-tool checks, reported metadata loss,
  final-code fuzzing and measured BDF-to-PCF performance. W1.4 remains pending.
