# W2.6 native variants/subset timings

Five wall-clock samples per case; final code `1dd7f84`.

Environment: Linux-6.18.44-x86_64-with-glibc2.41; Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; 9 logical CPUs; MemTotal:       10206504 kB; cgroup memory max unavailable.

Every generation reads the full Large SC/TC/JP/KR/Latin BDFs, including TC/HK language aliases. No comparison with different workloads or whole-phase speed claim.

| Case | Median seconds | Bytes |
|---|---:|---:|
| variants-square-ttf | 4.014547 | 9,345,980 |
| subset-square-ttf | 0.169947 | 5,988 |
| variants-square-otf | 3.173716 | 9,111,600 |
| subset-square-otf | 0.033239 | 484,488 |
| variants-dot-ttf | 1.588547 | 22,649,056 |
| subset-dot-ttf | 0.161168 | 10,228 |
| variants-dot-otf | 1.740891 | 14,181,516 |
| subset-dot-otf | 0.029420 | 486,276 |

All five repetitions produce identical bytes. Eight retained benchmark fonts pass fontTools inspection and OTS. CFF retains unused strings/subroutines/FD dictionaries for fidelity, so its small text subsets are roughly 484–486 KB; TrueType subsets are roughly 6–10 KB. These are measurements, not a claim that the complete 84-file ≤40-second phase goal is reached.

Raw samples/hashes: `results/variants.json`. Driver: `variants.py`.
