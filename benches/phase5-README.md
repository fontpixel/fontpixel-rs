# Phase 5 native format measurements

`phase5_native.py` prepares release executables and original fixtures, checks the
available adapter interfaces, and collects five sequential process repetitions
only after the coordinator confirms a quiet work window. `phase5-cases.json` is
the generated inventory (`inventory`), not a second editable configuration.
The runner does not modify any codec, frozen adapter, or installed oracle.

```sh
source scripts/local-env.sh
python3 benches/phase5_native.py selfcheck
python3 benches/phase5_native.py prepare \
  --manifest "$FONTPIXEL_WORK_ROOT/phase5-performance/prepared-smoke.json" \
  --target "$FONTPIXEL_WORK_ROOT/target-phase5-performance" --smoke-only
python3 benches/phase5_native.py smoke \
  --manifest "$FONTPIXEL_WORK_ROOT/phase5-performance/prepared-smoke.json" \
  --output "$FONTPIXEL_WORK_ROOT/phase5-performance/smoke-final.json" \
  --iterations 1
```

Preparation builds with `--locked --release`, pre-generates all inputs, and
records complete commands, exits, stdout/stderr hashes, fixture hashes, release
binary hashes, Rust source hashes, compiler/CPU/core/memory information and the
installed monobit source identity. `--groups b,c2,e2,transport` selects the new groups (all groups are the default);
`--cases a2:bbc,e1:read-split` selects individual prepared cases for smoke/measure.
Outputs and preparation directories must be new. `--skip-build` is only for
interface development: its manifests are explicitly ineligible for formal runs.
`--smoke-only` is also explicitly ineligible, even after every group freezes.
If sources changed during preparation, that manifest is likewise ineligible;
`source_changes_during_preparation` records each before/after hash. Final owner
proofs are checked both before building and after generating the fixtures.
B/C/C2/E2/transport require recorded final owner freeze proofs; missing or stale
proofs cannot produce a formal-eligible preparation. The C proof includes the
final C/FON detection integration supplement. An explicitly smoke-only
preparation records stale-proof errors without claiming acceptance.

After all task-owned builds, fuzzers and external consumers have stopped, wait
for the coordinator to authorize the quiet window. Prepare again without `--smoke-only` after final source acceptance if any source,
fixture, binary or oracle changed. Every group freeze manifest and its owned
source hashes, external JAR/class files, Pillow/FreeType sources and native
libraries are checked as well. Create a fresh preparation without `--smoke-only` at `prepared-final.json`; then run:

```sh
source scripts/local-env.sh
python3 benches/phase5_native.py prepare \
  --manifest "$FONTPIXEL_WORK_ROOT/phase5-performance/prepared-final.json" \
  --target "$FONTPIXEL_WORK_ROOT/target-phase5-performance"
python3 benches/phase5_native.py measure \
  --manifest "$FONTPIXEL_WORK_ROOT/phase5-performance/prepared-final.json" \
  --output "$FONTPIXEL_WORK_ROOT/phase5-performance/quiet-final.json" \
  --iterations 100 --quiet-window-confirmed \
  --environment-note 'Coordinator confirmed all task builds, fuzz and consumers stopped; describe other machine activity here'
```

The command requires the explicit quiet-window flag and a nonempty machine note.
It checks for competing workspace build/fuzz/consumer/runner processes before and after
measurement. This guard supplements coordination; it is not proof that unrelated
system activity was absent. Source changes are fatal for a formal run. During a
busy smoke they are recorded, and the result remains `formal: false`. A failed
run keeps raw logs and append-only progress but produces no successful formal summary.

## Meaning of results

Every native timed loop uses bytes already in memory. The E1 read adapter also
preloads every resolver resource, checks the original expected coverage/advance/
code mappings, compares the complete cached model and reports, then warms up
five times. Initial fixture loading, fixture creation and independent correctness checks
are outside its timing region. The E1 writer adapter retains its original
actual iteration and glyph counts; Cybiko/SBF use at most 190 input glyphs.
Its count is the **source model passed to the writer**, not a claim about native
padding/default slots. E1 reader and writer cases use different workloads.

The orchestration normalizes elapsed units to **nanoseconds per operation**:

| Adapter | Reported units | Iterations and warmup |
|---|---|---|
| A1 and X | Total nanoseconds | Requested count; existing setup reads retained |
| A2 and C/DRCS | Nanoseconds per operation | Requested count; existing setup retained |
| C1 | Microseconds per operation | Fixed 300; ten warmups |
| W5.2 and D | Total seconds | Requested count; three warmups |
| W5.3 | Microseconds per operation | Fixed 1,000; ten warmups |
| W5.4 | Microseconds per operation | Fixed 400; ten warmups |
| B | Total seconds, separate read/write counts | Fixed 100 reads / 20 writes; three warmups |
| C2 | Microseconds per operation | Fixed 300 reads/writes; ten warmups |
| E2 images reads | Total nanoseconds | Requested reads; five warmups; clock stops before full model/report equality verification |
| E2 images writers | Total nanoseconds, four-column TSV | One operation per process; source 100 glyphs, SFont capped at 94 |
| E2 SSFN/PFR | Total nanoseconds | Requested reads/writes; initial untimed source read, no explicit repeated warmup |
| New Processing/BitsNPicas reader | Nanoseconds per operation | Requested count; five warmups; public memory API, ImageIO disk cache disabled |
| Pillow PILfont reader | Nanoseconds per operation | Requested count; five warmups; primary and companion in memory |
| FreeType PFR reader | Nanoseconds per operation | Requested count; three warmups; full known-glyph materialization through Python wrapper |
| Shared raw/gzip pipeline | Total nanoseconds per operation kind | Requested count; five warmups for each kind; 256 original 8×16 glyphs |
| E1 reads | Nanoseconds per operation | Requested count; five warmups |
| E1 writers | Total nanoseconds, TSV | Requested rounds; no explicit warmup |
| CBDT/SBIX | Seconds per operation | One operation per process; no explicit warmup |
| New monobit memory wrapper | Nanoseconds per operation | Requested count; three warmups |

