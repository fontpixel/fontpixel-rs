# W6.4 restored source checkpoint, 2026-10-04

## What was restored

The PFR cache source, eight deterministic integration tests, reader inventory,
shared-review history and historical validation summary were restored from the
externally backed-up reconstruction packet into the Phase 5 baseline checkout.
The baseline commit is `12710057e9787318d1db70db41a60fca016ad53c`.
Only `crates/fontpixel/src/formats/pfr/wire.rs`,
`crates/fontpixel/tests/pfr_shared_physical.rs`, and this report directory are
owned by this restoration. Other workers are integrating shared changes.

The source and test are byte-for-byte copies of the reconstruction packet, not
verified byte-for-byte copies of the lost Phase 6 files. All nine original
recovery-packet hashes were checked before restoration. The pre-edit wire.rs
matched the baseline commit, and its reconstructed delta matched the saved
recovery patch. The original packet remains unchanged.

## Source review and retained contracts

The decode-local cache keys complete `(offset, size)` spans. Logical records
are validated once; a sorted, deduplicated index supports binary-search lookup.
Each unique physical table is parsed lazily on its first logical encounter.
Input, header and explicit dialect remain fixed for that decode call.

The new vectors are charged through the existing checked `Budget::items`
(which reserves twice the element-size estimate), before fallible exact
allocation. Deduplication does not refund retained cache capacity. Character
storage is charged once per unique physical table. Output fonts, glyphs,
metadata and diagnostic entries still use their existing cumulative charges.
The original logical/font-strike/glyph/input/table/pixel checks remain in place.

Outline counts are cached as facts; each logical encounter emits and charges
its own loss entry before missing-strike/output processing, retaining logical
indices and order. Cache sharing does not share output glyphs or diagnostic
entries. Equal offsets with unequal lengths remain separate records. Existing
writer options, explicit dialect decisions, Gray8 conversion and strict/loss
contracts are unchanged.

Extra indexing storage can reject a tighter unique-record budget than before.
Upfront logical validation can change first-error precedence for inputs with
multiple independent defects. These are documented consequences, not claims
that every prior malformed-input error has identical precedence.

## Evidence available now

Read-only source inventory reconfirmed 99 catalogue IDs, 104 declared fuzz
binaries, 82 same-name mappings, and 22 integration-test files containing
proptest. `reader-target-inventory.json` lists all 99 mappings and all 104
manifest entries, including helper targets; it records correspondence, not
execution or coverage acceptance.

No Rust build, formatter, test, project helper, seed replay or sanitizer run was
executed for this restored tree. Cargo is unavailable in the restored execution
environment according to the coordinator. The earlier 9 existing plus 8 new
PFR test pass and focused Clippy pass are historical observations only. Their
original source manifest and raw logs were lost; the reconstructed historical
JSON is not replacement execution evidence. Formatting and compilation still
need to be checked against the complete integrated sources.

## Local continuation and acceptance

The user accepted deferring blocked long ASAN campaigns to their own computer.
Those campaigns remain unperformed. Follow the existing repository
`fuzz/README.md`, `docs/transport-fuzz.md`, and the local-continuation section in
`reader-inventory.md`; no campaign was retried during restoration.

First check formatting and run the focused ordinary commands recorded in
`historical-validation-summary.json`, then the applicable shared/CLI/property
and final aggregate gates. Freeze final compiled source identities before new
long-run evidence, and rerun affected checks after later shared-source changes.
Keep corpus, build and artifact output under local-work and original inputs
read-only. No previous Phase 5 or pre-loss result proves this restored tree.

The externally back-up-able owned source snapshots, baseline wire, full textual
diff, hashes and restoration metadata are in
`recovery-20261004/robustness/restored-checkpoint` outside the checkout. Its
hashes identify this new checkpoint only. W6.4 acceptance remains pending.
