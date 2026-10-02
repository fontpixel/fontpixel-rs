# fontpixel

A Rust bitmap-font library and CLI, under development. MIT, copyright Tom Chen.
The full requirements and intended 46-format scope are in [PLAN.md](PLAN.md) and
[FORMATS.md](FORMATS.md).

## Current status: W1.3 BDF and PCF read/write

BDF 2.1/2.2 can be detected, read, edited and written, with Python SPEC-v3
canonical byte parity, full horizontal/vertical metrics, source charsets,
unencoded alternates, field-level source provenance and structured diagnostics.
PCF has full byte/bit/padding/scan-unit support and a standalone deterministic
writer matching SPEC and compiler goldens. Bounded gzip handles `.bdf.gz` and `.pcf.gz`. `info` prints actual font metadata;
`convert` buffers/validates all output before safe replacement. `--strict`
rejects combined read/write losses without touching the destination.

The other 44 formats and Python/WASM bindings remain unimplemented and are
reported honestly by `formats`. Packed Mono/Gray8 operations, loss-preserving
Glyph/Font/FontSet models and exact per-glyph charsets are implemented.
See [BDF contracts](docs/formats/bdf.md), [core API](docs/bitmap-model.md),
[charset support](docs/charsets.md), and [W1.2 validation](reports/w1.2.md), [PCF contracts](docs/formats/pcf.md),
and [W1.3 validation](reports/w1.3.md).
This is a work item, not completion of phase 1 or the full compatibility facade.

```sh
cargo run -p fontpixel-cli -- info font.bdf --json
cargo run -p fontpixel-cli -- convert font.bdf -o canonical.bdf --strict
cargo run -p fontpixel-cli -- convert canonical.bdf -o canonical.bdf.gz
cargo run -p fontpixel-cli -- convert font.bdf -o font.pcf.gz
cargo run -p fontpixel-cli -- convert font.bdf -o swapped.pcf --byteorder lsb --bitorder msb --glyphpad 8 --scanunit 4
```

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
generated charset data and Unicode diagnostic tables, then runs rustfmt, warning-free clippy, tests, and
cargo-deny for licenses, advisories, bans, and sources. It also tests default, all-format, no-default,
and BDF/PCF-only feature configurations. `cargo deny` needs network access for
its advisory database. Missing tools or unavailable checks fail rather than skip.
CI is configured, but no remote workflow run is claimed by a local check.

The library's production dependency tree is empty with `--no-default-features`.
`bdf` and `pcf` independently enable shared `gzip` transport using flate2's pure-Rust backend (flate2 plus five small
support crates); neither enables the other codec. Optional `serde`
enables structured reports. Each format has its own feature; `all-formats`
selects all 46, while defaults select `bdf,pcf,yaff`. Feature selection does not
claim an implementation. Heavy `cff`/`fea` slots still add no behavior.
`cjk-charsets` includes exact CJK tables and GB18030 ranges without a runtime
dependency. BDF can preserve CJK source codes without those tables; actual CJK
conversion returns an explicit feature error when the exact table is disabled.
Python and WASM are honest, non-publishable reserved crates, without pyo3/wasm-bindgen dependencies until their phases.

## Library contracts

- `Registry<M>` owns format metadata plus separate detector/reader/writer hooks
  over a shared model. `FontRegistry` / `registry()` use the real `FontSet`
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
W1.4 can use the real model and registry to implement yaff without changing
the established BDF/PCF contracts. PCF options are available through
`PcfWriteOptions`, `PcfCodec` and `FontRegistry::set_pcf_write_options`. BDF's `source_rows` and `validate_bitmap` make raw raster
provenance explicit; other consumers must not silently render invalid-source
placeholders. The future compatibility facade selects effective mappings from
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
