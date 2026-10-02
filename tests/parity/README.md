# Python ↔ Rust SPEC-v3 differential suite

This is the complete default `reference/bdfparser-js/tools/parity` suite adapted
for `fontpixel-parity`. No operation family, corpus extension, seed or default
count was removed. The Rust executable never imports/spawns Python or JS.

## Run

```
cargo build --locked -p fontpixel-cli --release --features parity --bin fontpixel-parity
PYTHONDONTWRITEBYTECODE=1 python3 tests/parity/run_all.py
```

Defaults: 67 charset tables, 2,000 seeded `random` cases, 1,000 `random_all`, 41
probes, and every distinct BDF/PCF/PCF.GZ/TTF in the supplied three corpus roots
(251 files in the task package). Corpus files over 1.5 MB receive the original
reduced script, unchanged from upstream. Python >=3.10 suffices; no Bun or Node
is needed. Use `BDFPARSER_PY=/path/to/bdfparser/src` and
`BDFPARSER_JS=/path/to/bdfparser-js` when references are elsewhere. Output is
under ignored `tests/parity/out/`, or `--out DIR`. `--rust CMD` chooses the runner.
`--seed`, `--n`, `--n-all`, `--jobs` and the original diagnostic skip flags remain.
A release runner is required for meaningful performance measurements.

The runner takes `CASES.jsonl OUT.jsonl [--full] [--only IDs]`, or
`--tables SPEC.json OUT.json`. Default output shrinks canonical texts above 600
characters to SHA-256; byte results retain byte length/hash, and `--full` adds
base64. JSON escapes are exact UTF-16 code units, including isolated surrogates.
Font state is compared after successful mutations **and exceptions**. Both
implementations raising counts as agreement, as in the original harness.
`compare.py` retains the original taint/cascade analysis and the wire field names
`py`/`js`; the latter means Rust here. No tolerance or hash comparison is relaxed.

```
python3 tests/parity/minimize.py tests/parity/out/random.jsonl r1-00042
python3 tests/parity/run_py.py tests/parity/out/random.jsonl /tmp/py.jsonl --full --only r1-00042
target/release/fontpixel-parity tests/parity/out/random.jsonl /tmp/rs.jsonl --full --only r1-00042
```

## Historical finding assessment

- R01/R02: exact empty-render and stale-writeback regressions; must agree.
- K02: zero-width todata results are exact; must agree.
- K03: negative input sizes and partial font-transform effects; must agree.
- K04: clipped, size-preserving overlay; must agree.
- K05/K08: signed-32-bit field syntax/property ranges and version syntax; must agree.
- K06a–d: insertion order, unusual names and COMMENT handling; must agree.
- K07: ASCII BDF whitespace boundaries; must agree.
- K09: removed attached handles do not remove replacements; must agree. A fresh
  detached clone removes by code, matching the executable Python reference.
- K10/K13/K14/K17: exact generated codecs, GB18030 and charset canonicalization;
  must agree. No runtime TextDecoder or host charset approximations are used.
- K11: **not applicable to Rust**. Arbitrary-width bitmap row integers remain
  exact, including beyond u64. The original precision probe now expects agreement.
- K12: applies only to invalid ragged bitmaps. Rust refuses ragged construction or
  replacement to preserve the shared model's rectangle invariant. Matchers require
  the actual ragged trigger and the corresponding validation error. Subsequent
  tainted-register differences are reported as cascades, exactly as upstream.
- K15: explicit PCF integer/resource guards before allocation; must agree (S07).
- K16: vertical empty-input offset rules; must agree.
- S01–S12: all strict regressions, including BOM names, big property values,
  unmappable Unicode and invalid hex rows.

`tools/gen_compat_goldens.py` regenerates the checked-in small synthetic reference
fixtures. Offline Rust tests cover 104 original probe/random scripts, literal
Mini BDF/SVG goldens, eight exact TTF byte/round-trip pairs, hole/pinch outlines,
large integers, native-model ownership and identity-safe handles. They run from
`scripts/check.sh` through all-feature Cargo tests without external references.
