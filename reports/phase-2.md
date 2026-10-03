# Phase 2 acceptance report

2026-10-03 UTC. Phase 2 OpenType work is complete and integrated into `main`,
including independent review fixes. The complete Tong export meets the requested
40-second target: **38.414 seconds median over five runs at quality 11**, with
**2.869 GiB peak process RSS**. All 84 outputs are deterministic and match the
independently validated full-font artifacts exactly. No Phase 3 work is included.

## Accepted scope

W2.1–W2.8 provide sfnt common tables and metadata, merged/separate-square and dot
outlines, glyf/loca and CFF1, embedded monochrome bitmaps and OTB, Apple bdat/bloc,
CBDT/sbix reading, WOFF1/WOFF2,
regional `Variants`/GSUB `locl`, subsetting, vector rasterization, collections,
and SVG images/glyphs/fonts with bounded reading. Compatibility mode retains its
separate byte-level contract.

The final performance work adds a bounded packed small-grid tracer, reusable
immutable variant plans, existing-glyph dot-pattern reuse, packed CFF axis runs,
release ThinLTO and a bounded multicore full-family API example. The example
reuses input faces, variant plans and encoded sfnt bytes. It is not the planned
Phase 6 generic `fontpixel batch` command.

The candidate history is `b7995ccb0bb127a84ac7c1b3db4bc0b9dba36470`; production
optimization commits are `3117375` and `df83a08`. Independent fixes are
`7b7ed29c86a75d02d4a429bdb61d9ec4324c3c61`. Final integration and source identities
are recorded in `reports/phase2-integration/`. The tested integration merge is
`5e31babdfdae6fc17b5a0e15b6253a0aa8abee49`; the following report commit changes only
acceptance documentation and retained gate evidence.

## Correctness and robustness

- Final integrated `scripts/check.sh`: **928 passing test executions** across
  feature profiles, exit 0. Warning-free integrated rustdoc also passes. Both
  integrated and independent-review gates have clean
  format, Clippy and dependency/license/source checks. Counts include repeated
  tests across configurations rather than 928 distinct tests.
- The integrated production source is verified against the tested review commit.
  Its final 84 output hashes match the complete external oracle set exactly;
  unchanged expensive matrices are retained instead of rerun under a new label.
- Complete-family vector/webfont matrix: 32 cases and 1,431,612 glyph-format checks.
  Newer supplied regional inputs: 12 cases and 547,971 checks.
- Sixteen complete WOFF1 transports pass independent table, native roundtrip,
  fontTools-writer/native-reader and OTS checks.
- Four literal-policy fonts retain baseline bytes. Four row-policy fonts pass
  178,954 independent full-glyph checks. Patterns is validated in the full matrix.
- Sixteen fonts undergo three-container subsetting and rereading, with 87,552
  shaping-glyph checks against original and fontTools subsets.
- Twenty-four complete sfnt/webfonts pass native raster reading against FreeType
  at 14 ppem: 1,073,708 glyphs and no exceptions.
- The 54 PCF/OTB baseline cases cover 1,585,430 glyph-format checks. Final rebuilt
  and supplied bitmap outputs match their oracle inputs exactly. PCF cannot
  represent 586 supplementary codepoints per regional face; each is reported.
- Six exhaustive SVG outputs match accepted W2.8 bytes; 48 preview/PNG/SVG
  combinations and three PCF/gzip checks pass.
- The retained compatibility gate has zero new divergences; the review fixes
  leave compatibility code unchanged. Seven documented K12 ragged-bitmap cases
  remain. Warning-free rustdoc also passed.
- Fresh ASan gates ran 612 seconds each without a crash: WOFF2 442,906 executions,
  vector raster 304,010, CFF 9,491,231. Coverage/feature counts are respectively
  3,981/12,442, 6,052/17,183 and 2,849/8,391. Earlier gates for unchanged readers
  remain in their individual W2 reports.
- All 690 protected reference/sample files remain unchanged.

Evidence indexes: `phase2-optimization/validation-summary.json`,
`phase2-optimization/fuzz-summary.json`, `phase2-review/README.md`, and
`phase2-integration/`. Full logs, exact commands and binary/source hashes are
retained in the repository history.

## Independent review fixes

The new exporter originally allowed output paths to alias its source BDFs.
A disposable 20-face reproduction returned success while truncating every input.
The accepted preflight checks all 84 destinations against all sources and against
one another using portable file identity. It rejects same-path, symlink and
hard-link aliases, unresolved output links and non-file destinations before any
file is written. Ordinary separate-directory reruns remain supported. Six unit
tests and eight actual-binary cases pass, including an identical-byte 84-file
rerun. This preflight assumes stable paths; it does not claim protection against
hostile concurrent filesystem replacement.

A second fix checks width+1 and height+1 before dense-grid multiplication.
Explicitly raised bitmap limits could otherwise overflow usize on 32-bit targets
for a valid zero-area bitmap. Maximum-usize arithmetic and a public maximum-width,
zero-height bitmap are tested. No 32-bit executable run is claimed on this host.

## Performance and size tradeoffs

The complete workflow reads/parses twenty inputs and writes twenty BDF copies,
twenty PCF, twenty OTB, eight TTF, eight OTF and eight WOFF2 files. Timings include
startup, parsing, planning, encoding, output I/O and teardown. They exclude build,
oracle checks, hashing and reconstruction of the frozen BDF input set.

