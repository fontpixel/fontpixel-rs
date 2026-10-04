# Phase-6 source and documentation recovery status

As of 2026-10-04, the present phase-6 source reconstruction is **uncompiled and
untested**. The earlier workspace disappeared at approximately 18:33 UTC. The
phase-5 baseline was restored separately, then recovered and reconstructed phase-6
files were added. No historical pass is current-source acceptance.

## Provenance

The three examples `batch.toml`, `tong_batch.py` and `check_documentation.py` were
recovered byte-identically against recorded source hashes. The API crate guide
and bitmap/model guide preserve exact captured base text plus recorded accounting
patches; this restoration adds status notices. Longer API/migration/CLI guides
are reconstructed prose and need review against the rebuilt implementation.

The original manifest is present and matches its recorded SHA-256. Its 41 active
family keys, four instance keys, seven licence keys and three variant override
keys were re-inventoried from TOML data, without running the reference pipeline.
The current format index links the baseline's unchanged 99 IDs. It adds no codec,
dialect support or test result.

Earlier observations included warning-free rustdoc, feature-matrix examples,
18 CLI documentation smokes and an 84-task miniature Tong batch. Their old raw
logs, generated outputs, binaries and final proof ZIP are absent from this source
checkout. The remembered hashes and counts are historical identifiers only.
The miniature batch did not establish real-font equivalence, performance or
large-report spooling correctness. All affected gates need renewed evidence.

## Open migration and source gaps

The catalogue comparison omitted bitmap family, style and copyright. The
inventory of implicit FontTools defaults and metric differences remains pending.
Do not claim semantic replacement from the earlier glyph/strike/package checks.
Reconstructed normalized name overrides are present in source, but their presence
does not prove that catalogue adapters populate the right values or preserve all
metadata, table flags, defaults and metrics. Inspect and compare those separately.
The original production `opf_vector` extension was absent from the supplied
reference; a pure fontTools oracle is a specifically labelled alternative baseline.

Read-only source inspection during this documentation restoration found `--each`
absent, `vectorize` registered as a hidden alias, and no
`bdf.write.notice_line_breaks` writer handling. These are restoration gaps observed
around 19:24 UTC, not executed test failures. Their intended contracts remain in
the restored guides; source owners may restore them subsequently. A current build
and focused tests must establish their actual status before removing this note.

## Scope and acceptance

Finish and deliver phase 6 with truthful outstanding-gate reporting, then continue
phase 7 followed by phase 8. Python and WASM/JS both require complete native public
developer APIs plus separate bdfparser/bdfparser-js compatibility APIs. Coverage
must map every operation/type/option/feature to a binding and independent native
test; platform-specific bytes/resources/async interfaces cannot silently omit
capabilities. Genuine unavailable capabilities need evidence, queryable status,
explicit errors and documented alternatives; unimplemented means unfinished.

Phase 9, remote push, package publication and deployment remain excluded. The
blocked long ASAN campaigns stay explicitly deferred to the user's local
computer and remain unperformed. Later binding work does not waive that phase-6
validation gate. The coordinator owns TODO, final reports and acceptance.
