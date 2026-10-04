# Phase 4 acceptance

2026-10-04. Six parallel work items implement all sixteen P2 format IDs on top
of accepted phase 3 commit `cbbaa359c7bad120a3da6dc11c189b46cb4601f7`.
The implementation candidate passes all phase 4 gates. The local integration
record identifies the resulting main commit and backup separately.
Phases 5–9 remain work to do; this is not whole-project completion.

## Scope and independent validation

| Work item | Formats | Evidence |
| --- | --- | --- |
| W4.1 | draw, raw presets, CPI/CP/CPX | 29 tests; 36 external reader/preset cases; 8 native CPX writer cases; [report](w4.1.md), [CPX](w4.1-cpx-writer.md), [independent review](w4.1-cpx-review.md) |
| W4.2 | PF2, FONTX2, HZK/ASC | 31 tests; 26 external cases / 91,379 slots; [report](w4.2-validation.md) |
| W4.3 | NFNT/FONT and seven Mac containers | 31 all-feature tests; 135 external cases and detector checks; [report](w4.3-nfnt.md), [final detector fix](w4.3-detector-final2.md) |
| W4.4 | BMFont, SFD bitmap, Kbits | 31 tests; 11 independent oracle groups and complete 38,653-glyph functional checks; [report](w4.4-validation.md) |
| W4.5 | PK, GF, Plan 9 | 27 tests; 9 external TeX/Go comparisons; [report](w4.5.md) |
| W4.6 | Amiga, GEM, SNF | 19 tests; 8 independent oracle groups; [report](w4.6.md) |

The all-feature registry now has **38 readers and 37 writers** across 46 IDs.
The eight remaining IDs are hanfont, iigs, vfont, fzx, figlet, palm, gdr and drcs;
color currently has a reader and still needs its writer. Phase 5 also surveys
additional general formats. Python/WASM bindings and later CLI/merge work are
not counted as complete.

Format-specific limits are documented under `docs/formats/`. Native CPX supports
NRV2B decoding and a bounded original encoder/stub: decoded FONT and CPX file
sizes are capped at 58,880 bytes, matches use distances at most 256, and output
requires net compression. Independent UPX decoding reproduces exact CPI bytes;
Unicorn executes the original external FreeDOS MODE loader and checks return
state and memory guards. External GPL tools are validation-only dependencies.
No DOS code executes in the Rust reader. NFNT/FONT palettes and Amiga color
inputs use documented grayscale projection with loss reports. GEM compressed
input is supported; GEM output uses the documented uncompressed representation.
BMFont uses bounded PNG pages; SFD and Kbits preserve native grayscale.

## Aggregate gates and integration corrections

`scripts/check.sh` passes on the final implementation: **1,536 passing test
executions in 273 suites**, including repeated feature configurations rather
than 1,536 distinct tests. It covers generated tables, formatting, workspace
Clippy with warnings denied, full/default/minimal feature tests and dependency
licenses. Additional final checks pass for all-feature rustdoc with warnings
denied, a P2-only timing example without CJK tables, and the minimal CLI.
Fuzz-workspace license checks also pass. Exact commands, exits and compressed
logs are in `phase4-evidence/phase4-check-final2.*`,
`phase4-extra-gates-final2.*` and `phase4-extra-gates.*`.

The first aggregate gate found NFNT's MacBinary detector falsely claiming an
OpenType fixture. Structural envelope/resource/raster validation fixes that
collision; all 31 vector tests and the final NFNT tests pass. GEM/SNF option
setters now update detectors along with readers. A GEM regression exercises
ambiguous byte-order detection through CLI options. Registry writer failures
retain preceding read diagnostics. CLI tests cover strict output protection,
sidecars, all seven Mac containers, atlas pages, machine layouts and `.cpx`
selection. [The CLI guide](../docs/cli-p2.md) documents all new options.

`final-source-verification.json` compares the final source with all successful
reader campaigns and the four aggregate/build snapshots (401 files each).
Worker manifests are checked in `worker-freeze-verification.json`.
`protected-inputs.json` verifies all 690 protected inputs without changes.

## Final-source ASAN campaigns

Every new reader completed **611 seconds** with a 64 KiB input cap, a 10-second
per-input timeout and a 1 GiB sanitizer RSS limit. There were no crashes,
timeouts or resource-limit violations. Successful writer paths are reopened
under the same limits and lossless cases compare native models, as defined in
the individual harnesses.

| Reader | Executions | Coverage / features | Peak fuzz RSS |
| --- | ---: | ---: | ---: |
| pf2 | 9,092,208 | 2,009 / 4,619 | 518 MiB |
| fontx2 | 1,665,924 | 1,612 / 5,307 | 509 MiB |
| hzk | 630,218 | 1,208 / 4,309 | 453 MiB |
| pk | 5,364,877 | 1,430 / 5,395 | 523 MiB |
| gf | 20,817,118 | 1,457 / 3,308 | 519 MiB |
| plan9 | 26,762,590 | 2,413 / 7,418 | 527 MiB |
| draw | 2,855,865 | 1,115 / 4,875 | 513 MiB |
| raw | 375,673 | 988 / 3,499 | 426 MiB |
| amiga | 12,554,541 | 2,072 / 5,048 | 439 MiB |
| gem | 2,856,943 | 1,641 / 5,249 | 489 MiB |
| snf | 8,555,599 | 1,703 / 6,318 | 509 MiB |
| bmfont | 7,475,230 | 6,083 / 17,674 | 481 MiB |
| sfd | 5,102,599 | 2,262 / 11,231 | 515 MiB |
| kbits | 9,605,199 | 1,385 / 4,446 | 525 MiB |
| nfnt | 4,490,490 | 3,256 / 9,333 | 504 MiB |
| cpi | 313,698 | 1,973 / 7,379 | 436 MiB |

