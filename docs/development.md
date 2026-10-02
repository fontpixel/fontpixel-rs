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

## Charset generation and independent parity (W0.2)

Python >= 3.12 is now required by `scripts/check.sh` to verify generated codec
tables; no Python packages are needed. Initial generation used CPython 3.12.14.
The checked-in registry and source make regeneration independent of the original
task package. CI selects Python 3.12; codec drift is an error requiring review.

For the stronger independent byte-for-byte comparison, pass the supplied
read-only Python checkout to `tools/check_charsets.py --reference PATH`, then
repeat with `--no-cjk`. It builds a release probe in Cargo's target directory and
writes nothing in the reference tree. `docs/charsets.md` describes exact coverage,
feature behavior, mapping sizes, and intentional per-glyph/stream boundaries.

## BDF validation (W1.2)

Ordinary `scripts/check.sh` runs without the supplied reference checkout: it
replays the committed oracle/real-font fixtures, seeded proptest, CLI and resource
checks. The Unicode printability generator uses Python 3.12's standard library.
Independent regeneration requires the unchanged sibling reference package:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 tools/gen_bdf_cases.py --check
cargo build --release -p fontpixel-cli
PYTHONDONTWRITEBYTECODE=1 python3 tests/crosscheck/bdf.py --monobit \
  --output tests/crosscheck/bdf-results.json
```

W1.2 installed monobit 0.54.0 and dependencies from PyPI in the isolated
`toolchain/python-packages` directory (add it to PYTHONPATH). The first acefile
wheel build needed an unavailable `clang`; retrying with the available official
system compiler (`CC=gcc`) succeeded. No source or system settings changed.
The crosscheck compares all encoded and unencoded glyph pixels and baseline/
advance metrics in both directions. Monobit may change unencoded glyph tags;
those external-tool differences are recorded, not attributed to fontpixel.

Nightly and cargo-fuzz are isolated in the same toolchain directory. See
`fuzz/README.md` for exact limits and commands. LeakSanitizer cannot perform its
exit-time ptrace check in this sandbox; fuzz runs disable leak detection, retaining
address sanitizer/coverage and the Rust panic/OOM checks. No kernel security
settings are changed. Do not count an infrastructure-failed run as a clean pass.
