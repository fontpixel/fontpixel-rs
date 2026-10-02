# fontpixel

A Rust bitmap-font library and CLI, under development. MIT, copyright Tom Chen.
The full requirements and intended 46-format scope are in [PLAN.md](PLAN.md) and
[FORMATS.md](FORMATS.md).

## Current status: W0.1 infrastructure and W0.2 exact charsets

No real font format can be read, written, or structurally detected yet. `formats`
lists all planned formats and marks every reader/writer **not implemented**.
`info` and `convert` parse and dispatch requests but return an explicit error;
conversion never creates or truncates an output file. Models, bitmap operations,
gzip decoding, and Python/WASM bindings belong to later items. Exact per-glyph charset maps are implemented; see
[charset support](docs/charsets.md).
Passing infrastructure tests does not establish font-format correctness.

## Build and inspect

Install Rust 1.99.0 with rustfmt and clippy (rustup honors rust-toolchain.toml),
then:

```sh
cargo run -p fontpixel-cli -- formats
cargo run -p fontpixel-cli -- formats --json
cargo run -p fontpixel-cli -- info --help
cargo run -p fontpixel-cli -- convert --help
cargo install cargo-deny --version 0.20.2 --locked
scripts/check.sh
```

The complete check requires Python >= 3.12 (standard library only) to verify
generated charset data, then runs rustfmt, warning-free clippy, tests, and
cargo-deny for licenses, advisories, bans, and sources. It also tests default, all-format, no-default,
and BDF/PCF-only feature configurations. `cargo deny` needs network access for
its advisory database. Missing tools or unavailable checks fail rather than skip.
CI is configured, but no remote workflow run is claimed by a local check.

The library's production dependency tree is empty with `--no-default-features`
or just `bdf,pcf`; optional `serde` enables structured report serialization. Each
format has its own feature and `all-formats` selects all 46. Default features are
`bdf,pcf,yaff`. Selecting a feature reserves that capability, not an implementation.
Heavy dependency feature slots (`cff`, `fea`, `gzip`) currently add no dependency
or behavior. `cjk-charsets` includes exact CJK tables and GB18030 ranges without
adding a runtime dependency; without it, known CJK charsets return a feature error.
Python and WASM are honest, non-publishable reserved crates, without pyo3/wasm-bindgen dependencies until their phases.

## Library contracts

- `Registry<M>` owns format metadata plus separate detector/reader/writer hooks
  over a shared model. Phase 1 will specialize it with the actual `FontSet`
- `Input` supplies primary bytes and an optional caller-owned `ResourceResolver`;
  `ReadContext` validates primary, sidecar, aggregate-byte, and resource-count
  limits, shared between detection and decoding
- `ResourcePath` rejects absolute/parent/current/noncanonical path components.
  A future filesystem resolver must additionally enforce a symlink-safe root
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
is not evidence for any of the 46 font formats.

## Next integration points

Exact generated tables and public conversion helpers live in
`crates/fontpixel/src/charset/`. Regenerate/check with `tools/gen_charsets.py`;
use `tools/check_charsets.py` for complete cross-language mapping comparison.
W1.1 will supply the real model and bitmap operations; later codec items add their
format documentation first and then register actual hooks under feature gates.
See [design decisions](docs/decisions.md) and [development tools](docs/development.md).
