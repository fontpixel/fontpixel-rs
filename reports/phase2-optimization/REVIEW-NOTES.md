# Phase 2 review handoff

Production source at `df83a08` (preceded by `3117375`) is frozen. Main remains
reviewed W2.8 `54c7f29`. The candidate and review-fix branches are not merged.

## Confirmed review findings and validated fixes

The new `tong_export` example copies input BDFs directly to output/bdf before
writing the other formats. When those paths alias inputs, `fs::copy` can truncate
them. Independent scratch reproduction used twenty disposable 282-byte BDFs and
input ROOT/bdf with output ROOT: the program exited 0 and all twenty source files
became zero bytes. Actual project inputs were never used in that reproduction.
Evidence is retained in `alias-review-reproduction.json`.

The isolated benchmark/test output directories do not alias inputs, and all
original 690 protected inputs remain unchanged. Thus existing correctness and
performance records are valid for their exact source/binary/path conditions,
but this candidate is not approved for general use or integration yet.

The independent review completed commit `7b7ed29c86a75d02d4a429bdb61d9ec4324c3c61`
on `review/phase2-input-safety`
in sibling `fontpixel-rs-phase2-review`, based on `00a5527`. The intended guard
checks all 84 outputs against all inputs, output/output hardlinks, and dangling
output symlinks, while preserving normal separate-directory reruns. The branch also fixes width+1/height+1 arithmetic before checked grid multiplication
on 32-bit targets. Both fixes pass the complete 928-execution check and eight
actual-binary path/rerun cases. Its new five-run default median is 38.414 s with
2.869 GiB peak RSS, and every one of the 84 files matches the frozen independent
oracles. The earlier timings retain their original binary identity. See
`../phase2-review/README.md`, `default-five-run.json` and `output-identity.json`.
Integration remains the coordinator's next step; no remaining blocking finding
was identified within the reviewed scope.

## Evidence to inspect

- `validation-summary.json`, `full-export-validation.json`, `supplied-vectors.json`
- `dot-policies.json`, `woff1.json`, `subsets-raster.json`, `benchmark-validation.json`
- `check-final.log` and exit 0: 912 passing test invocations
- `parity-final.log` and exit 0: 0 NEW divergence; seven retained known K12 cases
- `doc-final.log`, `fuzz-deny.log`, `protected-final.sha256`
- `fuzz-summary.json`: three fresh 612-second ASan gates, all exit 0
- `production-final.sha256` and `binaries-final.sha256`: exact measured identities
- `COMMANDS.txt`: complete review/reproduction commands; heavy timing is isolated
- `../../benches/PHASE2-REPORT.md`: five-run methods, values and explicit tradeoffs

The fast tracer preserves the SPEC edge/walk/rotation order. Dot reuse adds no
GIDs, makes selected helpers literal, limits nesting to two levels, consumes ink
once, and preserves pre-optimization work budgets. Prepared plans require exact
layout-limit equality. CFF axis runs cap at 40 operands and reset at every other
command kind. The rejected Brotli shared-hasher option is disabled; its regression
stays in the test suite.

The default dot pattern policy shrinks TTFs but enlarges compressed webfonts.
`literal`, `rows` and `patterns` are explicit CLI/API choices. Generic batch CLI
work is still Phase 6, and no W3 work has begun. Current model/fast-tier settings
cannot be independently verified from this execution environment.
