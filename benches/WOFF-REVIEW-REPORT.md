# WOFF2 final-code review measurements — 2026-10-03

Code: `2460412662b6a924b9a7c1faf74ac6b7e0cdeddf` (mandatory conformance fixes).
The unchanged Rust code is later integrated into main. Full raw samples, binary
fingerprint and machine description are in `results/woff-review.json`.

Eleven new groups have five wall-clock samples each. Input/output I/O stays
outside the timer. No reviewer-owned build, correctness or fuzz job ran during
these groups. Shared, unpinned VM scheduling still limits generalization.
Machine: Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; 9 visible CPUs; MemTotal:       10206504 kB.

## Affected single-font output strategies

| Strategy | Median s | Min–max s | Bytes |
|---|---:|---:|---:|
| native-Tong-SC-square-q5-t1-x1 | 0.240136 | 0.227035–0.243795 | 1,045,860 |
| native-Tong-SC-square-q8-t1-x1 | 0.535249 | 0.495470–0.570914 | 969,772 |
| native-Tong-SC-square-q9-t1-x1 | 0.813667 | 0.802104–0.828068 | 976,344 |
| native-Tong-SC-square-q11-t1-x1 | 13.653643 | 13.568940–13.992488 | 842,596 |
| native-Tong-SC-square-q11-t2-x1 | 7.088079 | 6.994662–7.125487 | 844,404 |
| native-Tong-SC-square-q11-t4-x1 | 5.954203 | 4.720394–6.434259 | 842,508 |
| native-Tong-SC-square-q5-t1-x0 | 0.215897 | 0.212240–0.253156 | 1,045,860 |
| native-Tong-SC-dot-q5-t1-x0 | 0.341015 | 0.310870–0.352467 | 1,112,260 |

Final CFF square q11: four intra-font threads are 2.29× faster in this segment, with -0.010% bytes.
The required default remains quality 11 and one intra-font thread. CFF q8 is
still smaller than q9; compression quality does not guarantee monotonically
smaller output.

## Final mixed-font batches, quality 5

Input order: SC square, SC dot, SC square, SC dot. One intra-font thread.

| Jobs | Median s | Min–max s | Aggregate bytes |
|---:|---:|---:|---:|
| 1 | 1.222678 | 1.104710–1.240343 | 4,144,216 |
| 2 | 0.976215 | 0.705377–1.033832 | 4,144,216 |
| 4 | 0.428863 | 0.386600–0.575361 | 4,144,216 |

Four jobs give 2.85× observed throughput with identical aggregate output sizes.
Input-order and distinct-font equality also remain directly tested in Rust.

## Unchanged output proof and historical comparisons

Seven transformed TrueType output strategies were regenerated once with final
code and exactly match their original candidate bytes: full-SC dot q5/q8/q9,
q11 with 1/2/4 threads, and Latin dot q11. These checks are not additional timing
samples. Their earlier same-input timing comparisons remain relevant historical
measurements of byte-identical algorithms.

CFF/null output changes are real: CFF q11 single-thread is now 842,596 bytes,
versus candidate 842,840. The old 29-group report is preserved separately in
`WOFF-REPORT.md`; affected byte counts and quality-11 mixed-batch timings remain
labeled candidate observations. No new final-code quality-11 batch speed ratio
or within-segment C++/fontTools comparison is inferred from unlike runs. The
earlier result that Rust is not universally faster remains explicit.

## Correctness

All 15 saved native outputs (eight newly timed single-font outputs plus seven
unchanged-output regressions) pass independent stock Brotli/table-stream checks
against final full-SC validation or exact null-transform tables, including the
mandatory head flag. The Latin font additionally passes every-glyph fontTools
and FreeType comparison. The three batch timing groups retain equal aggregate
bytes. See `reports/w2.5-review/benchmark-verification.json`.

Full-SC final-code glyph/table/pixel/shaping validation and independent Google
decoder checks pass. The full Tong export ≤40-second target remains a later
phase-2 integration measurement.
