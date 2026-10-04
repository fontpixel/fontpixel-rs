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

The original complete api_quickstart.rs example has not been restored from this
worker's context. Do not substitute remembered pass counts for its source or new
feature-matrix verification.
