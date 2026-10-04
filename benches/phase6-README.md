# Complete phase 6 benchmark contracts

Status: source reconstruction after workspace replacement. The restored Rust
implementation and examples are uncompiled; Cargo dependency access is blocked.
No new formal measurement exists. Historical results cannot validate restored code.

## Required complete cases

- Full Tong Large SC BDF, 38,653 glyphs and 5,746,021 bytes: direct native and
  public registry read; BDF/PCF/TTF writes; complete BDF→PCF/OTB/YAFF/TTF;
  2,000-character rendering; all-glyph atlas mode 1; one crop and concat call.
  Compare native/compat Rust and original copied Python/Node implementations.
- Full Tong 2 sizes × 2 shapes × 2 spacing policies: 8 TTF, 8 OTF, 8 WOFF2,
  20 PCF, 20 OTB, plus 20 source BDF copies. Native API, default batch CLI and
  original export-rs are separate profiles. API quality 11/8/5, serial, intra-font
  parallel and literal/row/pattern dot reuse variants retain all outputs.
- Actual catalogue: 8 families, 34 source fonts, 173 downloads and 8 family ZIPs.
  Native batch plus native export-name helper versus supplied original Python
  download policy with its explicit fontTools vector oracle.
- Full-SC square vector against Tong python_reference.py; same-sfnt WOFF2 using
  full-SC square and full Latin dot sfnts, same quality 11 and one compression
  worker. Reads/materialization are outside the resident compression timer.
- Unifont 57,086 plus Tong Latin: western-first/all-Unifont merge, 57,106 output
  glyphs, spacing prop, default budgets, complete BDF output and full JSON report.

`phase6_perf_prepare.py` enumerates 22 cases (110 processes for five runs). The
native timing example and orchestration adapters must be built/validated before
freezing a configuration. A changed workload profile needs a new case ID.

## Timing and evidence

Each independent process performs one untimed warmup then one measured operation
where the adapter documents that contract; complete command pipelines measure
one actual command. Native grouped conversion means direct bdf::read plus public
fontpixel::write and temporary parsed model/read-report destruction. Returned
encoded bytes and file materialization are outside the inner conversion timer.
All inner operation and whole-process metrics are retained, never conflated.
Python/Node explicitly collect before and after warmup, outside the timer;
automatic in-call collection remains included. Crop/concat copy the mutable
reference input to match native immutable result semantics, explicitly timed.

`phase6_perf.py` freezes actual source/input/binary SHA, requires a coordinator
quiet-window acknowledgement for formal timing, records five fresh processes,
CPU times, wait4 RSS, sampled process-tree RSS, actual versions and machine data.
It exclusively creates each dotted-ID stdout/stderr and rejects reused paths.
Every newly generated output is archived and every member re-read/hash-verified
before that newly generated directory can be removed. Full diagnostics stay in
raw streams/archives; no report entries are omitted to gain speed.

`phase6_perf_audit.py` independently verifies frozen inputs, every raw stream,
every archive/member, raw metric reconstruction and five-run medians. It imports
no recorder code. A failure or incomplete run is not relabelled successful.
Equivalent output SHA may link to an earlier complete semantic validation only
when the validated bytes and provenance still exist and the identity is checked.

## Corrected catalogue profile

Plan version 2 retains original resolved family/style, app copyright, literal
version, full/PostScript names and source export-name descriptors. Old plans
are rejected. Bitmap tasks pass `otb_upm=1000`, `otb_reference="largest"` and
`otb_rounding="ties-even"` plus explicit metadata. Vector tasks explicitly set
legacy_style, full_name and postscript_name as well as typography/style.
Native .notdef, real bearings/extents, retained line gap/underline/spacing,
glyph identities, timestamps, name platform coverage and codec table policies
remain independently inventoried. Original fontTools fsType=4/fsSelection=0/
usBreakChar=32 are library defaults, not source licensing instructions.

The helper `phase6_catalogue_export_name PLAN.json` runs inside the native timed
pipeline. Version-1 helper jobs contain input/output, export_name, size, weight,
width, spacing, script_subset and Python-computed weight_capitalized. It writes
new temporary BDFs only; BDF/PCF tasks use those, sfnt tasks keep source input.
PCF iso8859-1→Unicode normalization, helper reads/writes, batch jobs and ZIP
packaging are all included. This is honestly labelled a composed CLI + native
helper workflow. Both engines parse/resolve all sources each run; native uses a
frozen original-policy grouping inventory while Python groups each run.

## Dependencies and build

Use the restored project's pinned Rust toolchain and approved local environment.
No Cargo command is currently claimed to pass. Root registers example features.
Intended unified build, including core fea:

```
cargo build --locked --release --all-features -p fontpixel -p fontpixel-cli \
  --bin fontpixel --example phase6_timing --example tong_export \
  --example phase6_catalogue_export_name
```

Copy supplied references into writable local-work before executing their code;
never install or write caches in protected reference/sample/validation trees.
Set PYTHONDONTWRITEBYTECODE=1 and local TMPDIR. Provision the supplied pinned
requirements-oracles.txt into local-work/oracles and bundle the copied JS entry
using its lockfile/approved esbuild. Freeze dependency versions and native shared
library identities. Required independent consumers include fontTools, Brotli,
FreeType, HarfBuzz, OTS and bdftopcf. Missing dependencies remain explicit blockers.
The supplied catalogue opf_vector extension/source is absent; its named pure
fontTools reference path is a declared replacement, not production-extension speed.

Recorder and adapter contract tests use only the standard library:

```
PYTHONDONTWRITEBYTECODE=1 python scripts/phase6_perf_test.py
PYTHONDONTWRITEBYTECODE=1 python benches/phase6_catalogue_test.py
```

These tests do not compile or exercise production Rust and cannot satisfy the
full performance/correctness gate. All actual outputs and locl/outline/pixel/
metadata/notice/merge checks remain required before final acceptance.
