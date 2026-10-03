# Combined Phase 2 acceptance: in progress

This directory records work after reviewed W2.8 main `54c7f29`. No W3 work is
started. Acceptance and performance gates remain open until the running matrices
and five-run benchmarks finish and the final report is reviewed.

## Immutable sources

The task package supplies seven BDFs, not the complete two-size/two-spacing
family. Additional complete source inputs were recovered from the user's existing
`tong-dot-package-2026-10-02.zip`, never from a changing glyph worktree. Its
`tools/build.py` is byte-identical to the fontpixel task's reference builder.
The extracted original glyph/build-data files and all twenty generated BDFs have
hashes in `baseline-source-manifest.json`. They are not checked into this repo.

The three supplied Latin BDFs are byte-identical to these rebuilt faces. The four
supplied regional BDFs have the same 38,653 codepoints and 4,323 updated glyphs.
They are therefore tested separately, not replaced or misrepresented as the
older recovered snapshot. `sample-*-equivalence.json` records the distinction.
The uncropped-source and rebuilt-BDF reference plans have identical glyph order,
cmap and language substitutions for all four complete configurations; see
`source-plan-equivalence.json`. All 690 original reference/sample files remain
unchanged, as proved by the protected manifest.

## Gates and evidence

- `check.log` / `check.exit`: complete aggregate check, 884 passing test invocations.
- `bitmaps.json`: 54 complete native PCF/OTB cases. All 1,585,430 representable
  glyph-format checks agree with source BDF and independent bdftopcf/original
  Tong Rust output. Fontconfig size/family and OTB mono identification pass.
- `vectors.json`: full supplied plus rebuilt vector/web matrix against unchanged
  Tong Python/fontTools, OTS, FreeType and six-language/eight-script HarfBuzz.
  A `partial: true` result is explicitly unfinished.
- `full-export-validation.json`: complete actual CLI output vs original Tong
  Rust exporter, including both engines' webfonts through independent and native
  readers. A `partial: true` result is explicitly unfinished.
- `native-smoke.json` / `reference-smoke.json`: 84-file functionality smoke runs
  while other correctness work was running. Their durations are not accepted
  benchmark measurements and must not be used as speed claims.
- The first bitmap harness expected fontconfig to omit X11's foundry prefix.
  Its retained failure log is `bitmaps-initial-harness.log`; the corrected check
  requires exact agreement with bdftopcf, including its foundry prefix. No native
  code or pixel/metric assertion was weakened.

No production source changed from accepted W2.8. New harnesses add evidence only.
The original Tong Rust export also reproduces the W2.6-documented null DFLT
DefaultLangSys defect. OTS rejects its original sfnt and WOFF2 for that same
reason. This rejection is retained explicitly, never counted as an OTS pass,
and never permitted for a native output. The original reference still supplies
independent full-glyph, metadata and shaping evidence; native output is conforming.
FontTools's full-dot WOFF2 decoder uses the previously reviewed read-only
memoryview optimization for its unchanged composite parsing algorithm; it is not
used to claim a stock-fontTools decoding benchmark.

## Reproduction

Set up the existing dependency/toolchain environment as in W2.8's COMMANDS.txt.
Use the final all-feature CLI and freshly built `woff_check` example. Run:

1. `tests/crosscheck/phase2_prepare.py --archive PATH --work NEW_DIRECTORY`
2. Build a separate unchanged copy of reference/tong-pixel-sans/tools/export-rs;
   keep its sources and target outside the protected package tree.
3. `tests/crosscheck/phase2_bitmaps.py --help` and `phase2_vectors.py --help`
   describe their source, binary, scratch-directory and result arguments.
4. Build complete native/reference smoke outputs with
   `benches/phase2_exports.py --profile quality5 --runs 1` and
   `benches/phase2_reference.py --runs 1`. These are functionality checks only.
5. Run `tests/crosscheck/phase2_exports_validate.py` on those complete exports.
6. After every correctness/build job finishes, run `benches/phase2_exports.py`
   and `benches/phase2_reference.py` with the default five repetitions, then
   `benches/phase2_fonttools.py` for paired five-run current-code comparisons.
   Keep timings separate from correctness and other benchmarks.

The whole export is 84 files: twenty BDF copies plus the sixty-four encoded
outputs enumerated by PLAN §10 (20 PCF, 20 OTB, 8 TTF, 8 OTF, 8 WOFF2).
BDF building from glyph sources is outside both engines' benchmark intervals;
input BDF parsing, variant planning, encoding and output I/O are included.
The native benchmark uses bounded parallel CLI processes pending Phase 6's batch
command. This is disclosed rather than claiming that the future batch CLI exists.

## Resource interruption and restart rule

The initial Python vector oracle was killed by the OS after fifteen completed
cases, while another large fontTools oracle and RAM-backed scratch exports were
active. This is recorded in `vectors-interruption.json`, not counted as an
accepted run or a native failure. Completed input/binary/output identities are
retained. The harness now supports `--resume` with matching binary/tool versions,
source hashes, output hashes and the protected-input manifest. It releases the
reference builder before loading the two comparison fonts. Heavy fontTools
oracles run serially; only duplicate scratch copies created by this acceptance
run were removed. Original inputs, committed evidence and delivered archives
were not deleted.

## Later user-directed performance work

The baseline correctness/benchmark evidence above is retained as a checkpoint.
The user then explicitly emphasized maximum practical speed and multicore use.
Subsequent production optimization is documented separately under
`reports/phase2-optimization/`; it must pass fresh affected/final gates before
main integration. The baseline's unchanged-source statements do not claim that
those newer candidate sources have already been accepted. No W3 work began.
