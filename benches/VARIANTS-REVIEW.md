# W2.6 reviewed main timings

2026-10-03. Five wall-clock repetitions per case, after correctness, compilation and fuzzing completed.

Integrated main code: `472c45d5923de858745925aac06e249281a21ef9` (production change `5db6a0e`).
Environment: Linux-6.18.44-x86_64-with-glibc2.41; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; 9 logical CPUs; MemTotal:       10206504 kB.

Each generation reads all five supplied Large regional/script sources, including the TC/HK alias. These are end-to-end CLI timings, not isolated outline timings.

| Case | Median seconds | Bytes |
|---|---:|---:|
| variants-square-ttf | 3.286718 | 9,345,980 |
| subset-square-ttf | 0.177956 | 5,988 |
| variants-square-otf | 3.176537 | 9,111,600 |
| subset-square-otf | 0.027591 | 484,488 |
| variants-dot-ttf | 1.523139 | 22,649,056 |
| subset-dot-ttf | 0.145443 | 10,228 |
| variants-dot-otf | 1.288964 | 14,181,516 |
| subset-dot-otf | 0.028990 | 486,276 |

Every five-run group is byte-identical, and all eight outputs also match the original benchmark bytes. Independent fontTools and OTS validation is recorded in `../reports/w2.6-review/benchmark-validation.json`.

CFF retains unused strings/subroutines/Font DICTs for bytecode and metadata fidelity. Text subsets remain 484,488/486,276 bytes for CFF versus 5,988/10,228 bytes for TTF. Further dead-data pruning remains an optimization opportunity.

No speedup is inferred from differences versus earlier environment runs. The complete phase-2 84-file export and ≤40-second target are not established here.

Raw samples, output hashes and binary identity: `results/variants-review.json`. Reproduce using `variants.py`.
