Reserved for reproducible external-tool crosschecks. No font-codec crosscheck is implemented or claimed in W0.1.

W1.2 adds `bdf.py`: all 59 supplied BDF fonts are compared byte-for-byte and
warning-for-warning with Python, plus bidirectional monobit pixels/metrics
(including unencoded glyphs). Generated files use a temporary directory. Exact
results, corpus hashes and external monobit tag differences are in
`bdf-results.json`; see docs/development.md for invocation and dependencies.

## PCF (W1.3)

After `cargo build --release -p fontpixel-cli`, run:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 tests/crosscheck/pcf.py --monobit \
  --output tests/crosscheck/pcf-results.json
python3 -B tests/crosscheck/pcf_hashes.py
python3 -B tests/crosscheck/pcf_js_hashes.py \
  --esbuild /path/to/esbuild --bundle /path/to/bdfparser.mjs
python3 -B tools/gen_pcf_cases.py --check
```

The first script checks every supplied physical PCF and BDF sample. The second
replays all 107 Python hash goldens; the third extracts only the MIT random-font
generator/flag constants to temporary files and checks all 800 TypeScript hash
goldens, including exact Rust/Python/TypeScript output agreement. It does not
install packages or modify the references. Build the reference bundle once using
`benches/bdf_node.mjs` instructions. Install monobit separately if requested;
`bdftopcf` is an external oracle only, never a runtime dependency of fontpixel.

Both hash scripts explicitly distinguish corrected nondefault compiler behavior
from the compiler's known buggy scan-unit/ink-metric output. monobit 0.54 reverses
row/column axes for two-byte encoding tables. Tests preserve that finding,
compare all glyph pixels/bearings/advances in both directions, and separately
check exact one-byte encoding maps. No silent remapping/workaround is applied.

## Text and image rendering (W1.4)

Normal Cargo tests replay 1,680 committed synthetic Python cases without needing
Python, Node, the source package or external image libraries. To regenerate and
check all supplied BDF fonts as well:

```sh
python3 -B tools/gen_render_cases.py --check
python3 -B tools/gen_render_cases.py --corpus --output /tmp/render-corpus.json
cargo run --locked --release -p fontpixel --features bdf,cjk-charsets \
  --example render_check /tmp/render-corpus.json
node tests/crosscheck/render_node.mjs /tmp/render-corpus.json /tmp/fontpixel-bdfparser.mjs
cargo build --locked --release -p fontpixel-cli
python3 -B tests/crosscheck/render_images.py --output tests/crosscheck/render-image-results.json
```

Build the temporary TypeScript module using the esbuild command in
`benches/bdf_node.mjs`; source/reference trees must stay read-only. The image test
requires separately installed Pillow and CairoSVG. It compares Python's glyph
pixels against terminal art, decoded indexed PNG and independently rasterized
SVG across eight normal direction pairs, two scales and three opaque/transparent/
translucent palettes (48 combinations). It also checks PCF, PCF.gz and BDF.gz
transports. The synthetic layout fixture additionally covers all sixteen
independent direction pairs, negative/zero advances, duplicate codes, missing
replacements, file/reverse/ascending/descending drawall and zero-area cells.

Corpus inputs and their hashes, case counts and results are in
`render-results.json`. Large generated runtime JSON (~178 MiB) is intentionally
not checked in. Layout error cases currently compare success/failure; exact Python
exception classes/text and warnings belong to W1.5's compatibility facade. Image
SVG is pixel-equivalent native output, not W2.8 contour/SVG-font byte parity.