| Profile | Five-run median seconds | Peak RSS GiB | Eight WOFF2 bytes |
|---|---:|---:|---:|
| Accepted guarded exporter, default quality 11 | 38.414 | 2.869 | 8,778,688 |
| Pre-review codec, explicit quality 8 | 7.238 | 2.660 | 10,252,548 |
| Pre-review codec, explicit quality 5 | 6.101 | 2.562 | 11,147,828 |
| Original Tong Rust workflow including Python preparation | 134.569 | Not reported | 7,456,168 median |

The accepted default samples are 38.414, 36.947, 39.481, 39.317 and 36.554 seconds.
The acceptance criterion is the median, not a latency guarantee on every host or
run. The q8/q5 observations retain their pre-review executable identity; the
safety fixes do not change normal encoding, but those optional timings were not
rerun under the guarded executable. The earlier default candidate measured
38.233 seconds with 2.905 GiB peak RSS.

The accepted native workflow is approximately **3.50 times faster** than the
measured original workflow. This includes planning and export differences and is
not a claim of universal compressor superiority. On identical supplied SC input,
square TTF generation measures 0.879 seconds native versus 12.800 seconds stock
fontTools, meeting the separate one-second target. Full square-CFF WOFF2 q11/t1
compression measures 18.724 versus 19.184 seconds; Latin-dot compression measures
1.626 versus 2.351 seconds. These are separately scoped five-run comparisons.

Default Patterns reduces total output from 396,034,689 to 326,576,649 bytes,
17.54% smaller than the earlier native literal workflow. However, aggregate
WOFF2 grows from 7,749,984 to 8,778,688 bytes, **13.27% larger**, and individual dot
webfonts grow about **35–37%**. Native webfonts are about 17.74% larger than the
original Tong workflow's median total. These are explicit speed/size choices:
`--dot-reuse literal|rows|patterns` is available in native CLI/API workflows.
Quality stays 11 by default; optional q8/q5 increase webfont size further.
All policies retain glyph IDs, mappings, metrics and displayed circle geometry.

The full-family example defaults to up to eight font jobs and one Brotli worker
per font. Individual library/CLI exports default to two codec workers above
128 KiB, with a deterministic serial path for smaller streams. Concurrency and
quality are explicit and bounded. See `benches/PHASE2-REPORT.md` for full methods.

## Inputs and measurement conditions

The task package's seven BDFs do not contain the full twenty-face family. That
set was rebuilt from the user's original Tong archive using its unchanged
builder; exact hashes and reference-plan equivalence are retained. The three
supplied Latin BDFs match that snapshot. The four newer supplied regional inputs
contain 4,323 updated glyphs and were tested separately, never replaced with the
older snapshot. No active glyph-editing tree supplied the timed inputs.

Measurements used Linux 6.18.44 x86_64, Xeon Platinum 8370C, nine reported logical
CPUs, approximately 9.73 GiB RAM, Rust 1.99.0/LLVM 23.1.1, thin LTO and one codegen
unit without native-CPU flags. Python is 3.12.14, fontTools 4.66.1, FreeType binding
2.5.1, HarfBuzz binding 0.56.2 and OTS 9.2.0. The host is shared; no exclusive-core,
affinity, frequency or quota guarantee is claimed. No sustained build, fuzz or
correctness job overlapped accepted timings, but unrelated intermittent light
render/checkpoint work could occur. Earlier historical baselines used RAM-backed
scratch; final native/reference outputs are disk-backed. Cross-machine speed or
isolated per-optimization causality is not claimed.

## Known limitations and exclusions

The original Tong reference has a null DFLT DefaultLangSys that OTS rejects.
That specific original-reference rejection is recorded; native outputs never
receive the waiver. Pixels, metrics, mappings and language shaping still agree.
A read-only fontTools memoryview adapter avoids quadratic copies only during
full-dot correctness checks; stock fontTools timing comparisons do not use it.

The experimental Brotli shared-hasher setting was rejected after a large-stream
regression exposed an upstream assertion. It stays disabled. Failed intermediate
compiles, resource-killed attempts and partial probes remain clearly labelled;
none is counted as a passing gate or accepted timing.

The crate remains under development. The new public `dot_reuse` field can require
an update to exhaustive options struct literals; native packed representations
and compression defaults intentionally change bytes. Current format documentation records remaining unsupported encodings and
information-loss diagnostics; historical W2 exclusions may have been superseded
by later Phase 2 work. A zero-new-parity
result is not a promise that previously documented compatibility differences vanish.

The requested worker setup was gpt-6-astra, xhigh, without inherited chat.
Runtime model and accelerated-service settings cannot be independently verified.
Per-item launch/configuration notes are retained in `reports/w2.1.md` through
`reports/w2.8.md` and their independent `w2.5-review.md` through
`w2.8-review.md` reports; the combined acceptance and review tasks launched no
further workers.
This delivery excludes later-stage formats, Python/JavaScript bindings, the
Phase 6 generic batch CLI, downloaded toolchains, compiled targets, generated
large-font payloads and protected reference/sample copies. The Git bundle retains
all branches and full history, including historical unfinished/rejected states;
`main` and this report identify the accepted state.

Phase 3 may begin only after this accepted Phase 2 delivery is sent. Its first
work covers the remaining P1 formats. No W3 code or task is started by this
integration work.
