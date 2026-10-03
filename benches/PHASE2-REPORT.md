# Phase 2 complete Tong export performance

The independently safety-fixed exporter takes a **38.414 s median of five runs**
at quality 11, meeting PLAN §10's ≤40 s target on this host. Every one of its 84
artifacts is byte-identical to the full independent oracles. The earlier frozen
candidate measured 38.233 s; both identities are retained separately below.
All correctness/review gates pass, but coordinator integration and Phase 2
acceptance are still pending. No Phase 3 work is included.

## Machine and method

- Shared Linux 6.18.44 x86_64 host, Intel Xeon Platinum 8370C at reported 2.80 GHz.
- Nine logical CPUs reported; 10,206,504 KiB RAM (~9.73 GiB). No exclusive-core,
  affinity, frequency or cgroup-quota guarantee is claimed.
- Rust 1.99.0, LLVM 23.1.1; locked dependencies; release ThinLTO and one codegen
  unit, with no native-CPU flags. Pure Rust Brotli 9.0.0, quality 11 by default.
- Twenty frozen, rebuilt BDFs from the user's original Tong source archive;
  hashes and source/reference-plan equivalence are in the acceptance evidence.
  The newer supplied Large regional BDFs are a separate correctness matrix.
- Each timed run reads/parses those twenty inputs, constructs four variant plans,
  and writes 20 BDF copies, 20 PCF, 20 OTB, 8 TTF, 8 OTF and 8 WOFF2 files.
  BDF construction from the glyph-source files is outside all engine timings.
- End-to-end wall time includes process startup, parsing, planning, encoding,
  output I/O and destruction. Compilation, correctness oracles, fuzzing and
  output hashing are outside the timed intervals.
- Native workflow: the `tong_export` Rust API example, using immutable shared
  plans and already-encoded sfnt bytes. It is not the future Phase 6 generic
  `fontpixel batch` CLI. Defaults on this host are eight font jobs, one Brotli
  worker per font, quality 11 and `DotReuse::Patterns`. Individual library/CLI
  exports default to two Brotli workers for streams at least 128 KiB.
- Outputs use the workspace's disk-backed filesystem. Earlier historical CLI
  baseline/probes used RAM-backed scratch. The relocation preserves every file
  hash and is recorded; baseline-to-candidate comparisons are not isolated
  microbenchmarks of any one optimization.
- No other sustained build, fuzz job or correctness benchmark ran during these
  accepted intervals. Five unrelated glyph-editing workers could run intermittent
  light Python renders/checkpoints. This shared-host caveat applies throughout.

## Five-run complete-family results

| Compression choice | Median s | Min–max s | Peak process RSS GiB | Eight WOFF2 bytes |
|---|---:|---:|---:|---:|
| Default quality 11 | 38.233 | 37.868–42.079 | 2.905 | 8,778,688 |
| Explicit quality 8 | 7.238 | 6.859–7.402 | 2.660 | 10,252,548 |
| Explicit quality 5 | 6.101 | 5.911–6.211 | 2.562 | 11,147,828 |

The five default samples are 37.868, 39.842, 42.079, 38.032 and 38.233 seconds.
The criterion is the specified median, not a promise that every run stays under
40 seconds. Lower quality is optional: q8 adds 16.79% webfont bytes and q5 adds
26.99% relative to the default. It is still lossless font transport; no glyph
pixels or metrics are intentionally changed.

Default per-stage medians: reads/parsing 0.770 s, bitmap outputs 1.163 s, planning
0.392 s and vector/webfont work 35.038 s. Independently selected stage medians
need not sum to the end-to-end median. Process teardown and reporting remain in
the external wall-clock total. Every raw run includes stage data, CPU time,
maximum RSS, host load, input/output hashes and binary identity.

Raw records: `results/phase2-optimized-default.json`,
`results/phase2-optimized-quality8.json`, `results/phase2-optimized-quality5.json`.

## Representation and size tradeoff

The earlier literal-dot CLI workflow measured 143.884 s at default quality 11
(five runs, four CLI processes). Its repeated reads/planning and independent
sfnt/container encoding were not shared. It produced 396,034,689 total bytes,
including 7,749,984 WOFF2 bytes. The candidate produces 326,576,649 total bytes,
including 8,778,688 WOFF2 bytes. Total exported storage falls 17.54%, while
aggregate webfont storage rises 13.27%. Individual dot webfonts rise about
35–37%; packed square CFF webfonts become slightly smaller.

This tradeoff is explicit and selectable. `--dot-reuse literal` uses one circle
per pixel, `rows` reuses existing one-row patterns, and `patterns` also reuses
columns and bounded 4×4 patterns. The default favors generation speed and small
TTFs. All modes retain the same glyph IDs, metrics, cmap and circle geometry;
quality remains 11 unless the caller changes it. Compatibility output is unchanged.

The implementation adds no helper glyph IDs. Selected existing helpers remain
literal circle composites, so there are no cycles and at most two composite
levels. The original per-glyph and aggregate literal-dot work budgets remain in
force. Large/extreme extents and unrepresentable translations fall back safely.

## Concurrency and rejected experiments

Independent-font concurrency is essential. Repeated independent CLI parsing at
eight jobs exceeded available memory in the old workflow; that failed run is
retained and is not an accepted sample. Reusing parsed faces/plans allows eight
font jobs to fit at the measured 2.905 GiB peak RSS. Single probes with more
intra-font workers used more memory without a useful throughput gain. They are
engineering observations, not five-run performance claims.

