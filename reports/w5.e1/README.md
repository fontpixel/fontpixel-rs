# W5.E1 native device-font read/write evidence

Six native format families are implemented: Rockbox RB11/RB12 (Mono and Gray4),
U8/M, Cybiko, Playdate FNT/PFT, HMZK, and Sabriel SBF. No private container or
external tool is used by the library. Typed options, registry dispatch and CLI
integration are feature gated. Color W5.6 sources remained frozen.

## Verification

- 29 focused tests pass with each format enabled alone. Six tests exercise real
  registry dispatch and strict refusal of unrepresentable metadata. All six
  individual-feature Clippy runs use `-D warnings`; all six rustdoc runs use
  `RUSTDOCFLAGS='-D warnings'`. Exact commands/exits are in `checks/commands.json`.
- `crosscheck.json` records 22 independent interoperability checks, input hashes,
  tool hashes, command arguments and exits. Eight native variants are checked
  in both directions with pinned Bits’N’Picas: RB11, RB12, U8/M, Cybiko, SBF,
  HMZK, embedded Playdate FNT and split FNT/PNG. Every mapping, advance and
  covered pixel in baseline coordinates is compared; transparent cropping by
  the editor is permitted. Line ascent/descent are checked as well.
- An original Java adapter invokes the author's U8MFile API directly, comparing
  every glyph (including unencoded/default), its geometry/advance/pixels, every
  Unicode scalar mapping and all 256 native slots. This additionally avoids an
  importer convention that exposes an unencoded glyph zero as U+0000.
- An original C adapter calls unmodified official Rockbox `font_load_header`,
  `font_load_in_memory`, `font_get_width` and `font_get_bits`. Three original
  outputs cover Mono, inverse Gray4 and 32-bit offset tables (39 glyphs in all).
  The adapter checks the entire raster and advances against independently
  generated source pixels. Platform/cache stubs are outside the exercised path;
  RAM loading, validation and glyph addressing execute official code unchanged.
- Panic SDK 3.1.2 compiles our embedded and split source files. Its Simulator
  also consumes our PFT directly: the compiled game contains byte-identical PFT,
  not a pdc rewrite. Public font/image APIs validate all 15 glyphs, 945 pixels,
  cell sizes, advances/tracking and nine kerning pairs across the three flavors.
  Compressed/uncompressed official PFT fixtures use only our original pixels;
  `oracles/build_sdk_fixtures.py` reproduces both checked-in fixtures byte-for-byte.
- Six ASAN/libFuzzer short smoke runs succeed. Seeds exercise original valid
  variants and malformed/truncated inputs; the harness writes, compares a second
  deterministic write, reads using the same limits, and checks strict behavior.
  The final PFT change has an additional smoke run. Exact runs/counts/commands
  are in `fuzz/`. The parent owns the separate >=610-second campaign and quiet
  five-run release performance gate; this report does not claim those runs.

## Native findings and boundaries

Rockbox's actual u16/u32 offset boundary is `0xFFDB`. Official offsetless fixed
cells round to two bytes, whereas the Java producer packs odd-sized cells. The
reader recognizes both by raster length; the writer uses explicit offsets.
Native height and width fit positive signed 16-bit values used by the loader.
RB12 Gray4 quantizes to multiples of 17, with Loss for changed coverage.

U8/M keeps Unicode/native domains, aliasing and unencoded glyphs. Glyph/map
records stay within 256-byte pages; continuous raster bits do not pad each row.
An optional two-byte load address is a real native prefix.

SBF only limits raster **starts** to u16. The final bitmap may cross 65535; when
needed the largest bitmap is emitted last. Signed bearings and fixed advance
versus stored record advance remain distinct. Cybiko preserves unsigned geometry,
its absent-slot sentinel and space-derived baseline. The known author's four
suffixes use one stream; an unavailable historical archive is not evidence for
invented compression. HMZK retains ten opaque native header bytes and fixed
16-by-16 cells with Unicode BMP scalar mappings.

Official PFT output establishes four compressed-header words (including reserved
zero), cumulative-end page/glyph arrays, and actual tight image row strides.
The writer's four-byte image stride is accepted by the runtime. Same-page
kerning must use the short table; runtime ignores such targets in the long table.
The reader rejects noncanonical long same-page entries. In this SDK environment,
numeric `font:getGlyph(codepoint)` segfaulted for SDK-produced fonts as well as
our fonts; the documented string argument works and is used by the final oracle.
No numeric-call success is claimed. SDK startup is isolated to Xvfb/X11, local
configuration, disabled D-Bus/portal lookup, and explicit `PLAYDATE_SDK_PATH`.

Playdate source comments/metrics and known native properties are preserved.
Baseline outside i32, out-of-cell geometry, invalid Unicode or native field
width overflow is rejected. PNG is projected to alpha-weighted darkness;
opaque white/color and 16-bit sample reduction are diagnosed. The model remains
Mono/Gray8. Grayscale-to-Mono writers require a caller-selected threshold.
All unsupported metadata and mapping changes are compared against a native
readback and reported as Loss; strict writes fail. Loss diagnostics, source
model, directory structures, conversions, PNG/zlib scratch, output, and native
readback share the cumulative budget. No Unicode-wide scan or quadratic alias
search is used in production.

## Reproduction

```sh
source scripts/local-env.sh
export CARGO_TARGET_DIR="$FONTPIXEL_WORK_ROOT/target-w5e1" CARGO_BUILD_JOBS=2
cargo build -p fontpixel --example w5e1_timing --no-default-features \
  --features rockbox,u8m,cybiko,playdate,hmzk,sbf
python3 fuzz/seed_w5e1.py --output-dir "$FONTPIXEL_WORK_ROOT/w5e1-corpus"
python3 reports/w5.e1/oracles/build_rockbox.py \
  --source-dir "$FONTPIXEL_WORK_ROOT/w5e1-specs/rockbox-oracle" \
  --output-dir "$FONTPIXEL_WORK_ROOT/w5e1-specs/rockbox-oracle"
python3 reports/w5.e1/oracles/build_sdk_fixtures.py \
  --sdk "$FONTPIXEL_WORK_ROOT/w5e1-specs/PlaydateSDK-3.1.2" \
  --output-dir "$FONTPIXEL_WORK_ROOT/w5e1-sdk-fixture-repro"
# Start a caller-owned Xvfb :97, then run:
python3 tests/crosscheck/w5e1.py \
  --binary "$CARGO_TARGET_DIR/debug/examples/w5e1_timing" \
  --jar "$FONTPIXEL_WORK_ROOT/w44-oracles/BitsNPicas.jar" \
  --rockbox-reader "$FONTPIXEL_WORK_ROOT/w5e1-specs/rockbox-oracle/rockbox-read" \
  --sdk "$FONTPIXEL_WORK_ROOT/w5e1-specs/PlaydateSDK-3.1.2" \
  --output-dir "$FONTPIXEL_WORK_ROOT/w5e1-crosscheck" --display :97
```

The external compiler/consumer licenses remain with their owners. No Rockbox,
Bits’N’Picas, Gadgetbridge or SDK implementation is copied into the Rust library
or test adapters. `sources.json` records consulted primary sources and hashes;
format docs contain source links and field contracts. All checked-in rasters
are original test pixels, with no vendor fonts redistributed.

`w5e1_timing bench GLYPHS ROUNDS` covers all six families and PFT, with byte and
iteration counts. Cybiko/SBF use at most 190 consecutive representable slots;
this native capacity cap is printed rather than misreported as the requested
larger count. The parent can run the example in its isolated release window.
