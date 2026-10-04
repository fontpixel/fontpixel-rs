# Documentation examples

The recovered files were handed over as uncompiled/untested source; no example or
project command was executed during that documentation restoration. Later local
validation is recorded separately by the coordinator. Historical passes are not
current acceptance. See
[recovery status](../docs/recovery-status.md).

- `batch.toml`: three small conversion/validation tasks. Inputs are relative to
  the TOML; create its documented output directory before a later authorized run.
- `tong_batch.py`: emits a new TOML exclusively, checks source existence and
  preserves deterministic regional order. It does not run conversions, create
  output directories, delete stale files or publish. Its default 84 tasks and
  format/size switches are documented in the Tong migration guide.
- `check_documentation.py`: inventories the 99 existing format documents,
  13 command examples, manifest keys and generator task matrix. With --binary it
  executes that CLI's help and registry queries. It must not be run while the
  recovery execution gate is closed; source-inventory checks are not runtime
  font equivalence tests.

The original complete api_quickstart.rs was not recovered byte-for-byte. A new
[api_quickstart.rs](../crates/fontpixel/examples/api_quickstart.rs) is now written
against current public declarations, with common model/name/render/charset/merge
checks and feature-gated codecs, sidecars, embedded metrics, variants/subset and
WOFF2. It writes no files and prints all conversion reports. It is uncompiled and
untested; remembered passes apply only to the lost original example.

`check_api_documentation.py` uses Python's standard library to inspect source
declarations, feature names, local links and rustdoc linkage. It neither imports
project modules nor runs Cargo, the CLI, Rust examples or a font conversion. Its
source-reference checks are lexical checks, not a Rust type checker or native
acceptance test. `--require-rustdoc-link` makes missing crate-root inclusion an
error; without it the result reports any pending linkage explicitly.

## Recording CLI smokes

[replay_documentation_smokes.py](replay_documentation_smokes.py) runs the
18 argument sequences in [documentation-smoke-cases.json](documentation-smoke-cases.json)
and the complete 84-task tiny Tong job. The historical Han/geometric fixture was
lost; the runner creates a new deterministic CC0 two-glyph BDF and records its
provenance and hash. Tiny Tong uses twenty copies of `tests/data/compat/mini.bdf`.
It checks task IDs/order/options, complete success reports, commit counts, output
signatures, additional glyph/model readbacks, and all 13 command help spellings.
Each command, including generation and help, has separate stdout/stderr, status
and hashes. Failures and timeouts leave evidence, including a failed final result.

The coordinator supplies an immutable all-feature CLI and its actual build-source
manifest, including dirty and nonignored untracked files. The runner never builds
or installs anything. The JSON manifest schema is `{"head": "COMMIT", "files":
{"relative/path": {"bytes": 123, "sha256": "HASH"}}}`; it must cover the exact
`git ls-files --cached --others --exclude-standard` file set. Both source and
binary identities are checked before and after execution. Existing evidence
directories are refused; output must be below sibling `local-work`.

After the coordinator opens the validation window, use actual identity values:

```sh
python3 -B examples/replay_documentation_smokes.py \
  --binary /absolute/immutable/fontpixel \
  --expected-binary-sha256 ACTUAL_BINARY_SHA256 \
  --expected-source-commit ACTUAL_SOURCE_COMMIT \
  --source-manifest /absolute/build/source-after.json \
  --evidence ../local-work/phase6-local-20261004/docs-smoke-UNIQUE_RUN
```

The runner is new and native execution remains pending. It supplements
`python3 -B examples/check_documentation.py --binary ...`, whose font-related
runtime work is only help and registry queries. The 99-document inventory checks
presence, size and index links; semantic profile review remains separate.
Small identical fixtures do not prove full Tong/catalogue equivalence, distinct
regional `locl` behavior, >=1 MiB report spooling, negative/transaction/resource
contracts or performance. BDF tiny tasks are conversions, not source byte copies.

The recorder's own failure/identity guards can be tested without a native binary:

```sh
python3 -B examples/replay_documentation_smokes_test.py
```

## API example feature coverage

Run the filesystem-free quickstart under no defaults, normal defaults and all
features. Also run `--no-default-features --features FEATURE` separately for each
of `yaff`, `pcf`, `image`, `otb`, `sfnt` and `woff2`. For example:

```sh
cargo run --locked -p fontpixel --no-default-features --features sfnt --example api_quickstart
cargo clippy --locked -p fontpixel --no-default-features --features sfnt --example api_quickstart -- -D warnings
```

These nine profiles exercise core-only behavior and all six guarded branches.
OTB implies sfnt; WOFF2 implies vector-raster/CFF/sfnt. All profiles also exercise
model/names, atomic rejection, rendering/charset and merge. A default run includes
PCF/YAFF but no BDF-specific example branch. Run warning-free rustdoc and doctests
separately; static cfg/reference checks do not execute these examples. Cargo
commands belong to the single coordinator build slot.
