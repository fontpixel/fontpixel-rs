# W2.5 checkpoint 2 — 2026-10-03

Branch w2.5-woff from main 49fffd2bee1bf2983b17d17c2d5936aab3d14223.
Complete implementation and all scripts/check.sh gates pass. Fifteen WOFF tests
cover qualities, roundtrips, malformed streams, bounded allocation, metadata,
signature-loss reports, full triplet classes and hmtx arrays. This is still NOT
final acceptance: 10-minute fuzzes, full parity and five-median benchmark work are
running/pending.

Initial full Tong external pass verified all 28 TTF/CFF square/dot font cases,
then uncovered an oracle-harness error when recompiling bitmap tables. The harness
now compares original raw sfnt bytes for untouched tables, and the corrected
hint/overlap/affine/bbox plus all bitmap fixtures crosscheck passed 152 cases.
The full corrected crosscheck is running again, recording partial progress.

Two real compatibility fixes are committed: four-byte normalization for Google
and FreeType advisory-size handling; optional hmtx null transform for fontTools'
nonminimal long-metric-count recompilation bug. Both documented in decisions.

No phase-2 completion claim. Model requested gpt-6-astra/xhigh; fast mode 无法核实.
Reference/samples baseline contains 690 protected files. W2.6 has not begun.
