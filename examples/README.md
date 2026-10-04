# Documentation examples

These files were restored during phase-6 recovery. The current reconstructed
project is uncompiled/untested; no example or project command was executed during
this restoration. Historical passes are not current acceptance. See
[recovery status](../docs/recovery-status.md).

- `batch.toml`: three small conversion/validation tasks. Inputs are relative to
  the TOML; create its documented output directory before a later authorized run.
- `tong_batch.py`: emits a new TOML exclusively, checks source existence and
  preserves deterministic regional order. It does not run conversions, create
  output directories, delete stale files or publish. Its default84 tasks and
  format/size switches are documented in the Tong migration guide.
- `check_documentation.py`: inventories the99 existing format documents,
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
