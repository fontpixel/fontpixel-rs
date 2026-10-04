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
| W5.1 / W5.A1 | Survey complete; dosstart, edwin, mkwinfont, psf2txt, pcr, plus3dos, drhalo, writeon | Eight-format implementation started |
| W5.2 | Johab 8×4×4 Hangul, IIgs and BinSCII | Implementation and independent validation underway |
| W5.3 | vfont, FZX, FIGlet | Focused tests/oracles pass; final freeze pending |
| W5.4 | Palm versions/containers/gray, GDR versions | Implementation and independent validation underway |
| W5.5 | DRCS, U8g2/LVGL/NetBSD/U8glib/U8x8 C profiles and LVGL binary | Implementation and independent validation underway |
| W5.6 | CBDT/CBLC and sbix output | Focused tests/oracles pass; final freeze pending |

The remaining survey groups, imageset, BinHex, BMFont JSON and raw layouts stay
required and are queued after current owned work. No partial implementation is
reported as phase completion. Per-reader long ASAN campaigns, coordinated quiet
release timing, aggregate gates, main integration and a verified backup remain
required before moving to phase 6.

Worker requests use `gpt-6-astra`, `xhigh`, `fork_turns=none` per the supplied
worker configuration. Actual backend identity and fast service mode cannot be
independently verified. No remote push or external site deployment is part of
this work.