`fuzz-summary.json` links exact logs, commands and source manifests. Earlier CPI
and NFNT campaigns are explicitly marked superseded after CPX writer and
detector changes. Draw/raw sources and their shared helper remained unchanged;
their relevant source scopes are enumerated. No short, interrupted or build-only
run is counted. Sanitizer RSS is not ordinary release memory use.

## Release performance

Five sequential release runs were measured after task-owned compilers, fuzzers
and oracles stopped; unrelated desktop applications remained running. Binary
hashes, machine details, raw observations and min/max values are committed under
`phase4-evidence/`. This is a shared workstation, not an isolated performance lab.

The small-font example verifies all original 128 8×16 rasters and code mappings
before measuring. Each process performs five warmups and 100 read/write
iterations, then 400 independent reads on 1/2/4 threads. Fixed tables may add
blank/missing slots, shown by decoded counts. Times below are five-run medians;
output size includes all assets. Four-thread speedup compares the same 400 reads.

| Format | Decoded glyphs | Read µs | Write µs | Bytes | 4-thread speedup |
| --- | ---: | ---: | ---: | ---: | ---: |
| draw | 128 | 129.34 | 233.19 | 21,248 | 2.77× |
| raw | 128 | 23.01 | 33.13 | 2,048 | 2.73× |
| cpi | 128 | 17.25 | 24.43 | 2,113 | 2.71× |
| cpx | 128 | 20.51 | 34.43 | 1,109 | 3.01× |
| pf2 | 128 | 32.80 | 53.22 | 4,574 | 2.77× |
| fontx2 | 256 | 35.60 | 27.22 | 4,113 | 2.65× |
| hzk | 128 | 17.76 | 24.04 | 2,048 | 3.09× |
| nfnt | 128 | 25.68 | 43.51 | 2,988 | 2.96× |
| bmfont | 128 | 469.19 | 1613.08 | 13,847 | 3.51× |
| sfd | 128 | 135.49 | 200.58 | 13,343 | 2.87× |
| kbits | 128 | 42.40 | 88.46 | 28,200 | 2.67× |
| pk | 128 | 51.45 | 106.80 | 3,476 | 2.80× |
| gf | 128 | 99.17 | 213.53 | 17,856 | 2.52× |
| plan9 | 128 | 91.57 | 135.11 | 2,957 | 3.15× |
| amiga | 129 | 26.81 | 44.80 | 3,268 | 2.49× |
| gem | 128 | 23.83 | 43.75 | 2,650 | 2.83× |
| snf | 128 | 45.66 | 90.23 | 10,389 | 2.69× |

Peak ordinary process RSS across these runs is 18,824 KiB
(18.38 MiB). Small-font throughput does not establish
full-catalogue performance or imply one large font uses multiple threads.

The complete Tong SC font contains 38,653 glyphs. A separate five-process
measurement performs one whole-font read/write per process, without explicit
warmups; initial BDF parsing is outside the timers. Default resource budgets
succeed. These three formats can represent the complete sample; fixed legacy
code tables generally cannot. Prior full-font functional checks are in
`w4.4-large-functional.json`.

| Format | Read ms | Write ms | Output bytes |
| --- | ---: | ---: | ---: |
| bmfont | 138.253 | 550.394 | 4,439,568 |
| sfd | 42.234 | 77.442 | 5,081,353 |
| kbits | 14.610 | 51.484 | 8,844,721 |

Peak process RSS is 277,720 KiB (271.21 MiB),
including BDF parsing, original/copy models and temporary output/readback data;
it is not a per-codec allocation figure. These are phase 4 measurements, not
proof that all final §10 targets have been met. Reproduction commands and exact
methods are in [the benchmark guide](../benches/README.md).

## CLI size

The final all-feature Linux x86-64 executable is 10,823,176 bytes before
stripping, **9,739,984 bytes (9.29 MiB)** with `strip --strip-all`, and
**4,314,307 bytes (4.11 MiB)** with deterministic gzip. It links the standard
Linux C/math/GCC runtime libraries and requires no Rust or Python installation.
`cli-size.json` records all hashes and the version smoke check; `formats.json`
records actual capabilities. This measures the phase 4 build, not the eventual
final cross-platform release size.

## Coordination and remaining work

Workers W4.1–W4.6 were requested with `gpt-6-astra`, `xhigh`, and
`fork_turns=none`, following the supplied worker settings. Actual backend model
identity and fast service mode cannot be independently verified. The user
authorized parallel workers and cleanup of extra source directories; this stage
used one checkout with exclusive module ownership and a coordinator-owned
integration branch instead of creating more worktrees. No remote was pushed.

New decisions cover explicit layouts/native CPX, PF2 variant identities and
exact charsets, bounded resource budgets, native grayscale and writer failure
reports. Detailed field limitations remain in each format document. No user
decision blocks the next stage. Phase 5 covers the remaining eight IDs, the
color writer, additional C families and the general-format survey. Later CLI,
bindings, merge APIs/page, migration documentation and final performance goals
remain required.
