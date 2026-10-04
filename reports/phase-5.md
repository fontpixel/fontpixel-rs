# Phase 5 work in progress

The accepted baseline is phase 4 main `5c79f81bb52b44352a885e2dd79cdbe02ae3019c`.
Work continues on `phase5/p3` in the existing shared checkout. **Phase 5 is not
accepted, merged or complete; phases 6–9 have not started.**

The W5.1 survey is complete as a survey: 53 additional canonical IDs, 99 planned
IDs in total, plus explicit extensions to existing families. Its offline,
installed-oracle and pinned-upstream checks pass. This does not establish any
new codec's implementation or acceptance. See [the survey](w5.1-survey.md).

Six subagents work concurrently, with module ownership and coordinator-owned
registry, manifests, CLI, integration and final acceptance:

| Worker | Current implementation scope | State |
| --- | --- | --- |
| W5.1 / W5.A1 / W5.X | Survey, A1, BMFont JSON/raw layouts and SSFN/PFR frozen | Outline: 19 tests, 84 original-consumer checks; long ASAN running |
| W5.2 / W5.D | Johab/IIgs final3 and fbit/lisa/mgtk/qpf/qpf2/NFNT BinHex frozen; public integration audit | D: 71 regressions, 73 native comparisons, six isolated gates and six long ASAN campaigns pass |
| W5.3 / W5.C1 / W5.C2 | Original three and six C1 codecs frozen; seven workstation/printer codecs implemented, validation underway | C1 complete long ASAN passes; C2 tests and original consumer comparisons continue |
| W5.4 / W5.B | Palm/GDR and eight system font families implemented | B: 46 independent checks; a final detector correction and long ASAN are underway |
| W5.5 / W5.A2 | DRCS/C and eight native console/loader codecs frozen; bounded gzip transport integrated | A2 eight long ASAN pass; transport has 13 dedicated passing regressions |
| W5.6 / W5.E1 / W5.E2 | Color, six device families and PILfont/SFont/VLW/imageset frozen | Image group: 26 tests and 20 native-tool comparisons; long ASAN running |

All 99 catalogue IDs now have connected native reader and writer entry points.
This is an implementation inventory, not phase acceptance; the C2 group is
finishing its final oracle and feature checks. Per-reader long ASAN campaigns, coordinated quiet
release timing, aggregate gates, main integration and a verified backup remain
required before moving to phase 6.

First long ASAN evidence is archived in
[`phase5-validation/initial-long-fuzz.json`](phase5-validation/initial-long-fuzz.json):
vfont 1,477,611 runs, FZX 1,853,831, FIGlet 572,703, color 334,887 and
embedded OpenType/Apple bitmap readers/writers 6,910,245; each ran 611 seconds,
exit 0, with peak ASAN RSS 381–509 MiB. The initial frozen module source and
harness hashes match their evidence; shared API and disabled-feature guards
continue to evolve, so these are not a claim that the whole final tree is accepted.

Additional completed campaigns are archived in
[`phase5-validation/native-long-fuzz.json`](phase5-validation/native-long-fuzz.json).
Earlier Johab/IIgs runs remain as superseded evidence because final3 changes the
strict-mode audit; new final3 campaigns ran 612/611 seconds respectively without
errors. E1, A2, X and D campaigns have completed on their verified owned-file
freeze manifests. The cumulative [long-fuzz index](phase5-validation/long-fuzz-index.json)
currently contains 59 passing current campaigns and five superseded campaigns.
All fifteen newly completed B/E2 image/outline campaigns ran 611 seconds without
crashes; GPI and SymbOS are being repeated after review fixes, and C after its
native declaration detector correction. C2, public transport and corrected FON
campaigns remain pending. These remain module evidence, not aggregate acceptance.

An aggregate preview completed `scripts/check.sh` with 2,449 passing test
executions across 592 suites, plus both Clippy configurations and dependency
checks. Subsequent narrowly scoped review fixes require a fresh candidate/main
acceptance run; the preview is retained as intermediate evidence.

The latest all-feature CLI run passes all test suites, including 27 general
command cases, seven cumulative input-budget regressions, six P2 cases and
19 P3 native variant/resource/pixel cases. These include PXL TFM and Daisy
fragments, actual Consoleet/image directories, every Prebuilt depth and integer
byte order with both explicit pixel orders, both PFR dialects and native
printer/workstation profiles.
System/device CLI cases continue to expose and verify native container boundaries.
NFNT's CLI container matrix now includes BinHex and passes. Native directory
Input/OutputBundle support has 22 passing shared
contract tests and eight CLI filesystem tests, including rollback, scoped
listing, budget enforcement, rejection of stale output-directory members and a
2,048-input descriptor regression. CLI directory image tests cover PNG/BMP/GIF/
TGA/PBM/PGM, black/white/alpha backgrounds and unchanged output on strict errors. The shared
legacy report audit uses a budgeted sorted alias index to avoid quadratic work;
its regression and the original W5.3 tests pass. Minimal CLI all-targets Clippy
passes, including the latest no-default-features BDF/raw build. Quiet release
performance and final binary size are still outstanding.

Worker requests use `gpt-6-astra`, `xhigh`, `fork_turns=none` per the supplied
worker configuration. Actual backend identity and fast service mode cannot be
independently verified. No remote push or external site deployment is part of
this work.
