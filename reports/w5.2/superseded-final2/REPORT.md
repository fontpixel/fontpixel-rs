# W5.2 — Johab 8/4/4 hanfont, QuickDraw II and BinSCII

Both codecs implement native reading and writing with typed options and registry
traits. The coordinator owns shared Cargo/features/registry/CLI changes. Source
specifications preceded implementation in `docs/formats/{hanfont,iigs}.md`.
The final owned-file hashes, validation commands and external source hashes are
recorded in `freeze.json`, `commands.json` and `sources.json` in this directory.

| Format | Implemented representation |
|---|---|
| hanfont | Headerless 360-slot Johab 8/4/4, fixed 16×16 MSB-first components; explicit component or all-11,172-modern-syllable projection |
| IIgs | Pascal MacRoman family, QuickDraw II 1.1/1.5 headers, actual 32-bit offset extension, byte character range, missing symbol, native signed/unsigned metrics, unswapped raster bytes |
| BinSCII | One complete C8/0000 font data fork; custom alphabets, header/data CRC-16/XMODEM, arbitrary segment order, overlap/gap/conflict validation, deterministic 12 KiB segments |

Hanfont has no content detector. A length match alone must not steal other binary
formats. Other Korean layouts were investigated: Hanterm's 529 components and
Unifont's 6/3/1 are different arrangements; vendor DOS prefixes and unknown
layouts are not guessed. Native components include every filler. Syllable
composition skips an absent final and preserves unused filler artwork in source
bytes. Writing accepts 360 unique native components or a complete unchanged
syllable projection backed by verified native components. Edited syllables that
cannot be reproduced fail explicitly; no inverse decomposition is invented.

IIgs follows the Apple table address rules, including legal padding before the
location/width tables. Both versions retain native bytes, unknown header words,
ignored fontType bits and trailing data. Native replay is conditional on actual
model equality and requested version. Canonical writing preserves baseline and
bearings without clipping, increases a stale fbrExtent after glyph edits, and
reports native-record or geometric changes. The data fork does not store charset
identity; explicit reader declarations remain legacy codes and loss is reported
when that declaration cannot survive serialization. BinSCII keeps the original
wrapper as collection provenance and reuses it only when the decoded data fork
matches. Bare output reports removal of existing wrapper provenance.

The common audit covers every supplied public model field, including optional
font/glyph metrics, all names, aliases/order/stable IDs, default glyph identity,
charset, sequences, properties and headers. Newly acquired format defaults or
new native provenance are not themselves information loss. Strict registry writes
reject the resulting Loss reports. Gray8 requires an explicit threshold.
Limits account cumulatively for retained bytes, model/index estimates, output,
reread and reports; every successful writer reparses its output under the same
limits. IIgs segment lookup is O(segments × log(segments)); raster processing is
linear in bytes/pixels. Hanfont composition uses bytewise OR and indexed glyph
validation, O(glyphs × log(glyphs) + raster bytes), without glyph-pair scans.

## Validation and independent evidence

All 26 integration tests pass: hanfont 9 and IIgs 17. Each format independently
passes all-targets Clippy with `-D warnings` and rustdoc with
`RUSTDOCFLAGS='-D warnings'`. Combined tests/example Clippy and owned-file
rustfmt checks pass. `commands.json` records commands, log paths and exits.
Tests cover full native/syllable replay, nonblank fillers, initial G/K and every
vowel/final shape group, edited/incomplete components, all IIgs truncations,
extended actual offsets, relocated tables, signed bearings, negative ascent/descent/leading and rectangles outside the baseline, Gray8, complete
metadata loss, registry strict mode, exactly minimal cumulative writer budgets,
CRC failures, reordered segments, missing/duplicate/overlapping segments and
custom alphabets with CR-only lines.

`crosscheck.json` records **68 untouched upstream wire checks**, **6 supplementary
checks using an explicitly corrected external consumer**, known monobit
limitations, three sciibin consumer cases and **11,172 independent
syllable-selection and raster checks**. The wire checks include 12 original
bitmap variants through monobit's IIgs 1.1/1.5 writer and both native writers;
a real nonzero 1.5 high offset; single/multiple BinSCII segments from both Rust
and the separately authored Python fixture generator; and all 360 native Korean
components through monobit's raw reader/writer. Every compared code, width,
height, offset, advance and raster byte matches. Large 1.5 rasters exercise
128×5000 glyphs, so high-offset handling is not inferred from a zero high word.

The 844 composition oracle is the independent published GameMaker renderer
`TandyRum1024/hangul-johab-render-gms`, pinned to commit
`e21dc74c83491f8708a4a3391dc6ea7125470f0e`. The script downloads three pinned GML
files outside the repository, mechanically adapts syntax to JavaScript, executes
the original lookup generator and syllable drawing branch, and records their
chosen sprite indices. Only storage geometry changes: its 28-column sprite rows
are projected into the compact 20/22/28-column native slots. All selections and
OR-composed original-fixture pixels match. **This is not execution of the full
GameMaker engine.** No third-party glyph artwork is used. The old jo844touni.py
link was unavailable; a 529-component converter was not substituted for 844.

The original **sciibin 3.1, dated April 30, 1992**, independently consumes three
generated BinSCII files: 65, 12,353 and 30,065 decoded bytes. Normal `-oPATH`
operation exits 0, reports no checksum error and reproduces exact source bytes.
Its source archive hash and build command are recorded. The external tool's
optional `-s`/`-t` exploratory modes crash in its own `fclose(NULL)` path on this
host; those modes are not used as validation. Building with forced standard C
headers resolves old implicit-declaration warnings without algorithm changes.

