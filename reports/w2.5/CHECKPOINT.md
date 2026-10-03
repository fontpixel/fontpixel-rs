# Final accepted W2.5 checkpoint — 2026-10-03

W2.5 is complete on branch `w2.5-woff`, based on main
`49fffd2bee1bf2983b17d17c2d5936aab3d14223`. The coordinator's main-branch merge
is separate. Phase 2 is not complete; W2.6 was not started; nothing was pushed.

Read `reports/w2.5.md` for the complete acceptance report and
`benches/WOFF-REPORT.md` for all 29 five-sample performance groups. The source
bundle contains every branch/history, all code, format decisions, repeatable
crosschecks, benchmark scripts, raw observations and proof logs.

- Full scripts/check.sh: 580 passing test invocations; all gates pass.
- Full parity: 67 exact charset tables and all 3292 cases; 0 NEW differences.
- Strict rustdoc and woff-only/woff2-only Clippy pass.
- 264 external cases / 192,088 generated-glyph checks; 23 timed outputs verified.
- Both full SC fonts verified through all-glyph comparisons and independent
  official Google WOFF2, fontTools, FreeType, OTS and HarfBuzz paths.
- Final WOFF1 ASan: 3,473,903 executions / 611 s / cov 1039 / ft 3398, no crash.
- WOFF2 ASan: 56,422 executions / 611 s / cov 3831 / ft 18103, no crash.
- All 690 protected files unchanged. Original recovery records remain intact.
- WOFF1 physical table order and signature validity boundaries are fixed and
  tested byte-for-byte. WOFF2 glyf/loca and optional hmtx transforms, quality,
  independent-font and legal intra-font parallelism are implemented.
- Default q11 is retained. Explicit parallelism and lower-quality trade-offs
  are measured honestly; stock C++ can outperform Rust on the same input.

Known integration boundaries remain documented: standalone sfnt containers,
not arbitrary FontSet rasterization; collections/native registry/CLI integration
belong to W2.7. No XML-schema or cryptographic-signature validation claim.

Requested worker gpt-6-astra/xhigh; actual runtime visibility and fast mode:
无法核实. Intermediate infrastructure interruptions and the isolated,
verification-only fontTools memoryview adapter are explicitly documented.
