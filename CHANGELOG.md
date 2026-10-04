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
- Remaining format support and Python/WASM bindings follow the staged plan
- W1.3: actual PCF detector, bounded reader, deterministic standalone writer,
  independent PCF/gzip feature, typed layout options and CLI controls; all supplied
  physical/hash goldens, real corpus and cross-tool checks, reported metadata loss,
  final-code fuzzing and measured BDF-to-PCF performance.

- W1.4: bounded all-direction text/drawall rendering with borrowed packed rows,
  exact SPEC layout defaults, native ink/gap options, explicit missing/gray/loss
  policies, PNG/SVG image export and preview/render CLI; Python/TypeScript pixel
  oracle, independent image/transport checks and rendering benchmarks


- W2.7: native exhaustive-grid and explicit vector rasterization, exact quadratic/
  cubic center filling, matching embedded preference, aliases/unencoded glyphs,
  phantom/vertical metrics, TTC/OTC v1/v2 table-sharing containers, WOFF2 collection
  transforms, native registry/CLI integration, and bounded-reader validation.

- Phase 3: native HEX/PSF, Windows FNT/FON, image atlases with exact metadata,
  C bitmap headers, KBITX, Pebble PBF and HBF. Bounded sidecar resolution and
  staged multi-file CLI output preserve inputs and strict-loss destinations.
  Independent catalogue/tool comparisons, final-source fuzz and five-run release
  timings are recorded in `reports/phase-3.md`.
