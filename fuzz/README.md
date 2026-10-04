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

## PCF

The `pcf` target exercises actual registry/gzip decoding, checked TOC/table parsing,
metrics, encodings, names, properties, optional accelerators and ink metrics,
then writes parsed models in default and mixed-order pad-8/scan-4 layouts and
reads the default output back. It uses the same 64 KiB input/table/decompressed,
256 glyph, 65,536 pixel and 4 MiB model budgets. Seed with the synthetic PCF oracle:

```sh
python3 -B - <<'PY'
import json,gzip,pathlib
p=pathlib.Path('fuzz/corpus/pcf');p.mkdir(parents=True,exist_ok=True)
cases=json.loads(gzip.decompress(pathlib.Path('crates/fontpixel/tests/data/pcf_reference.json.gz').read_bytes()))
for i,c in enumerate(cases):
    if 'pcf' in c:
        b=bytes.fromhex(c['pcf'])
        if len(b)<=65536:(p/f'oracle-{i:04}').write_bytes(b)
PY
ASAN_OPTIONS=detect_leaks=0 cargo +nightly fuzz run pcf -- \
  -max_total_time=610 -timeout=10 -rss_limit_mb=2048 -max_len=65536 -detect_leaks=0
```

The reviewed reader run and earlier exploratory 611-second runs are reported in
`reports/w1.3.md`. LeakSanitizer is disabled for the same documented sandbox
ptrace restriction; AddressSanitizer remains enabled. No security settings changed.

## YAFF

The `yaff` target checks UTF-8 structure, metadata JSON, gray/mono/empty rasters,
registry gzip and all resource budgets. Successful parses are written and read
again, asserting exact native model equality when the bounded reread succeeds.
It uses the same 64 KiB input/table, 256 glyph and 4 MiB accounting limits above.

```sh
mkdir -p fuzz/corpus/yaff
cp tests/data/yaff/*.yaff fuzz/corpus/yaff/
ASAN_OPTIONS=detect_leaks=0 cargo +nightly fuzz run yaff -- \
  -max_total_time=610 -timeout=10 -rss_limit_mb=2048 -max_len=65536 -detect_leaks=0
```

An initial exploratory 611-second run reached LeakSanitizer's known sandbox
ptrace failure only at shutdown (empty artifact); the final run disables only
that unsupported leak detector, retains AddressSanitizer and is reported in
`reports/w1.6.md`.

## sfnt common layer

The `sfnt` target exercises checked envelope parsing/reassembly and standalone
name/cmap/post/OS2/gasp/metric decoders, plus successful table roundtrips. Counts
are limited to 256 glyphs/names, 4,096 expanded mappings and 64 tables; bytes to
64 KiB per input/table. Seed using the common integration example and its raw
tables, plus the checked-in malformed regression:

```sh
cargo run --locked -p fontpixel --features compat-bdfparser --example sfnt_check -- /tmp/sfnt-seeds
mkdir -p fuzz/corpus/sfnt
cp /tmp/sfnt-seeds/common.ttf tests/data/sfnt/truncated-format4.cmap fuzz/corpus/sfnt/
python3 -B - <<'PY'
from fontTools.ttLib import TTFont
from pathlib import Path
f=TTFont('/tmp/sfnt-seeds/common.ttf')
for tag in ['cmap','name','post','OS/2','hhea','hmtx','vhea','vmtx','gasp']:
    Path('fuzz/corpus/sfnt',tag.replace('/','_')).write_bytes(f.getTableData(tag))
PY
ASAN_OPTIONS=detect_leaks=0 cargo +nightly fuzz run sfnt -- \
  -max_total_time=610 -timeout=10 -rss_limit_mb=2048 -max_len=65536
```

AddressSanitizer remains enabled; LeakSanitizer's unsupported ptrace shutdown
check is disabled under the already documented sandbox limitation. The initial
exploratory crash, fix and final clean-run counts are reported in `reports/w2.1.md`.

## W2.2 readers

`glyf` exercises borrowed sfnt extraction, short/long loca, bounded direct glyph
records, graph expansion, centre-fill rasterization and canonical semantic
roundtrips. Its per-case budgets cap points, components, visits and table/model
sizes. `compat_ttf` exercises complete SPEC `read_ttf` both with explicit pixel
size and the BDFP-only route. Seed both from `tests/data/compat/*.ttf`.

