# Accepted W2.1–W2.4 integration into main

Date: 2026-10-03 UTC. Coordinator integration and fresh aggregate validation pass.

## Integration

The accepted recovery chain at `b59747ca046af00a70adfa9a1ed51315dffeffa2` was a strict descendant of original
main `caaeecd7ac1a144aa5819fe10ecbba312cf4e650`. A new `fontpixel-rs-main` worktree checked out main, then
`git merge --ff-only recovery/w2.4-20261003` completed without conflict or forced
reference update. No production source, test, configuration, dependency or fixture
changed during integration. The subsequent integration commit contains reports
and fresh check evidence only.

All 35 pre-existing non-main refs remain at their original objects.
The old main remains available through `progress/main` and `origin/main`.
Original `w2.1-sfnt`, both recovery branches and all historical evidence are
retained. No network push was made.

## Verification on integrated main

- Full `bash scripts/check.sh`: exit 0, 561 aggregate test executions.
- Generator checks, rustfmt, warning-free Clippy, full/default/minimal/feature
  test matrices and the production dependency audit all pass.
- Source, tests, tooling, dependency files and fuzz configuration equal the
  accepted recovery source byte-for-byte.
- All 690 protected reference/sample files remain identical.
- The two accepted W2.4 fuzz logs were reverified using terminal DONE markers,
  exit files and checksums: embedded_bitmap 6,587,559 executions and color_bitmap
  10,270,436 executions, each 611 seconds and exit 0. A transient transport
  disconnection is not being used as evidence of completion.

Earlier full recovery acceptance remains in `reports/recovery-20261003/` and
`reports/recovery-w2.4-20261003/`: 0 new parity divergences; W2.4 independent
crosschecks 140 cases / 380,808 glyph comparisons; all 37 regenerated W2.4 binary
fixtures repeated byte-identically. Those crosschecks were already completed on
the identical source and are not represented as rerun during this integration.

The new exact-version fuzz duplicate allowance remains separately recorded at
`cbcbed4`; original Git objects and unavailable historical evidence remain
unrecovered. Regenerated fixture bytes retain their explicit provenance labels.
Read the recovery reports for tool limitations and unsupported format scope.

## Continue

W2.1–W2.4 are accepted and now integrated into main. W2.5 WOFF1/WOFF2 is next;
phase 2 as a whole is still incomplete. Use main from this updated all-branches
bundle or the clean `fontpixel-rs-main` worktree. Create the next work item's
own branch/worktree from main; preserve reference/ and samples/ as read-only.
Exact local setup and test commands are in COMMANDS.txt beside this report.

The earlier reports are immutable historical checkpoints and may describe main
as unmerged at the time they were written. This dated integration report
supersedes those status statements. Requested gpt-6-astra/xhigh/fast execution
configuration remains unverified by the available interface.
