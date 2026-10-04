# Phase 3 acceptance report

2026-10-04. All five work items are implemented, independently checked and
integrated into `main` at `e4624e2ab47b602921eb671f0a7525091325df68`.
The merged `scripts/check.sh` passes with **1,184 test executions**. All 177
recorded Rust/manifest fingerprints match the tested candidate. Evidence is in
`phase3-evidence/phase3-main-check.*` and `integrated-source.json`.
**Phase 3 is complete. Phases 4–9 remain unfinished.**

## Scope and evidence

| Work item | Native implementation | Independent evidence |
|---|---|---|
| W3.1 HEX / PSF | HEX, PSF1/2, Unicode aliases/sequences, bounded gzip | Previously accepted in `w3.1-review.md`; production codec unchanged |
| W3.2 FNT / FON | Raster FNT v1/2/3, NE read/write, PE read, multiple strikes | Previously accepted in `w3.2-review.md`; production codec unchanged |
| W3.3 image / C | Six atlas outputs with native sidecars, explicit external maps, four C input families, generic/GFX writes, shared CLI assets | Catalogue font8x8/spritesheet, Pillow, C11 compiler, monobit GFX, full Tong strict roundtrip; `w3.3.md` |
| W3.4 KBITX / PBF | Native grayscale KBITX; Pebble v1/v2/v3, raw/RLE4 and aliases | Independent catalogue, 15,932-glyph MuzaiPixel, 109,780 real TUMBLED mappings, wire fixtures; `w3.4-kbitx.md`, `w3.4-pbf.md` |
| W3.5 HBF | HBF 1.1 sparse byte ranges, explicit legacy charsets, bounded bitmap resources, deterministic bundles | Bidirectional monobit plus independent BDF/catalogue probes; `w3.5-implementation.md` |

The all-feature registry exposes **22 readers and 21 writers**, with **24 IDs
still unimplemented**. The previously deferred color writer is not advertised.
KBITX and PBF are not supported by monobit 0.54; their catalogue/wire comparisons
are the actual independent evidence, without a claimed monobit pass.

## Aggregate validation

- `scripts/check.sh`: **1,184 passing test executions in 191 suites**, exit 0.
  This counts repeated executions across feature configurations, not distinct
  tests. Generated charsets/printability, rustfmt, warning-free workspace and
  dependency-free Clippy, default/all/no-default/BDF-PCF/CJK tests and cargo-deny
  all pass. Exact commands, source snapshots and compressed logs are retained
  under `phase3-evidence/phase3-check-final2.*`.
- Final warning-free all-feature rustdoc, standalone image Clippy and the fuzz
  workspace's dependency/license/source audit pass (`phase3-extra-gates.*`).
  Dedicated standalone C/KBITX/PBF/HBF checks are also recorded by their owners;
  respective dedicated test counts are **17 / 13 / 18 / 12**.
- New CLI tests cover version/compression/threshold options, HBF split assets,
  source/output protection and partial PBF RLE read loss. Strict conversion
  returns status 3 and preserves existing output. A PBF-only CLI test confirms
  this without relying on BDF support.
- The integrated release repeats the entire image/C/Pillow/Tong crosscheck.
  A separate catalogue spritesheet check passes 5 fixture groups and 17 commands,
  with complete pixel/metadata and both input-entry-point comparisons.
- All successful fuzz writers must reopen their output under identical limits;
  PBF's write budget includes alias-heavy model expansion. Original seeds have
  checked-in generators. No interrupted or build-only campaign counts as a pass.

Five final ASan campaigns completed without crashes, each with a 64 KiB input
cap, 10-second per-input timeout and 1 GiB sanitizer RSS guard:

| Reader | Executions | Duration | Coverage / features | Peak fuzz RSS |
|---|---:|---:|---:|---:|
| image | 3,110,723 | 611 s | 11,762 / 37,335 | 624 MiB |
| C | 2,031,619 | 611 s | 3,959 / 14,815 | 548 MiB |
| KBITX | 7,186,705 | 611 s | 4,976 / 18,915 | 518 MiB |
| PBF | 4,917,140 | 611 s | 1,571 / 6,126 | 488 MiB |
| HBF | 20,805,036 | 611 s | 2,598 / 7,448 | 523 MiB |

