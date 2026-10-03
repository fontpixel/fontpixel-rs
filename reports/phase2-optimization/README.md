# Phase 2 performance candidate, not yet accepted

The accepted main remains `54c7f29`; all work here is on `phase2-acceptance`.
Baseline correctness is complete in `../phase2-acceptance`. The native baseline
five-run whole-export median is 143.884 s. Subsequent single-run engineering
probes are explicitly labelled and are not accepted medians.

The candidate combines packed small-grid contours, immutable reusable variant
plans, release ThinLTO, CFF axis-run packing, existing dot-pattern reuse and a
bounded complete-family API example. Quality remains 11. Dot representation is
selectable as literal/rows/patterns because reduced sfnt size may increase WOFF2
bytes. The individual-font compression default is two workers above 128 KiB;
the full-family example uses up to eight jobs and one worker per font.

Completed focused tests include all 512 3x3 square masks, all 512 displaced dot
patterns, original work-budget/int16 fallbacks, CFF run-boundary/reset cases,
three-policy geometry/metadata/subset equivalence and explicit large parallel
WOFF2 streams. The first optimized full Large dot and packed-CFF fonts pass all
glyphs plus 912 shaping batches against the original Rust exporter.

The experimental Brotli shared-hasher option was rejected after a new regression
caught an upstream assertion. It is disabled. Its timings are rejected probes.
The original 2D draft reversed row ordering; its exact-order test failure is
retained, and top-down processing was restored. An early policy test called a
nonexistent method rather than the public free function; its compile-failure log
is retained. These are development failures, not accepted gates.

All listed aggregate, external, timing, parity, raster, subset, protected-input
and fresh fuzz gates now pass. See validation-summary.json, fuzz-summary.json
and ../../benches/PHASE2-REPORT.md. Independent review confirmed the example
input/output-alias defect; its separate guard and a 32-bit arithmetic fix are validated at `7b7ed29` and
must be integrated. See ../phase2-review/README.md for 928 checks, eight release
path cases and a fresh 38.414-second five-run profile with all 84 hashes unchanged.
The candidate is not merged or accepted, and no W3 work has begun.

Release commands with explicit --example selectors do not build CLI binaries.
The final CLI/parity gate therefore uses a separate explicit --bins build; its
help output and fresh binary hashes are checked before accepted measurements.
Any earlier parity run using the pre-rebuild CLI is diagnostic only.