An experimental shared-hasher Brotli option was rejected after an explicit
large-input test triggered an upstream assertion. The final implementation
keeps it disabled. Nightly ASan compilation also hit a resource kill when run
alongside another large build; it was retried separately with sixteen codegen
units, unchanged sanitizers/coverage and one Cargo build job. Neither failed
attempt is counted as a successful gate or timing sample.

## Independent comparisons and validation

The original Tong Rust exporter and its unchanged Python plan/metadata helpers,
adapted to the same frozen BDF input boundary, take **134.569 s median** across
five complete runs: 136.686, 122.436, 134.569, 136.874 and 101.748 s. It uses eight
Rayon jobs. Native complete export is 3.52× faster in these measurements. This is
a workflow comparison, not a claim that the Brotli implementation alone is
3.52× faster. The reference includes a 39.141 s median Python preparation stage,
92.000 s vector/webfont stage and 0.884 s bitmap stage. Its eight webfonts have a
median total of 7,456,168 bytes, versus native 8,778,688 bytes. The native default
therefore trades approximately 17.74% more webfont bytes for its faster workflow.

Every reference repetition verifies all 84 outputs: twenty BDF and twenty PCF
files match previous source/oracle byte identities, and 44 sfnt/webfont files
match the previously validated exact tables after normalizing only head dates
and checksum adjustment. The original reference embeds current timestamps;
its aggregate WOFF2 sizes vary from 7,454,760 to 7,458,740 bytes. Native output
does not vary. See `results/phase2-reference.json`.

Stock fontTools 4.66.1 and Google Brotli 1.2.0 comparisons also use five runs:

| Same input and operation | Native median s | fontTools median s | Native bytes | fontTools bytes |
|---|---:|---:|---:|---:|
| Full supplied SC BDF to square TTF | 0.879 | 12.800 | 7,550,376 | 7,549,728 |
| Full-family Large proportional square CFF to WOFF2 q11 t1 | 18.724 | 19.184 | 1,107,844 | 1,103,688 |
| Supplied Latin dot TTF to WOFF2 q11 t1 | 1.626 | 2.351 | 39,120 | 39,004 |

The SC native result also meets the ≤1 s target and is about 14.56× faster for
this end-to-end read/plan/vectorize/write operation. Compression excludes file
I/O equally; both encoders receive identical sfnt bytes. The full CFF compression
times are close and noisy, so no broad compressor superiority is inferred. The
Latin comparison is a separate small workload, not a proxy for full CJK dots.
All 38,654 SC glyphs/metrics and 912 shaping batches pass the independent vector
comparison; all 44,833 CFF and 3,647 Latin glyphs survive both encoded containers
and the final native reader. See `results/phase2-fonttools.json`.

Final correctness now passes: 32 full-family vector/webfont cases (1,431,612
glyph-format checks); twelve newer supplied-input cases (547,971 checks); sixteen
WOFF1 transports; all literal/row policy checks; sixteen font subsets (87,552
shaping-glyph checks); and twenty-four full native raster reads (1,073,708 glyphs,
no exceptions). All three timed native profiles are tied to these independent
oracles by exact sfnt/table/stream identities and OTS. The 54 bitmap oracle cases
cover 1,585,430 glyph-format checks; all final rebuilt and supplied bitmap outputs
are byte-identical to their accepted oracle inputs.

The original Tong exporter has the already-documented null DFLT DefaultLangSys
OTS defect. Its rejection is recorded explicitly; it is never accepted for a
native font. Pixel, metric, mapping and language-shaping comparisons remain full.
Full-dot WOFF2 correctness uses the previously validated read-only memoryview
adapter in FontTools to avoid quadratic tail copies; stock FontTools performance
measurements do not use that adapter.

## Reproduction

Build `fontpixel-cli` with `--bins` explicitly; `--example` selectors alone do not
build the CLI. Build the all-feature `tong_export` and `woff_check` examples.
Run `benches/phase2_optimized.py --help` for paths and explicit options. The
normal run count is five; `--runs 1` is only a labelled engineering probe.
Run `tests/crosscheck/phase2_optimized_validate.py` after the full external matrix
so every timed artifact is tied to independent glyph/table/OTS evidence.
Exact environment, commands and final source/binary identities accompany the
Phase 2 candidate report.

## Independent safety review and fresh timing

An independent review confirmed a safety defect in this measured candidate's new
example: if the output paths alias input BDF files, direct copying can truncate
those inputs. All measurements and correctness runs used separate output paths,
so their artifacts remain valid. The fix is committed at `7b7ed29c86a75d02d4a429bdb61d9ec4324c3c61` on
`review/phase2-input-safety`, together with a checked-add boundary fix for 32-bit
grid dispatch. Its complete check passes 928 executions and its release binary
passes eight source-preservation/path/rerun cases. No actual 32-bit executable
run is claimed; maximum-usize and public zero-height cases exercise the arithmetic.

A fresh isolated profile on the guarded binary gives 38.414, 36.947, 39.481,
39.317 and 36.554 s: **38.414 s median**, **2.869 GiB peak RSS**. All 84 artifacts
are byte-identical both across runs and to the frozen candidate's full independent
oracles, and all twenty input hashes match. The guarded workflow is about 3.50×
faster than the measured original workflow. Its binary SHA-256 is
`3426bb4d2357d9983574dd35920fc9bfc7b693648d91673270ce13ceda77c455`.
See `../reports/phase2-review/default-five-run.json`, `output-identity.json` and
`README.md`. The earlier tables retain their pre-review measured identities;
no unmeasured post-fix timing is assigned to lower-quality or stock-tool rows.
The fixed branch remains unmerged pending coordinator integration.
