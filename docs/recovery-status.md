# Phase-6 source and documentation recovery status

**Phase 6 delivered, 2026-10-05:** the full Tong/catalogue/SC/merge/legacy-parity
comparisons against frozen release binaries, the 104-target ASAN campaign and the
formal PLAN §10 benchmarks all passed; see the [phase 6 report](../reports/phase-6.md).
The pending items in the paragraphs below are the historical record.

**Update, 2026-10-05:** the reconstructed phase-6 source now compiles.
`scripts/check.sh` passed (3,453 tests, clippy `-D warnings`, cargo-deny) and
`RUSTDOCFLAGS=-D warnings cargo doc` is clean. This replaces the "uncompiled and
untested" statements below, which are kept as the 2026-10-04 record. It is not a
result of the full Tong/catalogue paired validation, performance or ASAN gates.
Catalogue output differences are documented in
[catalogue migration](migration-fontpixel.md#catalogue-download-differences).

As of 2026-10-04, the present phase-6 source reconstruction is **uncompiled and
untested**. The earlier workspace disappeared at approximately 18:33 UTC. The
phase-5 baseline was restored separately, then recovered and reconstructed phase-6
files were added. No historical pass is current-source acceptance.

The final continuation entry is [the local Codex handoff](LOCAL-CODEX-HANDOFF.zh.md)
and its [copyable prompt](PROMPT-FOR-LOCAL-CODEX.zh.md). The final package manifest
identifies the delivered commit, refs, source and evidence. Online fontpixel
implementation stops at this handoff; subsequent software work belongs to the
local continuation. A separately authorized western-rest glyph task is outside
this software package.

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

The earlier catalogue comparison omitted bitmap family, style and copyright.
Reference-side policy inspection has since identified app naming/copyright,
UPM1000/largest-strike/ties-even bitmap metrics and implicit FontTools defaults
including fsType=4, fsSelection=0 and usBreakChar=32. Corrected plan-v2 adapters,
normalized naming controls and explicit embedded metric flags are now in source.
Fresh original-Python output checks confirm 181 files and the expected app fields
in all 105 sfnts; see [external-only evidence](../benches/phase6-oracle-setup/README.md).
This supersedes the earlier statement that this policy inventory had not begun.
It does not establish reconstructed native parity. Full paired names/defaults/
metrics/encoded and unencoded glyphs/notices/ZIP verification remains pending;
source controls alone do not prove an adapter uses them correctly. The implicit
FontTools fields are library defaults, not source-author permission statements.
The original production `opf_vector` extension was absent from the supplied
reference; a pure fontTools oracle is a specifically labelled alternative baseline.

Source inspection around19:42 UTC found that `--each`, visible `vectorize` and
`bdf.write.notice_line_breaks` handling have now been restored, superseding the
missing-source observations at19:24. They remain uncompiled/untested. The new
api_quickstart example is a rewrite against current declarations, not the lost
byte-identical example. Its native assertions have not run.

Batch retained transaction accounting now covers prepared outputs plus shared
source identities under the same default512MiB cap. Identity admission failures
make protection incomplete and prevent every commit. Job/path/task and report/
working bounds remain separate; disk-backed temporary storage is required for
report spooling to reduce whole-system retained RAM. None of these limits is an
RSS guarantee. Runtime stress, source-alias and failure tests remain pending.

## Latest source and validation boundary

Final source-only review records fixes for explicit-format validation, returned
resolver capacity, shared writer limits, plain aggregate output length, complete
caller-model/report reservations, merge staging and escaped audit messages.
Known scope gaps remain: standalone source identities lack batch's aggregate
ledger; core Variants/export transients and subset read/selection are not fully
accounted; conservative zero-credit writers can change default-workload admission.
The W6.6 real Tong regional CJK+Latin merge-equivalence fixture still needs both
implementation and execution, separately from Tong variants/locl and Unifont union.

The 20:41 UTC snapshot parsed 600 Rust files with no syntax diagnostics, but did
not type-check, expand macros, resolve Cargo features, build or execute tests.
The 20:46 coordinator rerun passed the static API checker and 9+4+6 standard-library
performance-tool tests. External Python/Node/consumer fixture checks are fresh;
there is still no reconstructed Rust output proof or formal timing. Final Cargo,
libstd, formatter/Clippy, ordinary gates, independent native oracles, performance
and local final-source ASAN remain outstanding. Earlier checkpoint notes saying
no syntax review existed or oracle dependencies were missing are historical.

## Scope and acceptance

Complete phase-6 implementation, gates and delivery locally, then continue
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
