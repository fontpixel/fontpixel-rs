# fontpixel

[![Check](https://github.com/fontpixel/fontpixel-rs/actions/workflows/check.yml/badge.svg)](https://github.com/fontpixel/fontpixel-rs/actions/workflows/check.yml)
[![crates.io](https://img.shields.io/crates/v/fontpixel.svg)](https://crates.io/crates/fontpixel)
[![docs.rs](https://img.shields.io/docsrs/fontpixel)](https://docs.rs/fontpixel)
[![PyPI](https://img.shields.io/pypi/v/fontpixel.svg)](https://pypi.org/project/fontpixel/)
[![npm](https://img.shields.io/npm/v/fontpixel.svg)](https://www.npmjs.com/package/fontpixel)

A bitmap font toolkit in Rust, with a command-line tool and Python and
JavaScript (WebAssembly) bindings: detect, read, edit, render and write
bitmap fonts in 99 formats (BDF, PCF, PSF, YAFF, HEX, FNT/FON, BMFont, image
atlases, C headers, OpenType bitmap and outline fonts, WOFF/WOFF2, TTC and
many historical formats), build regional OpenType fonts, subset and merge.
Every conversion reports what the target format cannot keep, and `--strict`
refuses any loss. MIT, copyright Tom Chen.

## Install

| | package | install |
|---|---|---|
| command line | [`fontpixel-cli`](https://crates.io/crates/fontpixel-cli) | `cargo install fontpixel-cli` (binary `fontpixel`) |
| Rust library | [`fontpixel`](https://crates.io/crates/fontpixel), [`fontpixel-ops`](https://crates.io/crates/fontpixel-ops) | `cargo add fontpixel` |
| Python 3.10+ | [`fontpixel`](https://pypi.org/project/fontpixel/) | `pip install fontpixel` |
| JavaScript (Node 18+, browsers) | [`fontpixel`](https://www.npmjs.com/package/fontpixel) | `npm install fontpixel` |

```sh
fontpixel info font.bdf --json
fontpixel convert font.bdf -o font.pcf.gz
fontpixel convert font.bdf -o font.yaff --strict   # refuses any information loss
```

```python
import fontpixel as fp
fonts = fp.read(open("font.bdf", "rb").read(), name="font.bdf").fonts
open("font.pcf", "wb").write(fp.write(fonts, "pcf").output.into_single())
```

```js
import * as fp from 'fontpixel';
await fp.init();
const { fonts } = fp.read(bytes, { name: 'font.bdf' });
const pcf = fp.write(fonts, 'pcf').output.intoSingle();
```

Both bindings include a compatibility layer for
[bdfparser](https://github.com/tomchen/bdfparser) 3.x
(`fontpixel.compat.bdfparser`) and
[bdfparser-js](https://github.com/tomchen/bdfparser-js) 3.x
(`fontpixel/compat/bdfparser`); the differences are listed in
[docs/bdfparser-compat-differences.md](docs/bdfparser-compat-differences.md).

## Documentation

- Rust: [docs.rs/fontpixel](https://docs.rs/fontpixel), the
  [native API guide](docs/api.md) and [bitmap/model contracts](docs/bitmap-model.md)
- command line: [crates/fontpixel-cli/README.md](crates/fontpixel-cli/README.md),
  [CLI guide](docs/w6.1-cli.md) and the command examples below
- Python: [guide](docs/python-api.md), [API reference](docs/python-api-reference.md),
  [examples](crates/fontpixel-py/examples)
- JavaScript: [guide](docs/js-api.md), [API reference](docs/js-api-reference.md),
  [examples](examples/js/recipes.mjs), [package README](crates/fontpixel-wasm/README.md)
- formats: [FORMATS.md](FORMATS.md) and one document per format in
  [docs/formats/](docs/formats/README.md); [charsets](docs/charsets.md)
- [CHANGELOG.md](CHANGELOG.md), [RELEASING.md](RELEASING.md),
  [design decisions](docs/decisions.md), [development](docs/development.md),
  [project status and history](docs/project-status.md),
  [requirements (PLAN.md)](PLAN.md)

## Format notes

BDF 2.1/2.2 can be detected, read, edited and written, with Python SPEC-v3
canonical byte parity, full horizontal/vertical metrics, source charsets,
unencoded alternates, field-level source provenance and structured diagnostics.
PCF has full byte/bit/padding/scan-unit support and a standalone deterministic
writer matching SPEC and compiler goldens. Bounded gzip handles `.bdf.gz` and `.pcf.gz`. `info` prints actual font metadata;
`convert` buffers/validates all output before safe replacement. `--strict`
rejects combined read/write losses without touching the destination.

`preview` renders terminal art; `render` exports bounded PNG/SVG images with
scale, RGBA colors, all direction pairs, exact charset mapping and strict loss
checks. See [rendering API/CLI](docs/rendering.md) and [W1.4 validation](reports/w1.4.md).

YAFF 1.0 supports single/multi-strike monochrome and grayscale fonts, with
bounded gzip and lossless versioned native-metadata extensions for golden tests.
See [YAFF contracts](docs/formats/yaff.md) and [W1.6 validation](reports/w1.6.md).

GNU Unifont HEX supports 8/16/24/32-wide cells, explicit alternate heights and
baseline conventions. Linux PSF1/PSF2 support Unicode aliases and complete
combining sequences, with deterministic gzip and strict loss reporting.
The CLI includes both; library users enable `hex` and/or `psf` explicitly.
See [HEX](docs/formats/hex.md), [PSF](docs/formats/psf.md) and
[W3.1 validation](reports/w3.1.md).

Windows raster FNT v1/v2/v3 and NE FON support bounded deterministic read/write;
PE32/PE32+ FON resources can also be read. FON keeps multiple strikes. Legacy
byte charsets remain explicit; Unicode export uses exact selected code pages,
with loss reports for unsupported mappings and metadata. The CLI includes both;
library users enable `fnt` or `fon` (which also enables `fnt`).
See [FNT](docs/formats/fnt.md), [FON](docs/formats/fon.md) and
[W3.2 independent validation](reports/w3.2-review.md).

The all-feature build exposes 99 native readers and writers, including CBDT/SBIX
color output. Each format document states its native variants, metadata and loss
boundaries; this does not imply all encodings of every historical container are
supported. Packed Mono/Gray8 operations, loss-preserving
Glyph/Font/FontSet models and exact per-glyph charsets are implemented.
See [BDF contracts](docs/formats/bdf.md), [core API](docs/bitmap-model.md),
[charset support](docs/charsets.md), and [W1.2 validation](reports/w1.2.md), [PCF contracts](docs/formats/pcf.md),
and [W1.3 validation](reports/w1.3.md).
The optional `compat-bdfparser` facade and `fontpixel-parity` executable pass the
full default differential suite; see [W1.5 validation](reports/w1.5.md).
The merged phase-1 implementation passed its final aggregate gate; see the
[phase 1 acceptance report](reports/phase-1.md) for evidence and remaining scope.

```sh
cargo run -p fontpixel-cli --bin fontpixel -- info font.bdf --json
cargo run -p fontpixel-cli --bin fontpixel -- convert font.bdf -o canonical.bdf --strict
cargo run -p fontpixel-cli --bin fontpixel -- convert canonical.bdf -o canonical.bdf.gz
cargo run -p fontpixel-cli --bin fontpixel -- convert font.bdf -o font.pcf.gz
cargo run -p fontpixel-cli --bin fontpixel -- convert font.bdf -o golden.yaff --strict
cargo run -p fontpixel-cli --bin fontpixel -- convert golden.yaff -o golden.yaff.gz
cargo run -p fontpixel-cli --bin fontpixel -- convert font.bdf -o swapped.pcf --byteorder lsb --bitorder msb --glyphpad 8 --scanunit 4
```

Console examples (HEX/PSF cannot retain source names and all BDF metadata):

```sh
fontpixel convert source.hex -o console.psf.gz --to psf
fontpixel convert source.hex -o console.psf --to psf --psf-version 1 --psf1-count 512
fontpixel info eight-row.hex --hex-height 8 --json
fontpixel convert source.bdf -o unifont.hex --hex-height 16 --hex-descent 2
fontpixel convert source.psf -o restored.psf --to psf --strict
```

Windows examples (FNT cannot retain every BDF field or characters beyond the
selected code page's single-byte range):

```sh
fontpixel convert source.bdf -o latin.fnt --to fnt --fnt-version 3 --fnt-charset 0
fontpixel convert small.fnt large.fnt -o family.fon --to fon --strict
fontpixel info legacy.fon --fnt-read-code-page cp437 --json
```

Image atlases preserve exact glyph metadata in a JSON or TOML sidecar. Keep both
files together; either the image or mapping can be the input entry point. PNG,
BMP, GIF, TGA, PBM and PGM output are supported. External atlas images need an
explicit character map. Imageset directories use explicitly selected numeric
filename interpretation. C headers support font8x8, t-mat, explicitly described byte arrays
and Adafruit GFX; generic and GFX output report metadata and mapping losses.

```sh
fontpixel convert source.bdf -o atlas.png --to image --strict
fontpixel convert source.bdf -o atlas.pgm --to image --image-mapping-format toml --strict
fontpixel convert atlas.json --from image -o restored.bdf --strict
fontpixel info external.png --from image --image-mapping external.json --json
fontpixel convert font8x8.h -o imported.bdf
fontpixel convert source.bdf -o font.h --to c --c-family gfx --c-range U+0020-U+007F
fontpixel info cells.h --c-width 8 --c-height 16 --c-first U+0020 --c-charset unicode --json
```

Library features are `image` and `c`. See [image maps](docs/formats/image.md)
and [C arrays](docs/formats/c.md) for geometry, limits and loss behavior.

KBITX retains native grayscale, names and supported metadata. Pebble PBF supports
v1/v2/v3 and v3 RLE4; legacy files with partial RLE rows report the discarded tail.
The CLI exposes explicit threshold, trim, compression and wire-version choices.

```sh
fontpixel info source.kbitx --json
fontpixel convert source.kbitx -o mono.bdf --kbitx-read-threshold 128
fontpixel convert source.bdf -o watch.pbf --pbf-version 3 --pbf-compression rle4
fontpixel info watch.pbf --pbf-trim --json
```

Library features are `kbitx` and `pbf`; see [KBITX](docs/formats/kbitx.md) and
[PBF](docs/formats/pbf.md) for metadata, geometry and loss behavior.

HBF 1.1 reads and writes a text header plus one or more bitmap files. Keep all
assets together. Legacy codes retain their declared charset, including CNS
planes; no Unicode remapping is inferred. Output requires uniform positive
glyph bounds and one code per glyph. Gray8 conversion requires
`--gray-threshold` and reports loss. Library users enable `hbf`; see
[HBF contracts](docs/formats/hbf.md).

```sh
fontpixel info font.hbf --json
fontpixel convert fixed.bdf -o font.hbf
fontpixel convert font.hbf -o split.hbf --hbf-glyphs-per-file 4096 --strict
```

## Build and inspect

Install Rust 1.99.0 with rustfmt and clippy (rustup honors rust-toolchain.toml),
then:

```sh
cargo run -p fontpixel-cli --bin fontpixel -- formats
cargo run -p fontpixel-cli --bin fontpixel -- formats --json
cargo run -p fontpixel-cli --bin fontpixel -- info --help
cargo run -p fontpixel-cli --bin fontpixel -- convert --help
cargo run -p fontpixel-cli --bin fontpixel -- preview --help
cargo run -p fontpixel-cli --bin fontpixel -- render --help
cargo install cargo-deny --version 0.20.2 --locked
scripts/check.sh
```

The complete check requires Python >= 3.12 (standard library only) to verify
generated charset data and Unicode diagnostic tables, then runs rustfmt, warning-free clippy, tests, and
cargo-deny for licenses, advisories, bans, and sources. It also tests default, all-format, no-default,
and BDF/PCF-only feature configurations. `cargo deny` needs network access for
its advisory database. Missing tools or unavailable checks fail rather than skip.
CI is configured, but no remote workflow run is claimed by a local check.

The library's production dependency tree is empty with `--no-default-features`.
`bdf` and `pcf` independently enable shared `gzip` transport using flate2's pure-Rust backend (flate2 plus five small
support crates); neither enables the other codec. Optional `serde`
enables structured reports. Each format has its own feature; `all-formats`
selects all 99, while defaults select `bdf,pcf,yaff`. Feature selection does not
claim an implementation. `cff` enables the native CFF1 table reader/encoder and
its `sfnt` dependency; `fea` remains a reserved feature with no implementation.
`cjk-charsets` includes exact CJK tables and GB18030 ranges without a runtime
dependency. BDF can preserve CJK source codes without those tables; actual CJK
conversion returns an explicit feature error when the exact table is disabled.

## JavaScript / WASM

`crates/fontpixel-wasm` is the npm package `fontpixel`: the complete native
API (ESM, WebAssembly, Node 18+ and browsers, TypeScript declarations) and a
separate `fontpixel/compat/bdfparser` API, plus smaller builds for browsers
(see the [package README](crates/fontpixel-wasm/README.md)). See the
[JS guide](docs/js-api.md), [generated reference](docs/js-api-reference.md),
and [executable examples](examples/js/recipes.mjs).

```sh
npm install fontpixel
```

From the repository:

```sh
source scripts/local-env.sh
scripts/build_wasm.sh full          # also: minimal, merge, web, convert, bdfparser
node examples/js/run.mjs
scripts/build_npm.sh ../local-work/npm   # the npm package and its tarball
```

Call `await init()` before using the native API. Inputs and outputs are buffers
and named resource bundles. Use `createWorker()` for CPU work, progress and
cancellation; persistence is an explicit host operation. Check `capabilities()`
and `formats.catalogue()` for the selected profile.

## Python

`crates/fontpixel-py` is the Python package `fontpixel`: the complete native API
(formats, model, bitmaps, rendering, OpenType/WOFF, variants, subsets, merge and
every command-line operation in-process) plus `fontpixel.compat.bdfparser`, a
bdfparser-compatible layer. It is typed (stubs and `TypedDict` option shapes)
and documented in the [Python guide](docs/python-api.md) and the
[API reference](docs/python-api-reference.md). CPython 3.10+, one abi3 wheel
per platform:

```sh
pip install fontpixel
```

```python
import fontpixel as fp

fonts = fp.read(open("tests/data/bdf/spleen-5x8.bdf", "rb").read()).fonts
font = fonts[0]
print(font.glyph_for_codepoint("A").bitmap.to_text())
png = fp.to_png(font.draw("Hello").bitmap, {"scale": 4})
pcf = fp.write(fonts, "pcf", {"glyph_pad": "4"}).output.into_single()

from fontpixel.compat.bdfparser import Font      # existing bdfparser code
print(Font("tests/data/bdf/spleen-5x8.bdf").draw("Hello"))
```

[Examples](crates/fontpixel-py/examples) cover conversion, glyph editing,
rendering, typed options, OpenType/WOFF2/variants/subsets, merge, batch jobs,
sidecar resources, the bdfparser layer and error handling. For development:
`maturin develop --release` in a virtual environment, then
`pytest crates/fontpixel-py/tests`, or `scripts/check_python.sh`;
`scripts/build_wheels.sh` builds and tests release wheels and the sdist.

## Library contracts

- `Registry<M>` owns format metadata plus separate detector/reader/writer hooks
  over a shared model. `FontRegistry` / `registry()` use the real `FontSet`
- `Input` supplies primary bytes and an optional caller-owned `ResourceResolver`;
  `ReadContext` validates primary, sidecar, aggregate-byte, and resource-count
  limits, shared between detection and decoding
- `ResourcePath` rejects absolute/parent/current/noncanonical path components.
  Library callers provide the resolver; the CLI resolves sidecars through a
  directory handle rooted at the primary input's parent, with bounded reads
- Writers return an in-memory `OutputBundle` with a primary entry point and all
  sidecar assets. Names are validated, duplicate/file-directory collisions are
  rejected, and traversal is deterministic. `into_single()` refuses multi-assets
- Reports retain severity, stable code, glyph/codepoint/asset location, and a
  separate information-loss marker. Strict conversion rejects combined read/write
  loss before returning output assets. The registry does no filesystem writes
- Content evidence beats filename hints; equally strong candidates and ambiguous
  extensions return an error rather than silently selecting a format

The synthetic codec used in `crates/fontpixel/tests/contracts.rs` tests these
contracts with an invented byte protocol. It is not registered in production and
is not evidence for any of the 99 font formats.

## Next integration points

Exact generated tables and public conversion helpers live in
`crates/fontpixel/src/charset/`. Regenerate/check with `tools/gen_charsets.py`;
use `tools/check_charsets.py` for complete cross-language mapping comparison.
The compatibility facade and YAFF codec use the shared model and established
BDF/PCF contracts. General OpenType codecs and independent validation are covered
by the [phase 2 acceptance report](reports/phase-2.md).
PCF options are available through
`PcfWriteOptions`, `PcfCodec` and `FontRegistry::set_pcf_write_options`. BDF's `source_rows` and `validate_bitmap` make raw raster
provenance explicit; other consumers must not silently render invalid-source
placeholders. The compatibility facade selects effective mappings from
source duplicate glyphs. See the integration section of [W1.2](reports/w1.2.md).
See [bitmap/model contracts](docs/bitmap-model.md) and [core benchmarks](benches/REPORT.md).
See [design decisions](docs/decisions.md) and [development tools](docs/development.md).

## Core example

```rust
use fontpixel::{Bitmap, Font, FontSet, Glyph};
use fontpixel::model::Codepoint;
fn main() -> fontpixel::Result<()> {
    let bitmap: Bitmap = ".#.\n###".parse()?;
    let mut glyph = Glyph::new(bitmap.enlarge(2, 2)?);
    glyph.codepoints.push(Codepoint::Unicode(65));
    let font = Font::from_glyphs([glyph])?;
    let set = FontSet::from(font);
    assert_eq!(set.single()?.len(), 1);
    Ok(())
}
```

### Embedded bitmap sfnt (W2.4)

Optional `otb` and `bdat` features enable native multi-strike bitmap sfnt reading
and writing. `color` enables CBDT/CBLC and PNG/dupe sbix reading, with reported
alpha-to-Gray8 projection, and CBDT/sbix writing of Mono/Gray8 coverage as black
PNG pixels with alpha. The native model does not preserve RGBA artwork. Use
`formats::embedded::{read, write, EmbeddedReadOptions, EmbeddedWriteOptions}` for
typed bounds, color darkness/threshold choices and EBSC instructions.
`sfnt::bitmap` exposes editable glyph-ID strikes and table pairs for embedding in
existing TTF/OTF builders. Registry/CLI use the same native implementation.

    cargo run -p fontpixel-cli --features all-formats -- convert font.bdf --to otb -o font.otb
    cargo run -p fontpixel-cli --features all-formats -- info font.otb --json

See `docs/formats/otb.md`, `bdat.md`, and `color.md` for limits, loss reporting,
unsupported Apple Huffman/JPEG/TIFF cases and independent-tool caveats. General
outline conversion and collections are covered below.

### Regional OpenType fonts and web subsets (W2.6)

The default CLI now includes native `variants` and `subset`. No Python, FEA
compiler or external font program is used at runtime.

```sh
fontpixel variants --base SC.bdf --lang zh-Hant,zh-HK=TC.bdf \
  --lang ja=JP.bdf --lang ko=KR.bdf \
  --script latn,grek,cyrl=Latin.bdf -o regional.woff2
fontpixel variants --base SC.bdf --lang ja=JP.bdf \
  --lang-scripts ja=DFLT,hani,kana --shape dot --dotsize 0.9 \
  --embed-bitmaps --family 'My Regional Pixel Font' -o regional.ttf
fontpixel subset regional.woff2 --text '漢字 “Hello”…' -o small.woff2
fontpixel subset regional.otf --unicodes U+0020-007E,U+4E00-4EFF -o small.otf
fontpixel subset regional.ttf --text-file page.txt -o small.woff
```

Languages accept explicit OpenType tags (`ZHT`, `JAN`, etc.) or mapped BCP-47
language identifiers. `--lang-scripts` configures each language's script scope.
By default languages apply to DFLT/hani/kana/hang/bopo; script alternatives apply
to their script in every language. Identical alternate bitmaps and complete
metrics share one glyph. Alternative-only characters become default mappings.
Square web output defaults to CFF and dots to TrueType; override using
`--woff2-source cff|ttf|auto`. Quality defaults to 11; `--woff2-quality` and
`--woff2-threads` expose the existing bounded compressor controls.

Subsets preserve related single-substitution layout, composite dependencies,
raw CFF programs/hints/subroutines, names, metric arrays, post names, and supported
embedded strikes. Glyph IDs are compacted consistently. WOFF envelope metadata
survives web-to-web subsetting. Unsupported glyph-indexed tables/layout forms,
nondefault legacy CFF encoding, UVS and non-Unicode cmap records fail explicitly.
An exact unsupported table can be discarded with `--drop-table TAG`, generating
a loss report; `--strict` rejects that loss before replacing the destination.
A TTF subset remains TrueType and a CFF subset remains CFF; both can be wrapped
as WOFF or WOFF2. General raster reading and TTC support are described below.

Rust API: `Variants::new`, `add_language`, configurable `LanguageVariant` and
`ScriptVariant`, `Variants::plan`, `variants::encode`, `sfnt::gsub::Gsub` and
`sfnt::subset::subset`. Core BDF/PCF-only builds acquire no new dependencies.


## OpenType raster and collection conversion

TTF, CFF OTF, WOFF, WOFF2 and TTC/OTC now use the unified native reader. A proven
4–64 ppem integral grid is recovered automatically. Matching embedded images
win by default. Ordinary/off-grid/dot outlines require an explicit size:

```sh
fontpixel convert pixels.ttf -o restored.bdf
fontpixel convert round.otf --ppem 14 -o restored.bdf
fontpixel info family.ttc --face-index 1 --json
fontpixel convert small.bdf large.bdf -o family.ttc
fontpixel convert pixels.bdf --shape dot --embed-bitmaps -o round.woff2
```

`--no-embedded-bitmaps` requests outlines; `--grid-units` (with `--ppem`) and
`--pixel-offset-x/--pixel-offset-y` are explicit reviewed raster overrides.
`--ppu`, `--upm`, `--dotsize`, `--gray-threshold`, `--woff2-source`,
`--woff2-quality` and `--collection-v2` control native output. A TTC FontSet write
maps each Font to one independent face. Existing destinations stay intact after
errors or strict-loss refusal.

Rust APIs are `formats::vector_raster::{read, write, detect_native_ppem}`,
`sfnt::collection::{Collection, encode}`, and configurable registry methods
`set_vector_read_options` / `set_vector_write_options`. The low-level collection
and webfont APIs preserve original table semantics; raster conversion reports
unrepresentable source information. General hint execution, variable instances,
CFF2, and sparse anisotropic fallback remain explicit unsupported cases.
See [raster contracts](docs/formats/vector-raster.md),
[collection contracts and deployed-tool limits](docs/formats/ttc.md),
and [W2.7 validation](reports/w2.7.md). The [phase 2 report](reports/phase-2.md)
records the complete phase's acceptance.

### SVG images and SVG 1.1 fonts

With the `svg` feature (enabled in the CLI), `convert` writes path-based SVG
fonts and reads their native pixel geometry back. `render` emits standalone SVG
images that require no installed font. For a single glyph, render one character,
or use `formats::svg::glyph_image` on its original bitmap rectangle.

```sh
fontpixel convert source.bdf -o font.svg
fontpixel convert font.svg -o recovered.bdf
fontpixel convert source.bdf -o dots.svg --shape dot --dotsize 0.9 --ppu 100
fontpixel render source.bdf 'Hello, 世界' -o text.svg --scale 4 --shape square
fontpixel render source.bdf '永' -o glyph.svg --mode ink --shape dot
fontpixel info external.svg --ppem 16 --json
```

A generic off-grid SVG font requires explicit `--ppem`. SVG font collections use
`--face-index`; default import retains all fonts. Only path-filled SVG glyphs are
supported; scripts, external references, CSS, strokes and graphical effects are
rejected. Full CJK dot documents may exceed the default 64 MiB input budget; the
CLI supports `--max-input-bytes 268435456` to reopen those larger fonts; the
library also exposes typed limits. SVG 1.1 font embedding
has limited support in modern browsers; use path images for portable previews.
See [the SVG format contract](docs/formats/svg.md) for metadata loss and bounds.


## Restored phase-6 quickstart and command reference

The project used pinned Rust 1.99.0. Once the recovered project is independently
reviewed and execution authorized, use its rust-toolchain file and lockfile.
`scripts/local-env.sh` places disposable artifacts outside the source checkout.
Do not hardcode the disappeared cloud toolchain path into public installation docs.

```sh
source scripts/local-env.sh
cargo install --locked --path crates/fontpixel-cli --all-features --bin fontpixel
```

This describes source installation, not a published crates.io release.

## Every command

```sh
fontpixel formats --json
fontpixel info font.bdf --json
fontpixel convert font.bdf -o font.yaff --strict --report json
fontpixel preview font.bdf 'ABC' --chars '#.'
fontpixel render font.bdf 'ABC' -o preview.png --scale 4 --report json
fontpixel vectorize font.bdf -o font.ttf --shape square --report json
fontpixel variants --base SC.bdf --lang zh-Hant,zh-HK=TC.bdf --lang ja=JP.bdf --lang ko=KR.bdf --script latn,grek,cyrl=Latin.bdf --family 'Example Regional' -o regional.woff2
fontpixel subset regional.woff2 --text 'ABC' -o subset.woff2
fontpixel diff before.bdf after.bdf --json
fontpixel validate font.ttf --json
fontpixel charset font.bdf --to cp1252 -o cp1252.bdf --report json
fontpixel merge --family 'Example Mix' --part cjk.bdf --take cjk --part latin.bdf --take western --spacing prop -o merged.yaff --report json
fontpixel batch examples/batch.toml --jobs 2 > report.json
```

The CLI registers `vectorize` as a visible alias of convert (shown in
`fontpixel --help`). Native conversion is through the bitmap
model; it is not a table-preserving wrapper conversion. Select an explicit
`--font-index` for a multi-font input when needed. Reconstructed `--each` needs explicit `--to`
and an existing output directory; all outputs share one normal-error rollback
transaction and duplicate stems fail. `--each --font-index` selects per input.
`--woff2-threads` controls within-font compression separately from batch jobs.

Standalone commands reserve input identities from the original total byte limit
`L`: `I = min(1 MiB, floor(L / 256))` for the shared identity table, leaving
`L - I` for model/codec work. The default total remains 256 MiB, split into
1 MiB identity storage and 255 MiB working allowance. The full identity slice
stays reserved through the command, including sidecar discovery and persistence;
unused table space is not returned to the working allowance. This tightens the
effective working budget; the full default Tong, Unifont and catalogue workloads
pass under it (phase 6 report). See the
[budget contract](docs/w6.1-writer-budget.md) for retained-capacity accounting and
its limits; it does not cover every codec transient or bound process RSS.

`batch` always emits JSON and does not take `--report`. Relative paths resolve
from the TOML file. Output parents must exist. Independent tasks may prepare
successfully after another task fails, but all successful writes enter one
protected transaction after all input/sidecar identities are known. It has no
dependency graph. Consult the reconstructed batch contract for worker/report
spooling details and their limits. Its unchanged 512 MiB default
`max_pending_bytes` covers prepared outputs plus the shared source-identity table;
the standalone partition does not reduce batch task allowances. Job/path/task caps, working reservations
and report quotas are separate, and none is an RSS promise.

`merge` requires a new family, a selection per ordered part and at least one
output. Scoped `--take`, `--dy`, `--from`, `--font-index` bind to the preceding
part; `--to` binds to the preceding output. Positive dy raises ink. Geometric
half/full is a width policy separate from source selection. Full preserves
supplied geometry metrics; it does not synthesize a fullwidth glyph. Reconstructed strict BDF
merge rejects multiline-notice projection even with known licences; YAFF can
retain the model's full names directly. Those restored code paths compile and
the workspace tests pass; historical results do not validate them.

Strictness checks accumulated reader/transformation/writer loss. Warning severity
alone does not determine loss. Respect all sidecar assets, resource budgets and
format profiles. Caller-owned filesystem resolvers need symlink and source-alias
protection in addition to lexical ResourcePath validation.


Read [the native API guide](docs/api.md), [Tong migration](docs/migration-tong.md),
[catalogue migration and open gaps](docs/migration-fontpixel.md), and the
[batch/inspection contract](docs/w6.1-cli.md). Every flag in the commands here
matches the current `--help` (checked 2026-10-05); all output directories must be
created explicitly.

The filesystem-free [API quickstart](crates/fontpixel/examples/api_quickstart.rs)
has been rewritten against current source signatures, including names, retained
budgets, merge, sidecars, embedded metrics, regional names, subsets and WOFF2.
It compiles under `cargo clippy --all-targets` but is not run by `scripts/check.sh`;
the [crate-root guide](crates/fontpixel/docs/api-rustdoc.md) examples run as doctests. [Static documentation checks](examples/check_api_documentation.py)
inspect files and declarations only; they do not compile Rust or invoke the CLI.

The [documentation smoke runner](examples/README.md#recording-cli-smokes) records
18 CLI scenarios and 84 tiny Tong tasks against an already built, identified
all-feature binary. Its new Han/geometric fixture is explicitly synthetic.
The 99-format/13-command inventory alone does not run those font operations;
neither inventory nor tiny smoke establishes full-font correctness or performance.
