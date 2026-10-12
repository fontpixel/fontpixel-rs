# Changelog

## 0.1.0 (unreleased)

First public release: the Rust crates `fontpixel`, `fontpixel-ops` and
`fontpixel-cli` on crates.io, the Python package `fontpixel` on PyPI and the
JavaScript/WebAssembly package `fontpixel` on npm, all at 0.1.0.

- 99 native font formats behind one registry: detection, bounded readers,
  deterministic writers, a structured report of repairs and losses for every
  conversion, and strict mode that refuses any loss. Bitmap formats (BDF, PCF,
  PSF, YAFF, HEX, FNT/FON, BMFont, image atlases, C headers, KBITX, PBF, HBF,
  and many console, editor, Macintosh, TeX, printer, device and system
  families), OpenType bitmap (OTB, EBDT/CBDT/sbix) and outline fonts
  (TrueType/CFF, square or round dots), WOFF/WOFF2, TTC/OTC and SVG.
- Exact charset tables (CJK tables optional), OpenType `locl` regional
  variants, subsetting, merging with baseline/spacing reports, text rendering
  to PNG/SVG.
- `fontpixel` command line: `formats`, `info`, `convert`, `preview`,
  `render`, `variants`, `subset`, `merge`, `diff`, `validate`, `charset`,
  `batch` (TOML jobs); validated in-memory outputs replace files
  safely.
- `fontpixel-ops`: the command-line operations as a typed API with memory or
  sandboxed filesystem storage, shared by the CLI and both bindings.
- Python (CPython 3.10+, abi3 wheels for Linux glibc/musl x86_64/aarch64, macOS
  x86_64/arm64, Windows x64/arm64): the complete native API, typed, plus
  `fontpixel.compat.bdfparser`.
- JavaScript (ESM + WebAssembly, Node 18+ and browsers, TypeScript
  declarations): the complete native API, Web Worker helpers, plus
  `fontpixel/compat/bdfparser` (`/cjk`, `/cli`); the full build at the package
  root and smaller `fontpixel/bdfparser`, `fontpixel/web` and
  `fontpixel/minimal` builds.
- The bdfparser and bdfparser-js compatibility layers pass the unmodified 3.x
  test suites except for the documented differences in
  [docs/bdfparser-compat-differences.md](docs/bdfparser-compat-differences.md).

### Development history

The work items before the first release, as recorded during development
(details in `reports/`):

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

- Phase 4: sixteen native console, editor, Macintosh, TeX and legacy system
  families, including CPI/CP/CPX loaders and multi-asset outputs; accepted
  validation and performance are in `reports/phase-4.md`.
- Phase 5 implementation: survey expands the catalogue to 99 native read/write
  IDs; adds Korean/Apple, terminal, printer, embedded, system and image font
  families, color output, BMFont JSON and raw packing profiles. Native directory
  inputs/outputs, cumulative CLI budgets, bounded gzip envelopes and structural
  cross-format detection are covered by independent tools and regression tests.
  Phase acceptance, source/long-fuzz boundaries and corrected five-run native
  measurements are recorded in `reports/phase-5.md`.
