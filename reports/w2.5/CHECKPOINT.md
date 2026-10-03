# W2.5 checkpoint 4 — 2026-10-03

W2.5 correctness acceptance is complete; performance measurement is running.
No W2.6 work or phase-2 completion claim.

- scripts/check.sh: 579 passed test invocations, all other gates passed.
- Full frozen-source parity: 0 NEW differences, 67 exact tables, all 3292 cases;
  previous 7 known differences remain unchanged.
- Strict rustdoc, woff-only/woff2-only Clippy, production/fuzz/reference deny pass.
- 264 crosscheck cases, 192,088 generated glyph checks, all applicable
  fontTools/OTS/FreeType/HarfBuzz assertions pass.
- 7 installed independent webfonts, 14,903 glyphs, pass.
- Full SC square CFF (38,654 glyphs) and dot TrueType (38,655 including shared dot)
  pass all glyph/table/FreeType/OTS/HarfBuzz assertions. The fontTools component
  stream uses the proven byte-identical read-only memoryview adapter only for
  full-font verification. Stock run censoring and exact adapter are documented.
  Google's official C++ WOFF2 decoder independently passes all full-font checks.
- Final ASan fuzz runs: WOFF1 2,330,056 executions / cov 1024 / ft 3374 / 611 s;
  WOFF2 56,422 executions / cov 3831 / ft 18103 / 611 s. No crash, both exit 0.
- All 690 protected input files unchanged.

Only the isolated five-run timing suite and resulting final report/checkpoint
remain. It uses stock fontTools, native pure Rust and Tong's exact woff 0.6.3
path. Full-SC native/C++ data and smaller same-input Latin three-way comparisons
are kept separate. No adapter timing is labeled as stock performance.

Model requested gpt-6-astra/xhigh; actual runtime settings and fast mode 无法核实.

Aggregate batch guards are now committed and re-accepted at 24a032e: default
1024 fonts, 256 MiB aggregate input and separately retained output. The extra
ordering/budget test passes. scripts/check.sh, full parity, minimal-feature
Clippy and strict rustdoc were rerun successfully after this change. Reader
algorithms and serialized font bytes are unchanged. All timing samples are
being rerun with this final binary; earlier observations are kept separately.

## Runtime interruption at 08:40 UTC

The executor temporarily disconnected and its existing benchmark session could
not be resumed (unknown session ID). Files and HEAD are preserved. Eighteen
complete five-sample benchmark groups are retained verbatim in
`reports/w2.5/benchmark-pre-restart.json`; the in-progress C++ full-SC-dot q11
measurement has no complete result and is not counted. Ten planned groups remain.
Recovery uses a separate result path to avoid uncertain producer writeback and
records the post-restart environment as a new measurement segment.

## Post-restart timing milestone

All per-font native/C++/stock-fontTools comparisons and the quality-5 batch
matrix are complete: 26 complete five-sample groups are saved in
`benches/results/woff-resumed.json`, with measurement-segment metadata. The
post-restart full-SC-dot q11 comparison is native 45.772158 s / 669,740 bytes
versus C++ 34.134294 s / 671,148 bytes; Rust is slower here. On identical Latin-dot
input, q11 medians are native 1.404296 s, C++ 1.166106 s, stock fontTools 2.108960 s.
Only three quality-11 batch groups and final post-timing output verification /
report compilation remain. Existing fonts are reused; no large duplicate inputs.

## Final WOFF1 normative correction and complete measurements

All 29 timing groups are complete (five samples each), including a post-restart
native baseline repeat. Quality-11 four-font jobs 1/2/4 measured
125.077219 / 74.509803 / 49.641880 seconds with identical 3,025,160 aggregate bytes.
`benches/WOFF-REPORT.md` is generated from those complete samples.

Final normative review found and corrected WOFF1 §6 physical-table ordering.
The new bounded order-preserving assembler exactly reconstructs well-formed
sfnt bytes, including reverse-order layout and DSIG; invalidated signatures and
nonzero hidden data are explicitly reported. Full `scripts/check.sh` now passes
580 test invocations and full parity again has 0 NEW differences. The repeated
264-case external pass adds physical-order and exact sfnt-byte assertions.
WOFF1's final 611-second fuzz and all timed-output verification are still running.
WOFF2 algorithms/serialized bytes/timing paths are unchanged by this WOFF1 fix.