One oracle difference is deliberate and documented: monobit 0.54 assumes that
the location table immediately follows the raster and raises `ValueError` for
our legal pointer-relative table-gap fixture. Apple volume II defines locTable
relative to owTable; the native unit test validates those pixels and exact
rewriting. The original Apple volume III scan was also rendered and inspected
at PDF page 460 / printed 43-5 to confirm the high offset field.

## ASAN smoke and final coordinator handoff

| Target | Runs | Seconds | Edge counters | Features | Peak RSS | Exit |
|---|---:|---:|---:|---:|---:|---:|
| hanfont | 235 | 21 | 826 | 834 | 324 MiB | 0 |
| iigs | 45,211 | 21 | 1,765 | 3,573 | 520 MiB | 0 |

These are short smoke campaigns, not substitutes for the coordinator's required
610-second final-source campaigns. Hanfont checks both projections and several
complete 11,172-glyph read/write/reread models per accepted input; the small run
count is reported as observed. ASAN is enabled; `detect_leaks=0` follows the
repository's existing environment configuration.

The initial IIgs smoke stopped on an overstrong harness equality assertion when
the writer added requested BinSCII provenance to a previously bare font. No
source data changed. The harness now removes only this newly added property
before comparing complete models, and only when the source had no such property
and BinSCII was explicitly requested. Existing wrapper provenance is still
compared exactly. The original failure log and subsequent harness compile-fix
log are retained; the successful final campaign used the corrected harness.

```sh
source scripts/local-env.sh
export CARGO_TARGET_DIR="$FONTPIXEL_WORK_ROOT/target-w52" CARGO_BUILD_JOBS=2
python fuzz/seed_w52.py --directory "$FONTPIXEL_WORK_ROOT/w52/final-seeds"
ASAN_OPTIONS=detect_leaks=0 cargo +nightly fuzz run hanfont \
  --no-default-features --features hanfont-only \
  "$FONTPIXEL_WORK_ROOT/w52/final-seeds/hanfont" -- \
  -max_total_time=610 -timeout=3 -rss_limit_mb=1024 -max_len=11520
# IIgs: target iigs, feature iigs-only, corpus .../iigs, max_len=262144.
```

The nine fresh original seeds include bare 1.1/1.5, a nonzero extended offset,
legal table padding, rectangles entirely above/below the baseline, one/three
BinSCII segments and all 360 Korean components.
All generated data is original CC0 synthetic data, not third-party font artwork.

No formal performance measurement was taken by this worker. The coordinator
owns the quiet five-run release comparison and repository-wide acceptance.
`w52_timing time FORMAT INPUT COUNT [bare|binscii]` performs three warmup rounds
and separate in-memory read/write loops. `time-canonical` removes IIgs native
replay before measuring encoding. FORMAT is `hanfont`, `components` or `iigs`;
the hanfont mode exercises all 11,172 glyphs. Both byte counts and glyph counts
are emitted, along with durations. The crosscheck command is reproducible:

```sh
cargo build -p fontpixel --no-default-features --features hanfont,iigs --example w52_timing
python tests/crosscheck/w52.py \
  --driver "$CARGO_TARGET_DIR/debug/examples/w52_timing" \
  --work "$FONTPIXEL_WORK_ROOT/w52/crosscheck" \
  --sciibin "$FONTPIXEL_WORK_ROOT/w52/sciibin/sciibin"
```

## Superseded initial freeze and signed-metric correction

The initial freeze `787ffeaad7ffa72fd459ef2c6de21fc92fa854dbaeecd979a00cd21c64282427`
is superseded. Its manifest, report and affected validation logs are preserved
under `superseded-initial/`. A post-handoff reread of Apple volume II p.16-48
found that ascent and descent may be negative. The coordinator authorized
unfreezing before any IIgs long campaign started. Final code treats ascent,
descent and leading as signed words, derives pixel size from actual strike
height, and permits font rectangles that do not touch the origin. Native replay
and canonical 1.1/1.5 output preserve those metrics. The same source specifies
0–254 for each defined width/offset byte; partial FF entries now fail explicitly.
Hanfont production code, its shared support and its harness were unchanged.

The six new supplementary oracle cases expose two defects in monobit 0.54:
it declares ascent/descent/leading unsigned and applies the Macintosh positive
nDescent high-offset convention to IIgs. Original failures and raw unsigned
metric results are retained in `crosscheck.json` and the negative-oracle logs.
The adapter changes only those three field declarations and guards the high-word
branch with big-endian Macintosh input. It replaces functions **only inside the
crosscheck process**, restoring them afterward; no shared virtualenv source is
edited. Original/adapted function hashes are in `signed_metric_adapter` in the
JSON. Actual decoding, glyph extraction and metric conversion then run through
the independent monobit implementation. These six checks supplement the direct
Apple specification and original Rust regression; they are not claimed as
support by an untouched external consumer. The original 68 upstream checks
remain separate and unchanged.

## Attribution and suggested coordinator decisions

Production code is original Rust derived from public field descriptions and
factual selection tables. No GPL/MPL implementation was copied. monobit 0.54
is MIT-licensed and is used only as an external oracle. The GML repository has
no explicit license file in the inspected tree; sciibin identifies copyright
1989/1990 M. J. E. Mol without an explicit distribution license in the inspected
archive. These tools and their adapted implementation stay outside the project;
only original downloading/adaptation commands and hashes are distributed.
Apple specifications and the Unifoundry description remain external references.

Suggested decisions: declare Johab844 explicitly, avoid headerless auto-detection,
retain complete components and reject unrepresentable inverse composition;
derive IIgs table positions from the official offset rule and protect text-buffer
extent after edits; treat BinSCII as a bounded single-font transport with exact
native wrapper preservation and explicit loss on removal; record all actual
model differences through same-limit writer reread before strict-mode approval.