`phase3-evidence/fuzz-summary.json` records exact commands and DONE lines. These
are sanitizer process measurements, not ordinary release memory use. Codec
source fingerprints match their final campaigns; unrelated registry integration
and whitespace-only harness formatting are covered by the aggregate check.
All **690 reference/sample files** match the supplied SHA-256 manifest.

## Performance

Five release runs, each with five warmups and 200 iterations, give these median
microseconds per whole-font operation. The fixture has 256 glyphs; image/C use
9×16 cells and KBITX/PBF/HBF use 8×16. Input bytes and generated output are kept
in memory. Readback pixels/mappings are verified before timing.

| Format | Read µs | Write µs | Output bytes, all assets |
|---|---:|---:|---:|
| PNG + native map | 1,090.194 | 789.226 | 65,258 |
| PGM + native map | 984.109 | 689.232 | 90,393 |
| Generic C | 339.727 | 878.459 | 52,429 |
| GFX C | 345.958 | 1,197.882 | 31,230 |
| KBITX | 458.396 | 296.654 | 32,935 |
| PBF v3 raw | 90.298 | 117.483 | 7,434 |
| HBF + bitmap | 40.411 | 12.889 | 4,469 |

For 1,000 independent whole-font reads, the median 1/2/4-thread milliseconds are:

| Format | 1 thread | 2 threads | 4 threads |
|---|---:|---:|---:|
| Generic C | 353.447 | 183.075 | 110.055 |
| GFX C | 338.399 | 176.477 | 100.895 |
| KBITX | 464.796 | 244.018 | 142.763 |
| PBF | 94.483 | 52.153 | 30.687 |
| HBF | 40.099 | 25.634 | 15.017 |

Four-thread throughput improves by **2.67–3.35×**. This measures independent
library calls, not the later generic batch command. KBITX and PBF report 1 and 2
metadata losses for this shared synthetic source; all mappings/pixels survive.
Peak process RSS is **18,680 KiB** for image/C and **19,020 KiB** for the other
three formats. Source examples, reusable `benches/codec_timing.py`, binary hashes,
all five raw runs and min/max values are retained in `phase3-evidence/`.

## Findings resolved

Independent review corrected C stable-ID loss, array selection, family detection,
comment scope and fatal report retention; nested native image/YAFF metadata now
rejects unknown fields. Image memory accounting permits full Tong at default
limits without unbounded serializer growth. Independent Pillow validation caught
the TGA encoder incompatibility and led to portable uncompressed output.

KBITX canonicalizes numeric name identities, reports retained metadata ordering
changes and preflights same-limit output. PBF follows the official v1 word-offset
layout and reports the 7,829 incomplete RLE rows found in a real TUMBLED font.
Reader/model budgets are shared with writer preflight. HBF reports unsupported
metrics/properties, preserves exact code domains and rejects invalid native
headers. The monobit HBF adapter changes only a missing terminator and the
verified same-directory bitmap basename; raw artifacts remain available.

The corresponding choices are recorded in `docs/decisions.md`; upstream and
fixture attribution is in `THIRD_PARTY_NOTICES.md` and per-format documents.

## CLI size and remaining limits

The integrated Linux x86-64 all-feature release is **9,532,040 bytes**;
`strip --strip-all` produces **8,564,680 bytes (8.17 MiB)**; deterministic gzip is
**3,824,291 bytes (3.65 MiB)**. Size records include both binary hashes and linked
system libraries. These are current implemented features, not a final project
size promise. Python and Rust are not runtime dependencies of the CLI.

The filesystem bundle writer protects inputs and stages all output before
replacement, with ordinary-I/O rollback and the entry point committed last.
It does not promise crash-atomic multi-file transactions or concurrent-writer
isolation. Format-specific unsupported fields are reported; strict conversion
may reject inherently lossy targets. Resource budgets model allocations rather
than exact operating-system RSS. Core/SFNT compatibility and performance results
from earlier phases remain attached to their original measurements.

Four workers handled KBITX, PBF, HBF and independent review concurrently, with
shared files owned by the coordinator in the single existing checkout. The
original worker request specifies Astra/xhigh; actual model routing and fast
service cannot be independently verified. Performance runs exclude task-owned
builds, fuzz and heavy oracles; unrelated applications are not terminated.

Next, phase 4 implements the P2 format work items in the documented order. Later
CLI/batch/merge, Python, WASM, the browser merger and final all-project performance
acceptance remain separate required work. No external site was changed or deployed.
