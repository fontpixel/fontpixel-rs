# BDF fuzzing

`bdf` exercises the real BDF registry, gzip decoding, parser, source validator and
writer. Limits: 64 KiB input/decompressed/table, 256 glyphs, 65,536 pixels/glyph,
4 MiB model/input total and 1 MiB output. Seed from the committed oracle and real
small fixtures; corpus/artifacts/targets are excluded from Git.

```sh
cargo install cargo-fuzz --locked
rustup toolchain install nightly --profile minimal
python3 - <<'PY'
import json,pathlib
p=pathlib.Path('fuzz/corpus/bdf');p.mkdir(parents=True,exist_ok=True)
for i,c in enumerate(json.load(open('crates/fontpixel/tests/data/bdf_reference.json'))):
    (p/f'oracle-{i:04}').write_text(c['source'])
PY
ASAN_OPTIONS=detect_leaks=0 cargo +nightly fuzz run bdf -- \
  -max_total_time=610 -timeout=10 -rss_limit_mb=2048 -max_len=65536 -detect_leaks=0
```

Leak detection is disabled only because exit-time LeakSanitizer requires ptrace
unavailable in this sandbox. Address sanitizer remains active. The first run
finished 611 seconds of input fuzzing but failed during LeakSanitizer shutdown;
its empty artifact is not a parser crash. Final results, executions and coverage
are in `reports/w1.2.md`. This target does not fuzz unimplemented formats.

Check isolated fuzz dependencies with:
`cargo deny --manifest-path fuzz/Cargo.toml --locked --config fuzz/deny.toml check`.
The fuzz policy alone permits libFuzzer's additional NCSA license; production
policy remains unchanged.
