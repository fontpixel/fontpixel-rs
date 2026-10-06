# Binding coverage rules

One TOML file per area (`model.toml`, `formats.toml`, `ops.toml`, …); each binding
unit owns its files. Rule syntax and checker behaviour are documented in
`tools/bindings/check.py`. Run `tools/bindings/gate.sh python` (phase 7) or
`tools/bindings/gate.sh python,js` (phase 8); it fails on unmapped public items,
stale rules, missing entries or tests that don't exist.

Phase 8 (JS): add `js` (entry, camelCase; `{camelName}` in wildcard rules) and
`js_test` (`crates/fontpixel-wasm/tests/<area>.test.mjs::<test name>`) to every
`bound`/`adapted` rule of your area. `formats.toml` is generated: change
`tools/bindings/gen_formats_coverage.py` (`JS_PILOT`, `js_entry()`, `js_test_of()`),
not the file. The gate summary reports `missing.js.entry` / `missing.js.test`.
