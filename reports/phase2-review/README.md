# Independent Phase 2 review

Reviewed the frozen `phase2-acceptance` candidate at
`00a552777d16e62cc1d8c4d6b76ba68c68719abe`, with production changes
`31173757114e1e629dfe9cf844d500b1c035956c` and
`df83a084a52caa125f563259888426e391d2b585`. The accepted main remains
`54c7f299ba226d6f59946a7f2943692ba5a21c02` until coordinator integration.
Review fixes live separately on `review/phase2-input-safety`.

## Findings

1. **P1, confirmed source destruction in the new exporter example.** Calling
   `tong_export ROOT/bdf ROOT 1 0 1` on twenty newly generated disposable BDFs
   returned exit 0 but reduced all source files from 282 bytes to zero. The
   `fs::copy` destination was the source itself. Other output extensions could
   overwrite sources through pre-existing symlinks or hard links. See
   `alias-before.json` for the frozen executable identity and complete result.
   The fix preflights all 84 destinations against every source using portable
   open-file identity before any file is written. It also rejects output/output
   aliases, unresolved output symlinks and non-file destinations. Ordinary
   separate-directory overwrite/rerun workflows stay supported.
2. **P2, checked-add omission in 32-bit dense-grid dispatch.** Width+1/height+1
   occurred before checked multiplication. Explicitly raised bitmap limits allow
   a valid zero-area bitmap with a maximum-u32 dimension, which overflows usize
   on 32-bit targets. Both additions and the product are now checked. Tests
   exercise maximum-usize arithmetic and the public maximum-width/zero-height
   bitmap. The arithmetic was inspected on the 64-bit host; no 32-bit executable
   run is claimed.

The first fix is a stable-filesystem preflight, not protection against hostile
concurrent path replacement. `same-file` 1.0.6 was already present in the lockfile
via test/benchmark dependencies; it becomes an explicit dev dependency for this
example and adds no runtime library dependency.

## Independent review coverage

- Checked packed contour insertion/walk/turn/rotation order against the general
  tracer, resource charges, empty bitmaps and dispatch limits.
- Inspected pattern masks, displacement arithmetic, top-row anchors, zero gaps,
  the 256-helper bound, int16 and large-extent fallbacks, helper selection order,
  preservation of literal work budgets, and acyclic depth-two composition.
  Helpers stay literal and are retained through existing recursive subset closure.
- Checked reusable-plan ownership and lifetime: the immutable plan owns cloned
  glyph data and borrows base metadata; exact limits are required at encode time.
  Direct encoding remains available and rechecks shape/scale/units.
- Checked CFF alternating-axis packing across the 40-operand boundary, same-axis
  and zero-length segments, move/curve/close resets, width consumption and command
  budgets. No geometry or stack-limit defect was found.
- Checked bounded font-job scheduling, explicit job/thread limits, fixed result
  order, deterministic helper ordering, and size-based Brotli threading. The
  rejected shared-hasher experiment is disabled.
- Read the external full-font, policy, subset/raster, WOFF and fuzz harnesses,
  including the specifically scoped original-reference OTS exemption. Native
  outputs never inherit that exemption.
- Recomputed default five-run wall median 38.233479560003616 s and maximum RSS
  2.9048385620117188 GiB from raw records. Matched all 76 nonweb timed artifact
  hashes to complete independent oracles. Verified actual frozen production and
  binary hashes against their retained manifests. Counted 912 passing test
  executions across all check-script profiles; this is not 912 unique tests.
- Reviewed the native byte/default changes as intentional: pattern dot composites,
  CFF packing, and the two-thread WOFF2 default can change native output bytes.
  Literal dot mode preserves the old TTF representation and compatibility parity
  reports zero new divergences. Adding `dot_reuse` to a public options struct can
  require callers with exhaustive struct literals to add the field or use
  `..Default::default()`; this unreleased project does not claim semver-stable API.

No further blocking core rendering, layout, subset or deterministic-output issue
was found in this review. The intentional size tradeoff remains material:
Patterns favors generation speed and smaller TTFs, while dot WOFF2 grows about
35–37%; literal/rows/patterns remain explicit choices.

## Validation status

- The complete `scripts/check.sh` gate passed with 928 test executions across
  feature profiles, including six example path tests and two arithmetic tests.
  Clippy, format, dependency/license checks and all tests passed. A first format
  check needed a second rustfmt pass; the initial formatting-only log is retained.
- The optimized exporter builds independently and its actual binary passes all
  eight disposable-fixture alias/rerun cases. All twenty inputs are preserved on
  every rejection, and two ordinary exports produce 84 byte-identical artifacts.
- All 690 protected reference/sample file hashes and six frozen measured executable
  hashes still match. The separate review target does not replace those binaries.
- The guarded release binary's fresh default quality-11 runs are 38.414, 36.947,
  39.481, 39.317 and 36.554 seconds: median **38.414 s**, peak **2.869 GiB**.
  All 84 outputs are byte-identical across all five runs and byte-identical to the
  frozen candidate's complete independently validated output set, including all
  webfonts. Input hashes match too. See `default-five-run.json` and
  `output-identity.json`. This meets the ≤40 s median target after the safety fix.
  The measured binary SHA-256 is
  `3426bb4d2357d9983574dd35920fc9bfc7b693648d91673270ce13ceda77c455`.
- The earlier attempted debug harness invocation had no built debug executable,
  so it exercised no exporter behavior; only the passing release harness is an
  acceptance result. No heavy build, fuzzing or correctness job overlapped the
  final timed interval; the original shared-host caveat remains applicable.
- Existing full-font/fuzz evidence was inspected rather than blindly rerun: the
  actual final 84 files match the independently gated artifact hashes exactly.
  The only library change is a checked dispatch arithmetic boundary; its new
  focused tests run through the complete aggregate gate.

Recommendation: integrate these two review fixes before accepting Phase 2. No
remaining blocking finding was identified within the reviewed scope.

Frozen candidate timing/oracle records remain tied to their original binary.
This review does not merge main, accept Phase 2, or begin Phase 3.
