# Development tools and test scope

Rust 1.99.0 is pinned in rust-toolchain.toml. Install rustfmt and clippy through
rustup and cargo-deny 0.20.2 through crates.io (`cargo install cargo-deny --version
0.20.2 --locked`). These are the only installed tools required for W0.1.

In the original task workspace, the isolated reusable installation is adjacent
to `extracted/`, under `fontpixel-rs-package/toolchain/`. From this repository:

```bash
source ../../toolchain/activate.sh
scripts/check.sh
```

The activation script changes `CARGO_HOME`, `RUSTUP_HOME`, and `PATH` only in the
current shell. No global shell startup file or user tool configuration is changed.
The installation came from static.rust-lang.org (rustup-init's published SHA-256
was verified), then official stable toolchain manifests. cargo-deny came from
crates.io with its locked dependency graph. A monobit 0.54.0 wheel was downloaded
from PyPI and verified against its published SHA-256 only to establish package
access; it was not installed or used as a font correctness check.

The isolated tool directory is not part of git or deliverable bundles. New
machines install their own toolchain from the pinned versions above. Package
downloads, build targets, and installation logs must stay out of deliveries.

Run `scripts/check.sh` for all mandatory local checks. Test counts differ when
serde is disabled because JSON serialization only exists behind that feature.
The CLI tests execute a real built binary, parse JSON, assert the 46-format list,
check errors/exit codes, and verify failed conversion leaves existing files intact.
Library contract tests use explicitly synthetic byte fixtures, including a real
sidecar-resource/multi-asset roundtrip. None substitutes for future font-codec
roundtrips, external crosschecks, full parity, performance benchmarks, or fuzzing.

When future tests execute supplied reference implementations, copy only what is
needed to a writable work directory or redirect every generated output. Never
write caches, `node_modules`, golden output, or build files under the original
`reference/` or `samples/` trees. Fixtures copied into this repository require
source, license, and purpose entries in `tests/data/SOURCES.md`.
