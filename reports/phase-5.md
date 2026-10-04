# Phase 5 acceptance

The accepted baseline is phase 4 main `5c79f81bb52b44352a885e2dd79cdbe02ae3019c`.
Implementation is integrated and **phase 5 is accepted** in the existing main
checkout. Phases 6–9 have not started. The local task stops here; the user has
authorized an online dot handoff for phase 6 only.

The W5.1 survey is complete as a survey: 53 additional canonical IDs, 99 planned
IDs in total, plus explicit extensions to existing families. Its offline,
installed-oracle and pinned-upstream checks pass. The survey is separate evidence from the implementations and validation below.
See [the survey](w5.1-survey.md).

All six worker groups have frozen their implementations and returned ownership
to the coordinator. Their final source manifests verify **459 unique files
without differences** across 18 owner proofs. The final implementation is
committed at `81b577b` and integrated into main at `f64b729`. The merged main
gate passes with the same 2,480 test executions / 597 suites. Formal five-run
release timing and its complete raw-log verification now also pass.

| Group | Completed scope | Evidence |
| --- | --- | --- |
| W5.1 / A1 / X / outline | Survey; eight legacy formats; packed raw/BMFont JSON; SSFN/PFR | `w5.1-survey.md`, `w5.a1-freeze.json`, `w5.x-freeze.json`, `w5.e2-outline-freeze.json` |
| W5.2 / D | Johab, Apple IIgs; Fbit/Lisa/MGTK/Qt and BinHex | `w5.2/freeze.json`, `w5.d/freeze.json` |
| W5.3 / C1 / C2 | vfont/FZX/FIGlet; six printer formats; seven workstation/printer families | `w5.3-freeze.json`, `w5.c1-freeze.json`, `w5.c2-freeze.json` |
| W5.4 / B | Palm/GDR; eight system families | `w5.4-freeze.json`, `w5.b-freeze-final4.json` |
| W5.5 / A2 | DRCS/C embedded families; eight console/loader formats | `w5.5-sha256.txt`, `w5.a2-sha256.txt`, `w5.fon-detection.sha256` |
| W5.6 / E1 / E2 image | CBDT/sbix writing; six devices; PILfont/SFont/VLW/imageset | `w5.6-color/source-hashes.json`, `w5.e1/source-hashes.json`, `w5.e2-image/source-hashes.json` |

All **99 catalogue IDs** have connected native reader and writer entry points,
with 89 detectors; headerless formats require explicit selection. This inventory
does not promise every historical variant. Native profiles, unsupported cases,
read projections and loss policies remain in `docs/formats/` and owner reports.

First long ASAN evidence is archived in
[`phase5-validation/initial-long-fuzz.json`](phase5-validation/initial-long-fuzz.json):
vfont 1,477,611 runs, FZX 1,853,831, FIGlet 572,703, color 334,887 and
embedded OpenType/Apple bitmap readers/writers 6,910,245; each ran 611 seconds,
exit 0, with peak ASAN RSS 381–509 MiB. The initial frozen module source and
harness hashes match their evidence; subsequent shared API and disabled-feature
guard changes are disclosed in the final source audit below.

Additional completed campaigns are archived in
[`phase5-validation/native-long-fuzz.json`](phase5-validation/native-long-fuzz.json).
Earlier Johab/IIgs runs remain as superseded evidence because final3 changes the
strict-mode audit; new final3 campaigns ran 612/611 seconds respectively without
errors. E1, A2, X and D campaigns have completed on their verified owned-file
freeze manifests. The cumulative [long-fuzz index](phase5-validation/long-fuzz-index.json)
contains 71 passing current campaigns and five superseded campaigns.
The current campaigns total 319,144,269 executions, each completing at least
611 seconds as reported by libFuzzer. All C2, public transport, corrected
C/FON/GPI/SymbOS and image/outline campaigns have passed. These are module-source
evidence; shared integration changes are audited separately.

The final [source audit](phase5-validation/fuzz-source-audit.md) verifies all
142 compressed current-campaign evidence files and all 229 distinct target,
codec and dedicated-helper paths. Earlier shared helper/model/registry changes
and missing historical SHA entries are listed explicitly. In particular,
`model/budget.rs` was absent from the original fuzz snapshot dictionaries; this
is not evidence that it was absent from compilation. Its final SHA is covered
by the later 718-file aggregate gates. The public transport campaign matches
the final resources/transport/registry and RAW/BDF/SSFN source scope. These
different scopes are not presented as one whole-tree long-fuzz run.

The final candidate and merged-main `scripts/check.sh` each pass with **2,480 test executions in
597 suites**, including repeated feature configurations. Additional all-feature
rustdoc with warnings denied, minimal BDF/raw CLI Clippy and fuzz-workspace
cargo-deny gates pass. The gates recorded **718 tracked source hashes**; all 717 other paths still
match. A later benchmark-only log-path repair is separately checked with
22 regressions and a fresh preparation/measurement. No production, adapter or
seed source changed after the Rust gates or release build. Commands and complete compressed logs are in
`phase5-validation/`; `aggregate-source-verification.json` records the match.
The earlier 2,449-execution preview is retained as historical evidence.

