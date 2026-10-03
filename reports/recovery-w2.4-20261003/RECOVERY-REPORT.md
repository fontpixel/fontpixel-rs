# W2.4 recovery: buildable source checkpoint

2026-10-03 UTC. This is recovered source, not yet fresh W2.4 acceptance.

Branch recovery/w2.4-20261003 starts from fully validated W2.1–W2.3 recovery e5a4d298. The original archived main/W2.1 branches remain untouched. This is new history; original W2.4 Git objects 113539d, ece15ea, e08d69c and final byte fingerprints are unavailable.

The full initial bitmap/color/native-codec/example/test/fuzz/Python-driver snapshots were combined with the retained later changes. All 68 audited replacements matched. Two textual anchors were adapted only for whitespace in the pre-rustfmt snapshots. Ledger blocks were copied verbatim, then formatted with Rust 1.99.0. The existing CLI capability assertion update was recorded only semantically, so its equivalent test code was reconstructed and is not claimed as original text. No new feature or design was introduced.

Fresh all-feature workspace libraries, binaries and examples build: exit 0 (39.39 seconds). Cargo regenerated missing lockfile additions from crates.io, resolving the same recorded versions: png 0.18.1, fdeflate 0.3.7, miniz_oxide 0.8.9 and optional zlib-rs 0.6.8; existing flate2 remains 1.1.10 and its miniz_oxide remains 0.9.1. These are registry-verified regenerated locks, not recovered original lockfile blobs.

The old 37 generated fixture binaries and their original hash manifest, old raw logs and full external result JSON are missing. The original final-code aggregate/fuzz runs had unknown outcomes after rollback. Historical observations are kept separately and do not satisfy new acceptance. Fixtures will be regenerated from the recovered independent generator and explicitly labelled regenerated; checks, complete parity, external comparisons, feature matrix and final-reader robustness runs are pending.

No reference/ or samples/ files were modified. The previously saved W2.1–W2.3 checkpoint remains independently available. This all-branches bundle includes the old branches plus the W2.1–W2.3 and W2.4 recovery branches. Select recovery/w2.4-20261003 to continue this item.

Requested model/effort/tier: gpt-6-astra, xhigh, fast. Actual model and tier cannot be verified.
