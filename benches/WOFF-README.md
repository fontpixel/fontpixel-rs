# Reproduce W2.5 WOFF2 measurements

`benches/woff.py` creates deterministic full-SC CFF square/TrueType dot inputs and
a smaller Latin-dot input. Every recorded row has five wall-clock samples, their
median, output bytes and settings. Input reads/output writes are outside timing.
Build `woff_check` with features `woff,woff2`; build `benches/woff-reference`
separately. The latter is precisely Tong export-rs's woff 0.6.3 API call. No C/C++
WOFF code/dependency is used by production fontpixel.

Use `--prepare-only` to create inputs; `--resume` continues an interrupted result.
`--segment-note` records the conditions of a measurement segment; on executor
restarts use a fresh result path copied from the last complete JSON checkpoint.
`--recheck-dot-q11` adds a fresh native full-SC-dot baseline so its C++ comparison
can stay in one post-restart segment. JSON checkpoint writes are atomic. Never
mix unlike source fonts in a ratio. Shared VM scheduling is unpinned; inspect
all five samples rather than assuming exact/universal timing ratios.

Full SC strategies: native qualities 5/8/9/11, intra-font quality-11 threads
1/2/4, quality-5 null-transform control, and C++ qualities 5/11. Full-SC CFF and
smaller Latin-dot also have stock fontTools quality-11 same-input three-way
comparisons. Multi-font jobs 1/2/4 encode a fixed four-font input sequence (SC
square, SC dot, then both again), qualities 5/11, one intra-font thread.

Only run timing after other worker-owned builds/fuzz/correctness jobs finish.
The verification-only fontTools memoryview adapter is NEVER enabled in timing.
Afterward run `tests/crosscheck/woff_bench_verify.py` to independently validate
all timed output bytes/glyphs. Exact commands: `reports/w2.5/COMMANDS.txt`.

Independent review corrected the mandatory head flag for CFF/null-transform
outputs. `benches/woff_review.py` remeasures all affected single-font strategies
and quality-5 multi-font groups with five samples, regenerates unchanged output
strategies and requires exact equality with the prior candidate, and keeps all
results separate. Its `--original-work`, `--work`, and `--output` paths prevent
any modification of original candidate timing evidence. See
`reports/w2.5-review/COMMANDS.txt` and `WOFF-REVIEW-REPORT.md`.
