# WOFF2 performance — W2.5, 2026-10-03

**Candidate measurements:** The tables below retain the original `cc5ba81`
measurements. Subsequent conformance review changed CFF and explicit null-transform
output by setting mandatory head.flags bit 11. Final-code measurements and byte
verification are recorded separately in `WOFF-REVIEW-REPORT.md`; historical
affected byte counts are not presented as final-code outputs.

All medians use five wall-clock samples, retained in `benches/results/woff-resumed.json`. Input reads/output writes are outside the timer. No other worker-owned build, fuzz or correctness job ran during timed groups. The VM is shared and unpinned; these are observed results, not universal speed guarantees.

Machine: Intel(R) Xeon(R) Platinum 8370C CPU @ 2.80GHz; 9 visible CPUs; MemTotal:       10206504 kB; Linux-6.18.44-x86_64-with-glibc2.41.
Rust 1.99.0, release profile; native brotli 9.0.0 / brotli-decompressor 6.0.1; Tong comparison uses its exact woff 0.6.3 C++ call; stock fontTools 4.66.1 and Google Brotli Python 1.2.0.

An executor control-session interruption occurred at 08:40 UTC after 18 complete groups. These groups are segment 0. Segment 1 restarted incomplete groups under the same reported CPU/RAM/kernel, using retained binaries whose SHA-256 fingerprints were recorded after restart; it also repeats native SC-dot q11 for a within-segment C++ comparison. No interrupted partial group is counted. `benchmark-pre-restart.json` and the final JSON preserve both provenance and raw samples. Exploratory `benchmark-before-batch-caps.json` is excluded from every table below.

## Inputs

| Input | Glyphs | sfnt bytes | SHA-256 |
|---|---:|---:|---|
| Tong-SC-square.otf | 38,654 | 5,585,888 | 9deb82c1107845387b67387ca1584c0c35484d32962e747af1c89913757ddb61 |
| Tong-SC-dot.ttf | 38,655 | 18,642,720 | cb73ddf52517491f9eacf87f0b2ab74e5797face051658645f92d1b7e33462e2 |
| Tong-Latin-dot.ttf | 3,647 | 683,452 | f13fb97d3236a10c6ff9972585afd97337f2ee84bc4d1055c7fec65e68ac31db |

Inputs use all glyphs from the supplied SC or Latin BDF, plus `.notdef` and (for dots) the shared dot glyph. CFF-square and TrueType-dot are PLAN §5.3’s recommended source choices. Full family/locl assembly is outside W2.5; the phase-2 whole-export ≤40-second target is NOT claimed here.

## Full SC square: native quality/thread trade-offs

| Method | Median s | Min–max s | Bytes | Segment |
|---|---:|---:|---:|---:|
| q5, 1 thread(s) | 0.218827 | 0.208056–0.278708 | 1,045,860 | 0 |
| q8, 1 thread(s) | 0.480441 | 0.474110–0.518458 | 969,772 | 0 |
| q9, 1 thread(s) | 1.065344 | 0.890806–1.338471 | 976,340 | 0 |
| q11, 1 thread(s) | 13.506953 | 13.197636–13.617710 | 842,840 | 0 |
| q11, 2 thread(s) | 7.488200 | 7.239432–8.164095 | 844,152 | 0 |
| q11, 4 thread(s) | 4.888546 | 4.722943–5.297190 | 842,540 | 0 |

| Method | Median s | Min–max s | Bytes | Segment |
|---|---:|---:|---:|---:|
| q5, null transforms | 0.218708 | 0.211730–0.229747 | 1,045,860 | 0 |

At q11, four intra-font threads deliver 2.76× the observed single-thread throughput; size changes by -0.036%. Quality 9 is 12.68× faster than q11 with +15.84% bytes. These ratios use the same input and segment.

## Full SC dot: native quality/thread trade-offs

| Method | Median s | Min–max s | Bytes | Segment |
|---|---:|---:|---:|---:|
| q5, 1 thread(s) | 0.424448 | 0.353347–0.492142 | 1,026,248 | 0 |
| q8, 1 thread(s) | 0.936009 | 0.769565–1.834692 | 859,264 | 0 |
| q9, 1 thread(s) | 1.065694 | 1.060065–1.102620 | 830,792 | 0 |
| q11, 1 thread(s) | 45.130121 | 44.827426–50.736110 | 669,740 | 0 |
| q11, 2 thread(s) | 24.199307 | 23.922410–32.011251 | 669,720 | 0 |
| q11, 4 thread(s) | 19.359796 | 14.896357–19.693724 | 670,048 | 0 |

| Method | Median s | Min–max s | Bytes | Segment |
|---|---:|---:|---:|---:|
| q5, null transforms | 0.324688 | 0.315987–0.346175 | 1,112,260 | 0 |

