# W5.E2 native image-font group

PILfont, SDL SFont / Kreative RFont, Processing VLW 8–11, and native imageset
read/write implementations are complete. Existing atlas PNG/BMP/GIF/TGA/PBM/PGM
and JSON/TOML mapping behavior is retained. This group does not own SSFN/PFR.
No private font wrapper is introduced: PIL uses its native metrics plus mask;
SFont/RFont use native control pixels and PNG metadata; VLW is the native binary;
imageset is a real directory of numerically named images without a manifest.

## Validation and source evidence

- `checks/commands.json`: standalone feature tests, Clippy `-D warnings`, and
  rustdoc `-D warnings` for pilfont/sfont/vlw/image, plus combined detection and
  example checks. The combined integration suite has 26 passing tests (including
  10 pre-existing image-atlas regressions). Original fixtures, every truncation
  of native PIL/VLW/RFont samples, invalid mappings/geometry/UTF, same-limits
  readback, deterministic outputs and strict metadata loss are covered.
- `crosscheck.json`: 20 independent native checks, with exact commands, exits,
  library versions, executable hashes and all emitted fixture hashes. Pillow
  12.3 reads and actually renders PIL masks; Pillow and monobit 0.54 independently
  produce PILfont. Processing 4.5.2 consumes all four VLW versions and produces
  VLW11; indexed glyph access verifies supplementary mappings, coverage, bounds,
  advances and modified UTF names. Bits’N’Picas consumes and produces SFont/RFont.
  The unchanged official SFont.c executes its SDL loader and renderer. Monobit
  consumes and produces the native imageset directory, with exact image samples.
- `sources.json` records primary specifications and checksums. Failed discovery
  URLs are retained as errors, not represented as evidence. External Java/SDL
  reference implementation code is not incorporated into the library. Adapters
  in `oracles/` are original MIT, using public APIs; `build_sfont.py` obtains and
  verifies the unchanged native loader and SDL headers outside the repository.
- `fuzz/commands.json`, logs and `summary.json`: four ASAN fuzz smoke targets,
  each 13 seconds, totaling **2,379,613 executions**, with no crashes. The first
  VLW build encountered the shared transport module while its file was being
  added; that non-executed build is archived separately and the subsequent run
  passed. `seed_w5e2.py` generates 19 reproducible native/original seeds. Its
  FPSEED2 packet is only a fuzz resolver fixture, never a product font format.
- `source-hashes.json` identifies the frozen source, tests, docs, fixtures,
  adapters and generators. `environment.json` records tool and fuzz-binary
  hashes. Parent owns the separate >=610-second formal ASAN campaign and quiet
  five-run release performance gate; these smoke timings are not those gates.

## Exactness, loss and native-tool limitations

PIL retains signed x/y advances, rectangles, blank slots, arbitrary header/info
bytes and Mono/Gray8 masks. Native mask lookup follows PNG/GIF/PBM order. Primary
names that shadow a lookup name are rejected; dotted parent directories and
hidden filenames are handled correctly. Unsupported model fields and numeric
Unicode-to-byte coercion are reported; strict mode rejects loss.

VLW retains obsolete mbox/pointer fields as native properties, exact Gray8,
NUL and supplementary modified UTF, and v11 smooth. Output versions are typed.
Downgrades and Mono-to-Gray8 changes are audited. Processing's text API uses
UTF-16 characters; indexed access verifies stored supplementary scalars without
claiming supplementary text-rendering support.

SFont uses the original SDL midpoint cell convention. Its default ASCII 33..126
sequence and added blank slots are explicit. Optional codepoint sequences and
baseline are caller declarations, not guessed metadata. RFont preserves native
metric bands, Unicode pages, redundant codepoint columns and rfVM/rfHM v1/v2.
Table fields must agree with the image. RFont line-gap rows remain stable across
repeated writes. Native FontFamily/FontStyle/Copyright tEXt fields and unknown
text payloads are retained; non-Latin-1 names cannot use this native field and
are diagnosed. Color and sample-depth projection are reported.

Bits’N’Picas contracts transparent borders in its public glyph objects. Checks
compare every nonzero coverage sample in exact baseline coordinates and every
advance, while separately validating complete native cells. SFont's runtime
virtual space is excluded from wire comparison as an external convention, not
invented as an encoded wire slot. Its Java exporter can paint the first-glyph overhang into the vertical
column used by its own importer; Java then rejects its own output. This expected
failure and exact bytes are archived, and Rust agrees with unchanged SDL on the
same overhanging file. SDL's documented caller surface blend mode is set to NONE
so the adapter can inspect original alpha without blend quantization.

Imageset filenames carry numeric labels only. Explicit prefix/radix/domain and
Black/White/Alpha coverage profiles avoid inventing metadata. All six existing
image containers are supported; Alpha writes require PNG or uncompressed RGBA
TGA. Zero-sized images fail clearly. Missing metrics/names, added aliases as
separate glyphs, depth/threshold changes and color projection produce loss.
Listing, pathname copies, retained bytes, PNG metadata/scratch, models, outputs,
diagnostics and native readback are cumulatively charged. A directory costs one
listing operation plus one per glyph image. Selection/sorting/indexing uses
bounded ordered maps; raster work scales with total pixels, without pair scans.

Generic PNG/BMP/GIF/PNM magic gives Possible confidence. A validated native rfVM
chunk gives RFont Certain confidence. With both image and sfont enabled, generic
images retain a content ambiguity that callers resolve explicitly; filename
hints do not overpower conflicting content evidence.

## Reproduction

Source `scripts/local-env.sh`; use `CARGO_TARGET_DIR=$FONTPIXEL_WORK_ROOT/target-w5e2`
and `CARGO_BUILD_JOBS=2`. The recorded gate argv arrays are directly replayable.

```sh
cargo build -p fontpixel --no-default-features --features pilfont,sfont,vlw,image --example w5e2_timing
python3 fuzz/seed_w5e2.py --output-dir "$FONTPIXEL_WORK_ROOT/w5e2-corpus"
python3 reports/w5.e2-image/oracles/build_sfont.py --work "$FONTPIXEL_WORK_ROOT/w5e2-specs" --report reports/w5.e2-image/oracles/build-sfont.json
"$FONTPIXEL_WORK_ROOT/oracles/bin/python" tests/crosscheck/w5e2.py \
  --binary "$CARGO_TARGET_DIR/debug/examples/w5e2_timing" \
  --jar "$FONTPIXEL_WORK_ROOT/w44-oracles/BitsNPicas.jar" \
  --processing "$FONTPIXEL_WORK_ROOT/w5e2-specs/processing-core-4.5.2.jar" \
  --sdl-reader "$FONTPIXEL_WORK_ROOT/w5e2-specs/sfont-reader" \
  --output-dir "$FONTPIXEL_WORK_ROOT/w5e2-crosscheck"
```

The example supports `emit FORMAT PATH [N]`, `inspect FORMAT PATH`,
`rewrite FORMAT IN OUT`, `bench [N]` and `bench-read FORMAT PATH [ITERATIONS]`.
Writer bench is memory-only, prints actual glyph counts (PIL capped at 200,
SFont at 94), and uses the authored source model. Read bench preloads all assets
into an indexed memory resolver, warms up five times, times only bounded reads,
and checks the complete model/report after stopping the timer. Source/native
signatures remain separate. `timing-smoke.json` checks the interface on 100-glyph
PIL/VLW/RFont/imageset and 94-glyph SFont fixtures; its concurrent debug timings
must not be treated as the formal performance result.