`--iterations` does not override a frozen adapter's fixed count. Five independent
process repetitions are always used by `measure`; medians/min/max retain the
original reported counts. Peak RSS is ordinary child process RSS from `wait4`
(KiB on Linux), including interpreter, input, model and allocations outside the
timed loop. It is not allocation-only memory or the peak of just the codec.
Read/write timings within one process share one RSS observation. Multi-record
E1 and Color processes also share their process RSS.

The output separates Rust, Python, Java and external native consumers. Missing
engines and unsupported operations remain null. Existing external correctness
consumers are not automatically valid memory-only timing adapters. No Java or
native speed ratio is invented from command startup, disk conversions or absent
writers. COM has no monobit writer. XBIN-512 and ZapRedraw's independent writer
projections do not provide a matching writer baseline. A missing independent
reader is retained with its exact exception rather than labeled successful.
Python X raw column layout uses the published Raster API because the upstream
raw wrapper ignores its obsolete argument; the frozen crosscheck records this.
Palm's read fixture is bare v1, while the native timed writer produces PRC;
monobit supplies a bare NFNT reader but no matching PRC writer. The DRCS oracle
writer receives the same public-model reconstruction validated by W5.5: it
removes the upstream reader's malformed `dec_drcs` property outside the timer,
retaining the original glyphs. The result explicitly records that projection.
PXL's optional TFM resolver is explicitly in memory. Wyse's external two display
columns are removed before the independent writer, outside its timed loop.

`fixture_metadata` reports glyph/pixel counts from an untimed complete native
model and exact fixture byte counts. An adapter's own `metadata.glyphs` is kept
separately. Python adapters that omit their count do not acquire a fabricated
Python model count. BMFont fixture bytes include referenced PNG pages; E1 read
results separately report primary/resource bytes. Measurements are per supplied
fixture, not per Unicode mapping (aliases can share a glyph).

Replay/canonical D writers have separate rows. The native writer may validate
its output by reading it back and audit resource limits/report losses; an oracle
writer need not do those same operations. No automatic speedup ratios are
published by this runner. Any later comparison must use the matching operation,
fixture, geometry and documented projection and cite both engine columns.
These fixtures do not replace PLAN §10 whole-font or whole-catalogue goals.

## New group profiles and comparison boundaries

B includes all 28 emitted native profiles and a separate `vtfont-rebuild` row.
Default VFNT writing preserves its original bytes; that row has no Python writer
comparison. The explicit rebuild row keeps the same fixture and permits a
separate monobit writer measurement. Native BeOS Native8 and monobit FullNibble
interpretations differ; the default Native8 case therefore has no Python timing
baseline. FullNibble remains an explicit, independently validated profile.

C2 uses all unique original seed profiles, including unsupported-by-monobit
relative offsets, RISC OS phases, PCL RLE and Xerox variants. Reader absence is
explicit. Only HPPCL has a matched independent writer. Known Alto baseline,
Prepress padded-height and Prebuilt vertical-advance differences remain visible;
no model equality or automatic speed ratio is claimed for those projections.
Little-endian Prebuilt has no direct monobit timing baseline: the upstream
reader mirrors its rasters, while the original NeXT producer uses MSB-first
pixels. RISC OS calls the unchanged upstream decoder directly so its optional
IntMetrics lookup uses the preloaded in-memory resolver.

E2 image writers retain their original source metadata and Gray8 pixels. Their
one-operation source-model workload is separate from image-reader cases, which
use the independently emitted native projection. No external writer is timed
against that different model. PILfont reads use Pillow's original in-memory
`ImageFont._load_pilfont_data` decoder, including companion decode and advance
checks outside timing. VLW v8/v9/v10/v11 use Processing `PFont(InputStream)`;
SFont/RFont use BitsNPicas `importFont(byte[])`. These are original thin Java
wrappers compiled outside production code, with JAR/source/class identities in
the manifest. Java/ImageIO file caching is disabled before timing. Imageset uses
monobit's unchanged directory reader through a memory-only location. BitsNPicas
adds a virtual runtime space to SFont: its real returned count is 95 versus 94
encoded native glyphs. Counts are kept as returned, including any RFont virtual
space; neither model is trimmed to create apparent equivalence.

PFR FreeType cases use the explicit FreeType dialect, all native strikes and
fixture-known U+0041 lookup. Face creation, glyph loading and complete bitmap
materialization are timed; the Python wrapper cost is included in the native
engine column and labeled. A full 65,536-code scan is never part of the timing.
SSFN's sfnconv remains a file/process correctness consumer and is not represented
as a memory-only timing baseline. Unsupported engines and writer operations stay
null. The raw/gzip probe includes the second native budget audit in gzip output;
raw and gzip have separate operation records, and no oracle speed ratio is added.

`selfcheck` tests total/per-operation units, unequal operation counts, fixed
counts, missing operations, E2/transport output shapes, finite values and unique
case IDs. Existing v1 preparation evidence remains untouched; its exact six-file
source snapshot is archived under
`$FONTPIXEL_WORK_ROOT/phase5-performance/prep-v1-source`.