At q11, four intra-font threads deliver 2.33× the observed single-thread throughput; size changes by +0.046%. Quality 9 is 42.35× faster than q11 with +24.05% bytes. These ratios use the same input and segment.

## Same-input independent encoder comparisons

### Full SC CFF square, q11

| Method | Median s | Min–max s | Bytes | Segment |
|---|---:|---:|---:|---:|
| native Rust | 13.506953 | 13.197636–13.617710 | 842,840 | 0 |
| Tong / Google C++ | 12.498389 | 12.293598–12.581650 | 841,060 | 0 |
| stock fontTools | 13.784326 | 12.881283–14.996965 | 841,060 | 0 |

### Full SC TrueType dot, q11, post-restart segment

| Method | Median s | Min–max s | Bytes | Segment |
|---|---:|---:|---:|---:|
| native Rust recheck | 45.772158 | 45.114129–47.182421 | 669,740 | 1 |
| Tong / Google C++ | 34.134294 | 32.763018–34.257028 | 671,148 | 1 |

### Latin TrueType dot, q11, same-input three-way comparison

| Method | Median s | Min–max s | Bytes | Segment |
|---|---:|---:|---:|---:|
| native Rust | 1.404296 | 1.398078–1.423837 | 39,028 | 1 |
| Tong / Google C++ | 1.166106 | 1.104611–1.203648 | 39,256 | 1 |
| stock fontTools | 2.108960 | 2.092594–2.194203 | 39,016 | 1 |

The stock full-SC dot correctness pass was canceled after about 20 minutes without finishing its first fontTools/FreeType equivalence pass. That is a censored correctness-run observation, NOT an encoder/decoder benchmark or median. The verification-only memoryview adapter proved byte-identical on representative simple, composite, hinted, affine, explicit-bbox and overlap fixtures and then passed all full-SC checks, corroborated by the official C++ decoder. The adapter was never enabled for these performance measurements. Stock fontTools’ Latin-dot three-way comparison is a separate workload and is not mixed with SC speed ratios.

## Four independent full-font batch

Fixed input order/multiset: SC CFF square, SC TrueType dot, then both again. Intra-font threads are one. Batch guards are enabled (1024-font and 256 MiB aggregate input/output limits).

| Method | Median s | Min–max s | Bytes | Segment |
|---|---:|---:|---:|---:|
| q5, 1 job(s) | 1.150512 | 1.060735–1.170214 | 4,144,216 | 1 |
| q5, 2 job(s) | 0.743379 | 0.698662–0.772633 | 4,144,216 | 1 |
| q5, 4 job(s) | 0.564425 | 0.479961–0.703923 | 4,144,216 | 1 |
| q11, 1 job(s) | 125.077219 | 119.718873–133.245848 | 3,025,160 | 1 |
| q11, 2 job(s) | 74.509803 | 72.913831–79.669895 | 3,025,160 | 1 |
| q11, 4 job(s) | 49.641880 | 45.979069–55.261202 | 3,025,160 | 1 |

q5: four jobs are 2.04× faster than one in this VM, with identical aggregate output byte counts and deterministic input-order results.
q11: four jobs are 2.52× faster than one in this VM, with identical aggregate output byte counts and deterministic input-order results.

## Recommendations and boundaries

- Keep required default quality 11. Expose lower qualities explicitly: they greatly reduce latency but materially increase bytes for the dot font.
- Independent multi-font jobs preserve each file’s bytes. Tune job count to CPU and concurrent compressor working-memory capacity; input/output/count budgets also bound retained batch data.
- Single-font parallel Brotli is implemented using brotli’s legal single-stream chunk encoder, not concatenated independent streams. It changes compression decisions and may slightly increase or decrease size; fixed options remain deterministic.
- Quality is not a guarantee of monotonically smaller output: the CFF q8 result is smaller than q9 in this dataset. All observed sizes are retained.
- Avoid guessing that Rust is always faster: compare the same-input tables above. C++ and stock fontTools remain competitive on CFF q11; latency gains primarily come from explicit parallelism or lower quality.
- The benchmark covers webfont envelope coding. It does not establish the complete Tong eight-vector/PCF/OTB/locl pipeline target or native CLI/FontSet integration reserved for later work.

## Correctness of measured outputs

`tests/crosscheck/woff_bench_verify.py` independently decodes every native timed stream with stock fontTools/Google Brotli and requires exact transformed-block equality with the already verified full-font output (or original bytes for null transforms). C++/fontTools comparator outputs go through the official C++ decoder and every-glyph fontTools/FreeType comparisons. See `reports/w2.5/benchmark-verification.json`.

Reproduction: `benches/WOFF-README.md`, `reports/w2.5/COMMANDS.txt`, `benches/woff.py`, `benches/report_woff.py`. All protected sources remain unchanged.
