# Binding coverage rules

One TOML file per area (`model.toml`, `formats.toml`, `ops.toml`, …); each binding
unit owns its files. Rule syntax and checker behaviour are documented in
`tools/bindings/check.py`. Run `tools/bindings/gate.sh python` (phase 7) or
`tools/bindings/gate.sh python,js` (phase 8); it fails on unmapped public items,
stale rules, missing entries or tests that don't exist.
