# Phase 2 acceptance candidate

2026-10-03 UTC. Native complete Tong export now meets the 40-second target with
a **38.414 s five-run median at quality 11 after independent safety fixes**, while preserving independently
checked glyphs, metrics, mapping and shaping. The full supplied SC square TTF
also meets its 1-second target at **0.879 s median**.

**This is an unmerged candidate, not Phase 2 acceptance.** Independent review
confirmed and fixed an input/output-alias truncation defect and a 32-bit checked-add
boundary. The fixes are validated on `review/phase2-input-safety` at
`7b7ed29c86a75d02d4a429bdb61d9ec4324c3c61`; coordinator integration remains.
Main remains reviewed W2.8 `54c7f299ba226d6f59946a7f2943692ba5a21c02`. No W3 work
has begun.

## Scope completed

W2.1–W2.8 were already reviewed and integrated into main: sfnt common tables;
square/separate-square/dot outlines and glyf/loca; CFF1; embedded bitmaps and OTB;
WOFF1/WOFF2; regional variants, GSUB locl and subsets; vector rasterization and
collections; and SVG images/glyphs/fonts with bounded reading.

This candidate completes the previously missing whole-family input recovery,
combined external matrices and five-run comparison work, and adds performance
changes on branch `phase2-acceptance`:

- Packed small-grid square tracing with the exact existing edge/walk/rotation
  order; larger grids and compatibility rows keep the general path.
- Immutable reusable variant plans with exact layout/resource-limit matching.
- Existing-glyph dot reuse, without adding glyph IDs. Helpers stay literal,
  nesting is at most two levels, each ink pixel is consumed once, and original
  literal-dot work budgets remain enforced. Unsupported/extreme cases fall back.
- CFF alternating-axis operand runs capped at 40, below the Type 2 stack limit.
- Release ThinLTO and bounded multi-font export reusing parsed faces, plans and
  encoded sfnt bytes. The complete-family example is not the Phase 6 batch CLI.
- Explicit `literal`, `rows` and `patterns` dot policies, because faster/smaller
  TTF generation can produce larger compressed webfonts. Quality remains 11.

The pre-review production commits are `31173757114e1e629dfe9cf844d500b1c035956c` and
`df83a084a52caa125f563259888426e391d2b585`. Later candidate commits record tests,
benchmark harnesses and reports. Exact code and measured binary identities are
in `phase2-optimization/production-final.sha256` and `binaries-final.sha256`.

## Source provenance

The supplied task package contains seven BDFs, not the complete two-size,
two-spacing, five-face family. The full twenty-input set was rebuilt from the
user's existing `tong-dot-package-2026-10-02.zip`, using its builder, which is
byte-identical to the protected reference builder. All source and output hashes
are recorded in `phase2-acceptance/baseline-source-manifest.json`.

The three supplied Latin BDFs match the rebuilt sources exactly. The four
supplied regional BDFs have 4,323 updated glyphs and are tested separately.
They were never substituted with the older recovered snapshot. All four
uncropped-source/rebuilt-BDF reference plans have identical glyph order, cmap
and language substitutions. No active glyph-editing tree was used as input.
All 690 protected reference/sample files remain unchanged.

## Terminal validation

- Pre-review aggregate `scripts/check.sh`: 912 passing test invocations; fmt, both Clippy
  configurations and dependency advisory/license/source checks pass.
- Independent review aggregate: 928 passing executions, including six path and two
  checked-arithmetic tests. Eight actual release-binary alias/rerun cases pass.
- Warning-free rustdoc; full final-code compatibility parity: 0 NEW divergences,
  with the same seven documented K12 cases retained.
- Complete-family external matrix: 32 cases, 1,431,612 glyph-format checks.
- Newer supplied regional inputs: twelve cases, 547,971 glyph-format checks.
- WOFF1: sixteen full sfnts, exact independent table/native roundtrip identities,
  plus independent fontTools-writer/native-reader and OTS checks.
- Dot policies: four literal outputs remain byte-identical to the fully checked
  baseline; four row-policy fonts pass 178,954 independent full-glyph checks.
- Subsets: sixteen fonts, three output containers and native rereading, with
  87,552 shaping-glyph checks against original and fontTools subsets.
- Native vector rasterization: twenty-four full sfnt/webfonts, 1,073,708 glyphs,
  matching FreeType at 14 ppem, with no exceptions.
- All three five-run native timing profiles are tied to complete independent
  oracles by exact source/sfnt/table/stream identities; every webfont passes OTS.
- All 54 baseline PCF/OTB cases covered 1,585,430 glyph-format checks. Final
  rebuilt and supplied outputs are byte-identical to those oracle inputs.
  PCF's 586 supplementary codepoints per regional face are explicitly reported
  as unrepresentable, not silently dropped or counted as failures.
- Six exhaustive SVG-font outputs are byte-identical to the accepted W2.8
  oracle files; 48 preview/PNG/SVG combinations and three PCF/gzip checks pass.

Fresh ASan runs use new nested-dot and independently checked 40-operand CFF seeds:

