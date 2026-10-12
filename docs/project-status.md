# Project status and history

This text was the top of the repository README until the 0.1.0 release
preparation (2026-10-12). The phase reports in `reports/` and TODO.md hold the
complete history.

## Phase 6 delivery (2026-10-05)

Phase 6 (CLI, batch, documentation, benchmarks, robustness, merge) is complete;
see the [phase 6 report](../reports/phase-6.md). `scripts/check.sh`, warning-free
rustdoc, full Tong/catalogue/SC/merge/legacy-parity comparisons against frozen
release binaries, a 104-target ASAN fuzz campaign and the formal PLAN §10
benchmarks ([benches/REPORT.md](../benches/REPORT.md)) all passed. The phase-6
source was reconstructed after the earlier workspace was lost; the
[recovery status](recovery-status.md) explains which historical results
predate this source.

The catalogue contains the same 99 IDs and documented profiles. Differences
between catalogue downloads and the original Python pipeline are documented in
[catalogue download differences](migration-fontpixel.md#catalogue-download-differences).

After phase-6 delivery, work is authorized through phase 7 then phase 8, with
full native Python and WASM/JS APIs plus separate legacy compatibility APIs.
Phase 9, remote push, publication and deployment are excluded. Blocked long ASAN
validation remains explicitly deferred to the user's local computer. This scope
change is not phase-6 acceptance. Root-owned [TODO.md](../TODO.md) tracks integration.

The baseline functionality and historical evidence below are preserved as context;
they are not fresh tests of the reconstructed changes.