The latest all-feature CLI run passes all test suites, including 27 general
command cases, seven cumulative input-budget regressions, six P2 cases and
19 P3 native variant/resource/pixel cases. These include PXL TFM and Daisy
fragments, actual Consoleet/image directories, every Prebuilt depth and integer
byte order with both explicit pixel orders, both PFR dialects and native
printer/workstation profiles.
System/device CLI cases continue to expose and verify native container boundaries.
NFNT's CLI container matrix now includes BinHex and passes. Native directory
Input/OutputBundle support has 24 passing shared
contract tests and eight CLI filesystem tests, including rollback, scoped
listing, budget enforcement, rejection of stale output-directory members and a
2,048-input descriptor regression. CLI directory image tests cover PNG/BMP/GIF/
TGA/PBM/PGM, black/white/alpha backgrounds and unchanged output on strict errors. The shared
legacy report audit uses a budgeted sorted alias index to avoid quadratic work;
its regression and the original W5.3 tests pass. Minimal CLI all-targets Clippy
passes, including the latest no-default-features BDF/raw build. Quiet release
performance is complete with v3 after the v2 raw-log archive rejection. The stripped Linux x86-64 all-feature CLI is
13,103,232 bytes (12.50 MiB), or 5,634,032 bytes (5.37 MiB) with deterministic
gzip. `phase5-validation/release-artifacts.json` records exact hashes and checks.
This is the phase 5 size, not a future cross-platform release guarantee.

The independent detector review replays 189 original fixture cases and 75
original CLI checks. RISC OS/FONTX2, ChiWriter/QPF, Windows FON/OS/2 GPI,
C comments/HEX and Xerox/Palm/NFNT false matches are corrected; legitimate
PNG/image/SFont and Palm/NFNT ambiguities still require explicit selection.
Forty-one previously successful CLI JSON outputs remain byte-identical. See
[the final review](w5.detector-review-final.md).

Known native boundaries are collected in
[the scope review](w5.scope-limitations-review.md): Mono/Gray8 coverage rather
than arbitrary RGBA; embedded-bitmap SSFN/PFR rather than outline rendering;
explicit native field limits and variants; incomplete historical Printshop,
Catalyst/Lisa/HFNT documentation; and external-consumer qualifications. Not
every profile has an original operating-system/application runtime oracle.
These limits are reported, not normalized into claims of universal compatibility.
The [scaling review](w5.scaling-review.md) documents the fixed GPI allocation
budget bug and the remaining bounded PFR shared-physical-table reparse case.

The 690 protected files under `../reference/` and `../samples/` remain unchanged.
Decisions recorded this phase include native directories and stale-member
rejection, cumulative input/transport budgets, explicit numeric/pixel orders,
real structural detection, and the GPI preallocation check. Per-format reports
retain exact provenance, independent commands and native-profile restrictions.

Worker requests use `gpt-6-astra`, `xhigh`, `fork_turns=none` per the supplied
worker configuration. Actual backend identity and fast service mode cannot be
independently verified. No remote push or external site deployment is part of
this work.

The user's latest instruction is to finish phase 5 and stop. `TODO.md` records
the stop point and the unstarted phases 6–9. No phase 6 implementation is begun.


## Accepted release performance

The corrected recorder completed **185 cases, 301 engine/format records and five
independent process repetitions**. Its 1,445 command executions retain 2,890
unique stdout/stderr files; all raw hashes pass. An independent archive pass
recomputes every median/minimum/maximum from the five observations and checks
every referenced raw stream SHA. There is no source/fixture/binary/oracle drift
or competing task-owned build/fuzz/consumer process before or after timing.

The machine is an Intel Core i7-12700H, 14 physical / 20 logical CPUs,
65,503,764 KiB RAM, Rust 1.99.0, release/thin-LTO/one-codegen-unit settings.
Unrelated desktop applications remained active; this is a shared workstation.
Input bytes are preloaded. RSS includes runtime/setup/model memory, and native
writer audits remain inside the measured operation. Native/Python/Java/FreeType
scope differences and unavailable baselines remain explicit.

All measurements and ranges are in [the complete benchmark report](../benches/PHASE5-REPORT.md).
The [summary](phase5-validation/performance-summary.json) links compressed raw
results, the final preparation identity and every retained stream. These small
native-profile workloads do not satisfy the later complete Tong/catalogue/merge
or binding performance targets by themselves; those remain stages 6–8 work.

During independent archive verification, v2's `Path.with_suffix` handling was
found to collapse extension-bearing case names and overwrite earlier logs.
Fifteen cases shared 13 stdout/13 stderr paths, with 82 mismatching retained
stream references. V2 is explicitly rejected despite its original `formal:true`.
All original evidence remains in `phase5-validation/superseded/performance-v2/`.
V3 appends suffixes, refuses existing files and checks all raw hashes before
publishing a result. Twenty-two checks and a full smoke precede the new complete
measurement. This correction changes benchmark orchestration only; production
codecs, native adapters and seeds remain the versions accepted by the Rust gates.

## Delivery and stop point

`TODO.md` records completed work, evidence boundaries and all unstarted phases.
The online handoff includes the full all-ref Git bundle, immutable references
and samples, full phase-2 Tong inputs, restoration/checksum scripts and the
six-worker phase-6 prompt. Exact snapshot/ZIP identities and independent restore
verification are in the handoff's external `delivery-manifest.json`.

The local coordinator does not begin phase 6. The next coordinator follows
`docs/ONLINE-DOT-PHASE6-PROMPT.md`, completes only W6.1–W6.6 with six requested
Astra/xhigh/fast workers, and reports actual model/service-mode availability.
