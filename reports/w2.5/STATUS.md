# W2.5 acceptance status (before performance runs)

Source frozen at fda181f plus test-only standalone full-SC crosscheck driver.
W2.6 and subsequent work have not started.

- `scripts/check.sh`: pass, 578 passed test invocations across the mandated
  feature/workspace configurations, including 17 new WOFF tests.
- Independent `woff`-only and `woff2`-only strict Clippy: pass.
- API documentation with `RUSTDOCFLAGS=-D warnings`: pass.
- Full parity after source freeze: pass, 0 NEW differences; 67 exact charset
  tables, 2000 random (35,941 operations), 1000 random_all (17,588), 41 probes
  (89), 251 corpus files (5221). Existing 6 random_all + 1 probe exclusions
  remain unchanged. The earlier run had a source-change warning; only
  `parity-final.log` is acceptance evidence.
- Full independent crosscheck: 264 cases / 192,088 generated-glyph checks,
  including all Latin samples and 256 evenly selected glyphs from each regional
  BDF, four outline combinations, quality 11/5, single/parallel/null transforms,
  both-direction fontTools coding, OTS, FreeType, HarfBuzz, all bitmap fixtures,
  hints/explicit bounds/affine composites/overlap.
- Additional installed webfonts: 7 faces, 14,903 glyphs, all checks pass.
- First fuzz pass: WOFF1 2,152,708 executions (cov 1013, ft 3350, 611 s);
  WOFF2 53,169 executions (cov 3787, ft 17789, 611 s), no crashes.
  Another 610+ second pass is running against the frozen source.
- Full-SC square CFF: 38,654 glyphs, all independent glyph/render/shaping checks
  passed. Full-SC dot TrueType verification is still running.
- Protected input SHA-256 verification: all 690 files unchanged.
- Performance measurement is pending the completion of other CPU-heavy jobs.
  `benches/woff.py` measures five-run medians on full-SC 5.6 MB CFF square and
  18.6 MB TrueType dot inputs. Native pure-Rust, fontTools/Google Brotli and
  Tong's exact woff 0.6.3 call are prepared; lower quality, multi-font parallel,
  and legal single-font Brotli parallel modes are included.

## Final correctness results

The final source-frozen fuzz passes completed cleanly (611 seconds each):
WOFF1 2,330,056 executions, cov 1024/ft 3374, peak 427 MiB;
WOFF2 56,422 executions, cov 3831/ft 18103, peak 281 MiB.
Both full-SC fonts passed all assertions using the proven verification-only
memoryview adapter and independent official Google WOFF2 decoder. See
`stock-full-sc-censoring.txt` and `oracle-zero-copy.json` for the unchanged stock
run observation and byte-identical adapter evidence. The performance suite starts
only after all correctness/fuzz/build tasks have terminated.

Final retained WOFF1 reader fuzz: 3,473,903 executions, 611 s, cov 1039 / ft 3398, no crash. Final acceptance is summarized in ../w2.5.md.
