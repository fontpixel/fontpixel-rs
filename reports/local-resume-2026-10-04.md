# Local restoration and baseline verification — 2026-10-04

The owner authorized importing the complete local handoff and resuming its
development sequence. All **27,469 delivered file hashes** matched before import
and again after copying. The original handoff directory is unchanged.

## Restoration

- Repository: `/home/chen/githubprojects/fontpixel/fontpixel-rs`.
- Restored accepted main: `9e6510d1a8f255776d22d119e03631afa6acd285`.
- All 52 refs match `../audit/refs.json`; all 1,587 main source/evidence files
  matched `../audit/accepted-source-files.json`. `git fsck --full` passed.
- Four clean draft worktrees restored at their documented commits: W3.2
  `8497b4c`, W3.3 `b7faf03`, W3.4 `0b7bd11`, W3.5 `c9ee2cf`.
- All references, samples, frozen validation inputs, original instructions,
  branch snapshots, bundle and supplemental evidence were copied into the
  specified sibling layout. No external remote URL was configured.
- New caches and results live in `../local-work/`. Each worktree has its own
  Cargo target. See [local instructions](../LOCAL-DEVELOPMENT.md).

## Local environment and baseline results

Linux x86_64, Intel Core i7-12700H, 20 logical CPUs, approximately 62 GiB RAM.
Rust/cargo 1.99.0, cargo-deny 0.20.2, cargo-fuzz 0.13.2; existing nightly
`1.99.0-nightly (1a833e165 2026-07-29)`. Python 3.12.13 is one patch release
earlier than the historical machine's 3.12.14. The installed oracle versions
match the handoff requirements; system FreeType is 2.13.2. No subagents were
started for this restoration/review. Service-tier/fast routing is unverified.

The following results apply to restored main `9e6510d`, before W3.2 integration:

| Check | Result |
| --- | --- |
| `scripts/check.sh` | Passed: generated tables, fmt, clippy, all feature matrices, deny; **976 passed test invocations**, not distinct tests |
| All-feature rustdoc with `-D warnings` | Passed |
| All-feature release CLI and parity runner | Built |
| Complete default Python differential suite | **3,292 cases / 58,839 operations**, 67 charset tables identical, zero new divergences |
| `tests/crosscheck/hex_psf.py` | Passed |
| `tests/crosscheck/console_review.py` | Eight independent review groups passed |

The seven historical K12 ragged-bitmap divergences remain explicitly classified;
the result is not a claim of zero total differences. Previous phase-2 timing and
all-format completion claims are not renewed by these baseline checks.

Machine-readable command/source/exit records and compressed logs are in
`local-resume-evidence/`; complete working output remains under
`../local-work/results/`. Protected input hashes are rechecked before delivery.

## Continuation

W3.2 is the first continuation item. Local review reproduced and repaired
cumulative FON source/report accounting, container property accounting, aligned
resource bounds, and lost omission diagnostics on fatal FNT conversion. Its
final acceptance and integration are recorded separately in `w3.2-review.md`
when complete; this restoration report alone does not accept that branch.

W3.3, W3.4 and W3.5 remain unaccepted drafts. Phase 3 must finish and pass its
aggregate gate before phase 4. The updated main PLAN and verbatim merge addendum
remain authoritative. Nothing was published, pushed or deployed.
