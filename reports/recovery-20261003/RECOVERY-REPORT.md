# fontpixel W2.1–W2.3 recovery checkpoint

Date: 2026-10-03 UTC. Status: source integration and all completed validation pass; full-SC CFF comparison still running.

## What is restored

- Original archived baseline remains available: main caaeecd and w2.1-sfnt 76ab7eb. Neither was overwritten.
- Recovery branch: recovery/w2.1-w2.3-20261003. New commits reconstruct content; they are not the lost original Git objects or a byte-identical recreation of history.
- 4f1bfd6 restores the 12 surviving W2.1 acceptance documentation/evidence files. The historical acceptance commit changed no production source or tests. Ten original evidence files remain missing, enumerated beside this report.
- 34d8482 restores W2.2 from initial exact source-writing payloads, all recorded sequential edits, and Rust 1.99.0 formatting between edit stages. All 67 audited string replacements matched; no ambiguous or skipped edit was encountered.
- bb31cf4 restores W2.3 CFF sources, tests, example, crosschecker, fuzz target and docs, plus the recorded feature/module/append-only integration edits.
- W2.4 is deliberately not included in this checkpoint.

## Historical production-source fingerprints match

Using the exact parity source-fingerprint algorithm, recovered W2.2 is `63c4972f231fa4b4` and recovered W2.3 is `873ec98e9affe795`, both identical to their retained historical observations. The complete fresh parity run also records W2.3 `873ec98e9affe795` at start/end with clean crate sources.

These are 16-hex truncated SHA-256 fingerprints over `crates/fontpixel/src/**/*.rs` and `crates/fontpixel-cli/src/parity.rs`, including relative pathnames and source bytes. They provide strong corroboration for exact production-source recovery in that scope. They do not authenticate all tests/docs/fuzz inputs or recover missing Git objects and evidence.

All 67 recorded W2.2 replacements matched their expected anchors; no patch was skipped or invented. Every W2.3 Rust production/test/example file equals the Rust 1.99.0-formatted recovered payload. Its separate fuzz target preserves the raw recovered payload.

An initial diagnostic incorrectly sorted Git pathnames as strings rather than sorting Path objects componentwise, and therefore reported a mismatch. This was corrected by comparison with the actual parity harness before final delivery. The corrected comparison is in source-fingerprint-comparison.json.

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

## Validation progress checkpoint, 2026-10-03 06:18 UTC

Fresh scripts/check.sh passed all 546 aggregate tests. Complete default parity passed with zero NEW divergences. Default sfnt/glyf/CFF external suites passed; glyf totals 16 cases / 30,091 glyph checks, CFF totals 20 / 30,094. Full-SC glyf passed 4 / 77,317. Strict rustdoc, sfnt-only tests, sfnt generator checks and fuzz dependency checks passed. All four restored-reader robustness runs completed 611 seconds with ASAN and exit 0: sfnt 6,124,739; glyf 6,132,071; compatibility TTF 314,472; CFF 18,109,323 executions.

Six CFF fixtures are now regenerated and explicitly labelled as newly generated bytes, with a manifest and synthetic crosscheck results. The full-SC CFF run is still active, after square passed and dot entered streamed native reads. It is not counted as completed in this checkpoint. Complete evidence for finished checks is committed here; the ongoing CFF full-SC log is omitted until complete. No new production changes were needed.