| Reader | Runs | Seconds | Coverage | Feature count | Peak RSS MiB |
|---|---:|---:|---:|---:|---:|
| WOFF2 | 442,906 | 612 | 3,981 | 12,442 | 112 |
| Vector raster | 304,010 | 612 | 6,052 | 17,183 | 498 |
| CFF | 9,491,231 | 612 | 2,849 | 8,391 | 393 |

All exit 0 with no crash. The earlier accepted gates for unchanged readers are
retained in the individual W2 reports. Raw terminal evidence, hashes and exact
commands are in `phase2-optimization/`; `validation-summary.json` and
`fuzz-summary.json` provide compact indexes.

## Performance

The native complete workflow writes 84 files: 20 BDF copies plus 20 PCF, 20 OTB,
8 TTF, 8 OTF and 8 WOFF2. Eight bounded font jobs and one Brotli worker per font
are the example's throughput defaults on this nine-logical-CPU host. Individual
CLI/library exports use two workers for streams of at least 128 KiB.

| Workflow | Five-run median seconds | Eight WOFF2 bytes |
|---|---:|---:|
| Reviewed guarded native default quality 11 | 38.414 | 8,778,688 |
| Frozen pre-review native quality 11 | 38.233 | 8,778,688 |
| Native explicit quality 8 | 7.238 | 10,252,548 |
| Native explicit quality 5 | 6.101 | 11,147,828 |
| Original Tong Rust plus Python preparation, quality 11 | 134.569 | 7,456,168 median |

Guarded native runs are 38.414, 36.947, 39.481, 39.317 and 36.554 s, with
2.869 GiB maximum process RSS. All 84 outputs and all inputs exactly match the
independently validated frozen candidate. Its earlier runs ranged 37.868–42.079 s
with 2.905 GiB RSS. Lower-quality comparison timings retain their pre-review
binary identities; no unmeasured post-fix speed is attributed to those rows.
The requested criterion is the median, not a per-run latency guarantee. The
same-input SC square TTF comparison is 0.879 s native versus 12.800 s fontTools;
full square CFF WOFF2 compression is 18.724 s versus 19.184 s; Latin-dot WOFF2 is
1.626 s versus 2.351 s. All are five-run measurements with complete input and
output identities, and read/plan/I/O boundaries documented.

The guarded full native workflow is about 3.50× faster than the measured original workflow;
this is not a compressor-only claim. The default pattern policy grows dot
webfonts about 35–37% versus the earlier native literal representation, even
though TTFs shrink markedly. Aggregate webfont bytes rise 13.27% versus that
native baseline, or about 17.74% versus the original Tong workflow. Callers can
select literal or row reuse instead. No lower compression quality is hidden.

Measurements use a shared Xeon 8370C host with nine reported logical CPUs and
~9.73 GiB RAM, Rust 1.99.0 and no native-CPU flags. Intermittent light glyph
render/checkpoint work could occur, but no other sustained build, fuzzing or
correctness job overlapped accepted timing intervals. Earlier historical native
baseline/probes used RAM-backed scratch; final native/reference measurements
use disk-backed output scratch. No exclusive-core or cross-machine timing claim
is made. See `../benches/PHASE2-REPORT.md` for raw samples, stage data and caveats.

## Known issues and rejected experiments

The example's alias defect and a 32-bit width/height-plus-one overflow were
confirmed and fixed on `review/phase2-input-safety`. Actual task inputs remained
untouched. The guard checks every output against all inputs and rejects output
aliases, dangling links and non-file destinations while allowing normal reruns.
It assumes a stable filesystem rather than hostile concurrent path replacement.
The arithmetic fix is covered by maximum-usize/public-zero-height regressions;
no actual 32-bit executable run is claimed. See `phase2-review/README.md` and
`phase2-optimization/REVIEW-NOTES.md`. No remaining blocking finding was identified
within the reviewed scope; fixes and candidate still require integration.

The original Tong reference has a known null DFLT DefaultLangSys defect that OTS
rejects. That original rejection is recorded explicitly; native outputs never
receive a waiver. Complete pixels, metrics and language shaping still agree.
The read-only FontTools memoryview adapter is used only for full-dot WOFF2
correctness, not stock fontTools performance measurements.

An experimental Brotli shared-hasher option was rejected after the new large
parallel regression caught an upstream assertion. It remains disabled. An
initial ASan compilation was killed under concurrent build memory pressure;
it succeeded with one Cargo build job and sixteen codegen units, retaining
sanitizers, debug assertions and coverage. Failed/partial development attempts
remain labelled and are not counted as accepted gates or timings.

Previously documented container/tool limitations and format-specific losses
remain in their W2 reports and format documentation. This work does not implement
later-stage formats, bindings or the generic batch CLI. Runtime model and
accelerated-service settings cannot be independently verified here.

## Handoff and acceptance boundary

Keep the unmerged candidate and independent review-fix branches. Integrate the
validated `7b7ed29` review fixes and candidate evidence, rerun integrated-main
checks, verify measured-output identities or repeat affected timings, and then
write the accepted `phase-2.md` report. Do not start W3 before that acceptance
and delivery. The checkpoint bundle includes all branches; large generated fonts,
compiled targets and downloaded dependency/source payloads are excluded.
