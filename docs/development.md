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

## YAFF validation (W1.6)

YAFF's default feature enables serde/serde_json and the existing gzip transport;
`--no-default-features --features bdf,pcf` remains free of JSON/serde production
dependencies. The ordinary matrix runs YAFF syntax/editing/native-model tests,
the checked-in canonical golden, 200-case proptest, and CLI multi-font gzip tests.
For a standalone build, also run:

```sh
cargo test --locked -p fontpixel --no-default-features --features yaff
cargo build --locked -p fontpixel-cli --release --features parity
PYTHONPATH=../../toolchain/python-packages python3 tests/crosscheck/yaff.py \
  --output tests/crosscheck/yaff-results.json
python3 -B tests/parity/run_all.py --out tests/parity/out-w1.6
cargo run --release -p fontpixel --example yaff_timing -- \
  ../samples/tong-pixel-sans/TongPixelSans14Large-SC.bdf
```

Timing excludes filesystem reads and BDF parse, and verifies exact native model
roundtrip on each of five iterations. The complete default W1.5 differential gate
is rerun unchanged, with matching start/end source fingerprints. See
`reports/w1.6.md`, `tests/crosscheck/yaff-*-results.json` and `fuzz/README.md`.

## sfnt common-table validation (W2.1)

The public `sfnt` feature provides borrowed envelope parsing, deterministic
assembly and common-table APIs; it does not enable a native TTF/OTF/OTB codec.
The normal all-feature matrix exercises its 29 integration/property tests. A
standalone build and the independent oracles can also be run explicitly:

```sh
cargo test --locked -p fontpixel --no-default-features --features sfnt --test sfnt
cargo build --locked -p fontpixel --features compat-bdfparser --example sfnt_check
PYTHONPATH=../../toolchain/python-packages python3 -B tools/gen_sfnt_data.py --check
PYTHONPATH=../../toolchain/python-packages python3 -B tests/crosscheck/sfnt.py \
  --output tests/crosscheck/sfnt-results.json
cargo build --locked -p fontpixel-cli --release --features parity --bin fontpixel-parity
python3 -B tests/parity/run_all.py --out tests/parity/out-w21
```

The external script requires fontTools, OTS, FreeType and HarfBuzz and fails if
any is absent. It writes temporary generated fonts and edits, not reference
files. W2.1's shaping/rendering cases concern the common tables and reuse the
already-accepted compatibility outlines; full native outline, bitmap-strike,
CFF, `locl` and Tong family checks remain later phase-2 work.

Keep source files fixed while running parity and robustness acceptance; record
matching start/end fingerprints and the process exit status. An interrupted log
or a parity warning about changed sources is not a final pass. See
`fuzz/README.md` for the documented local sfnt target and limits and
`reports/w2.1.md` for the verified results and known scope boundaries.

## HEX / PSF validation (W3.1)

`hex` and `psf` are independent library features; the CLI enables both by default.
No production dependency is added (`psf` uses the existing gzip transport).
The focused test binary requires both features and runs 19 tests including 400
proptest cases. Use the complete matrix as usual, plus individual feature builds:

```sh
cargo test --locked -p fontpixel --no-default-features --features hex,psf --test hex_psf
cargo clippy --locked -p fontpixel --no-default-features --features hex -- -D warnings
cargo clippy --locked -p fontpixel --no-default-features --features psf -- -D warnings
cargo build --locked --release -p fontpixel-cli --bin fontpixel
PYTHONDONTWRITEBYTECODE=1 python3 tests/crosscheck/hex_psf.py \
  --output tests/crosscheck/hex-psf-results.json
cargo run --locked --release -p fontpixel --no-default-features \
  --features hex,psf --example console_timing
```

The crosscheck requires monobit 0.54.0 and the read-only supplied catalogue
reference. It compares both directions for the oracle's real supported subset,
records its extended-HEX and PSF-sequence shortcomings, independently validates
complete kbd wire grammar, and calls the actual BDF catalogue importer after new
format conversions. There is no direct HEX/PSF importer to claim as tested.
All inputs for these tests are generated; no external font is copied into Git.

On storage-constrained workers, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`, and `CARGO_BUILD_JOBS=4`
reduce disposable build-cache size. They change neither test coverage nor
release settings. An interrupted/full-disk run is never counted as passing.
