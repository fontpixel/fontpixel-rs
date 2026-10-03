# fontpixel W2.1–W2.3 recovery checkpoint

Date: 2026-10-03 UTC. Status: source integration builds; fresh validation pending.

## What is restored

- Original archived baseline remains available: main caaeecd and w2.1-sfnt 76ab7eb. Neither was overwritten.
- Recovery branch: recovery/w2.1-w2.3-20261003. New commits reconstruct content; they are not the lost original Git objects or a byte-identical recreation of history.
- 4f1bfd6 restores the 12 surviving W2.1 acceptance documentation/evidence files. The historical acceptance commit changed no production source or tests. Ten original evidence files remain missing, enumerated beside this report.
- 34d8482 restores W2.2 from initial exact source-writing payloads, all recorded sequential edits, and Rust 1.99.0 formatting between edit stages. All 67 audited string replacements matched; no ambiguous or skipped edit was encountered.
- bb31cf4 restores W2.3 CFF sources, tests, example, crosschecker, fuzz target and docs, plus the recorded feature/module/append-only integration edits.
- W2.4 is deliberately not included in this checkpoint.

## Fresh verification at this checkpoint

- Rust 1.99.0 installed from the official toolchain source into the writable workspace. Its default home installation initially failed because the home directory is read-only; no permissions were changed.
- cargo build --locked --workspace --all-features --lib --bins --examples: PASS (32.21 s). This compiles libraries, binaries and examples, not missing-fixture integration tests.
- Source formatting was performed by the pinned Rust 1.99.0 toolchain. Cargo.lock and fuzz/Cargo.lock were unchanged.
- Full scripts/check.sh, parity, external crosschecks and fuzz validation have NOT yet been rerun. Historical reports and evidence retain their pre-rollback results; they do not assert new acceptance.

## Remaining recovery gaps

The old Git objects, final original source-byte hashes and complete logs cannot be recovered from the salvaged material. W2.2/W2.3 sources are reconstructed from authored content plus recorded changes, not authenticated against unavailable final blobs.

Six tiny CFF fixtures and their original SOURCES.json hash manifest are still absent. The original deterministic recipe survives and is authorized for fresh regeneration; resulting bytes will be labelled regenerated, not recovered original bytes. Full raw W2.1–W2.3 acceptance logs and crosscheck result sets cannot be fabricated. Fresh verification will be stored separately here.

## Protected data and continuation

The original reference/ and samples/ trees are read-only inputs. A 690-file SHA-256 baseline is retained in protected-before.json and will be checked after tests. The salvage directories were never edited.

Restore with git clone fontpixel-rs-all-branches.bundle fontpixel-rs, then select recovery/w2.1-w2.3-20261003. The bundle includes every restored local/remote branch plus the new recovery branch and its full reachable history.

Requested worker configuration: gpt-6-astra, xhigh, fast. Actual model/tier cannot be verified from this execution environment.
