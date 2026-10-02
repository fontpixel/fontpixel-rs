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
