# Phase 2 accepted integration

The candidate and independent review fixes are integrated into main. The current
acceptance report is [phase-2.md](phase-2.md), with final integrated evidence in
`phase2-integration/`. The guarded default exporter measured 38.414 s median,
2.869 GiB peak RSS, and all 84 files match the independently gated output set.
No Phase 3 work has begun.

The chronological checkpoints below are retained for history and do not override
the accepted-stage report.

# Phase 2 acceptance candidate progress

2026-10-03 UTC: the unmerged `phase2-acceptance` candidate has completed the
full-source baseline, production performance work, a 912-pass aggregate check,
warning-free documentation and final-code default parity with 0 new divergences.
All six SVG outputs remain byte-identical to their accepted full oracles and all
690 protected input files remain unchanged. Native complete-family quality-11
export has a five-run median of 38.233 s, meeting the 40-second target, with
2.905 GiB peak RSS and 84 byte-identical outputs in every run. The independently
guarded exporter subsequently passed a 38.414 s five-run median at 2.869 GiB RSS
with all 84 outputs exactly identical.

All final full-font, transport, subset, raster and fresh ASan gates now pass.
The isolated original-Tong and stock-fontTools comparisons also completed.
An independent review confirmed input/output alias truncation in the new example;
its guard and a 32-bit arithmetic fix are validated at `7b7ed29` on
`review/phase2-input-safety`, with 928 passing check executions and eight release
path/rerun cases. The earlier measured
production source is `df83a08`, and main remains reviewed W2.8 `54c7f29`.
This is not a Phase 2 acceptance or merge. See `phase-2-candidate.md` for terminal
evidence and the review/integration boundary. No W3 work has begun.

## Earlier reviewed main status

2026-10-03 UTC: W2.1–W2.8 are accepted and integrated into main. W2.8's
independent review corrected separate-path paint, aliases, metadata/resource
handling and CLI input capacity. Integration merge:
`e82baf5268cbafdb54c69da363cda52497efa8f9`.

Integrated full check: **884 passing test invocations**. Rebuilt-release full
parity: **0 NEW divergences**, with stable clean source fingerprints. W2.8's
fresh reader ASan gate and complete external SVG oracles also pass. See
`reports/w2.8-review.md` and `reports/w2.8-review/` for exact evidence.

**Phase 2 is still incomplete.** The next separate task is the combined
acceptance gate: PLAN §9.3 external-container checks, §9.4 complete Tong export
and locl equivalence, and §10 whole-export five-run timing/size comparisons on
final integrated main. Do not begin W3 until the combined gate passes and the
phase-2 report is written. W2.8 integration alone does not establish that gate.

## Earlier checkpoint history

The following statements describe earlier checkpoints and do not override the
current status above.

### Earlier W2.1–W2.4 integration checkpoint

2026-10-03 UTC: W2.1–W2.4 have been recovered, independently validated,
accepted and fast-forward integrated into main. The integrated-main full
aggregate check passes (561 test executions). W2.5 WOFF1/WOFF2 is next;
phase 2 remains incomplete. See `main-integration-20261003/INTEGRATION-REPORT.md`
for current integration status and `recovery-w2.4-20261003/` for acceptance evidence.



# Phase 2 interrupted progress / 第2阶段未完成进度

2026-10-03 UTC.

Phase1 remains complete at caaeecd; see delivered phase1 report.
W2.1 sfnt common layer has source changes on branch w2.1-sfnt. These are an unfinished checkpoint, not accepted completion.
The worker execution returned a possible cybersecurity-risk safety flag twice, including during an attempt to obtain exact status. No exact denied tool operation was available. The coordinator stopped that execution and did not retry blocked actions.
Earlier worker reports stated focused tests, external fontTools/OTS/FreeType/HarfBuzz checks and default parity passed, and a local robustness regression had been fixed. Those final reports were not independently completed/accepted; remaining final checks must be reviewed before resuming.
W2.2–W2.8 have not begun. Native format status remains as of phase1 until acceptance.
This archive includes all Git branches and current W2.1 source checkpoint. reference/ and samples/ remain in the original task package and are not redistributed here.
Requested worker: gpt-6-astra, xhigh, fork none; fast requested but interface cannot enable or verify.

恢复：解压并读取 README-RESTORE.txt。请保留原任务包，在 w2.1-sfnt 分支检查未完成工作，不要把本包当成第2阶段完成包。

## 2026-10-03 recovery verification

The W2.1 production source at `76ab7eb` has now passed a fresh complete
`scripts/check.sh`, 29 standalone sfnt tests, warning-free rustdoc, all applicable
fontTools/OTS/FreeType/HarfBuzz common-table checks, unchanged-source full default
parity (0 new differences), and one complete 611-second sfnt robustness run
(11,865,370 executions, exit 0). The 690 protected reference/sample files match.
See `reports/w2.1.md` and `reports/evidence/w2.1/` for exact scope and evidence.
Earlier interrupted runs are historical only, not counted as successful runs.
The work branch is ready for coordinator review/merge. Phase 2 is still
incomplete; W2.2–W2.8 have not started in this recovery task, and native format
registration is unchanged from phase 1.