```sh
ASAN_OPTIONS=detect_leaks=0 cargo +nightly fuzz run glyf -- \
  -max_total_time=610 -timeout=10 -rss_limit_mb=2048 -max_len=65536
ASAN_OPTIONS=detect_leaks=0 cargo +nightly fuzz run compat_ttf -- \
  -max_total_time=610 -timeout=10 -rss_limit_mb=2048 -max_len=65536
```

LeakSanitizer is disabled only because this environment cannot support its
exit-time ptrace check; AddressSanitizer remains active. Record each run's `DONE`,
exit status, execution count and coverage counters; incomplete runs do not meet
the ten-minute requirement. No generated corpus or binaries are committed.

## HEX and PSF (W3.1)

`hex` exercises three explicit heights (8/16/24), comments, Unicode scalars,
resource bounds and exact native read/write/read equality. `psf` covers both
headers, arbitrary widths, aliases, sequence-only/multiple sequences, strict
Unicode grammar and both writers. No-loss roundtrips must preserve the native
model exactly. Seeds are generated from the independent fixtures described in
`crates/fontpixel/tests/hex_psf.rs`; no licensed external font is required.

```sh
python3 -B tools/gen_console_cases.py
ASAN_OPTIONS=detect_leaks=0 cargo +nightly fuzz run hex -- \
  -max_total_time=610 -timeout=10 -rss_limit_mb=1536 -max_len=65536 -detect_leaks=0
ASAN_OPTIONS=detect_leaks=0 cargo +nightly fuzz run psf -- \
  -max_total_time=610 -timeout=10 -rss_limit_mb=1536 -max_len=65536 -detect_leaks=0
```

Both targets use 64 KiB input, 4 MiB aggregate model accounting and 65,536
pixels/glyph; HEX caps records at 8 KiB and 256 glyphs, PSF tables at 64 KiB and
512 glyphs. Writes cap at 256 KiB. Final elapsed time, executions, coverage,
exit status and identical production-source fingerprints are recorded in the
W3.1 report/evidence. The same sandbox-only LeakSanitizer exception applies;
AddressSanitizer and Rust overflow/panic checks remain enabled.

## Windows FNT and FON (W3.2)

`fnt` exercises v1/v2/v3 reads and all three writers, asserting readable output
and native model equality for no-loss writes. `fon` exercises NE/PE resources,
NE writes and every returned font. Generate independent original seeds with
`python3 -B tools/gen_windows_cases.py`. Use the normal nightly cargo-fuzz flow
with each target, `-max_total_time=610 -max_len=65536 -timeout=10
-rss_limit_mb=1536 -detect_leaks=0`, and `ASAN_OPTIONS=detect_leaks=0` for the
same sandbox-only LeakSanitizer exception. Each target limits inputs/tables to
64 KiB, glyphs to 512, resources to 64 and aggregate working data to 8 MiB.
Final counts, coverage and production-source hashes are in W3.2 evidence.

## Image, C, KBITX, PBF and HBF (W3.3–W3.5)

Generate original synthetic seed corpora with `fuzz/seed_image_c.py`,
`fuzz/seed_kbitx.py`, `fuzz/seed_pbf.py` and `fuzz/seed_hbf.py`. Corpus mutations
and artifacts remain ignored. Image/C use safe in-memory resource resolvers;
HBF inputs contain a header/bitmap pair. XML, wire layouts, gray/mono,
roundtrip stability, lossless model equality and same-limit reader/writer
contracts are exercised by the corresponding targets.

```sh
cargo +nightly fuzz run image --no-default-features --features image-c -- \
  -max_total_time=610 -timeout=10 -rss_limit_mb=1024 -max_len=65536
cargo +nightly fuzz run c --no-default-features --features image-c -- \
  -max_total_time=610 -timeout=10 -rss_limit_mb=1024 -max_len=65536
cargo +nightly fuzz run kbitx --no-default-features --features kbitx-only -- \
  -max_total_time=610 -timeout=10 -rss_limit_mb=1024 -max_len=65536
cargo +nightly fuzz run pbf --no-default-features --features pbf-only -- \
  -max_total_time=610 -timeout=10 -rss_limit_mb=1024 -max_len=65536
cargo +nightly fuzz run hbf --no-default-features --features hbf-only -- \
  -max_total_time=610 -timeout=10 -rss_limit_mb=1024 -max_len=65536
```

The final local campaigns need no LeakSanitizer override; their exact commands,
source identities, successful DONE records and counts are in phase 3 evidence.
Earlier interrupted campaigns and build failures are not acceptance evidence.
