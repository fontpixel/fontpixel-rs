# W2.5 checkpoint 3 — 2026-10-03

W2.5 correctness acceptance is complete; performance measurement is running.
No W2.6 work or phase-2 completion claim.

- scripts/check.sh: 578 passed test invocations, all other gates passed.
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
