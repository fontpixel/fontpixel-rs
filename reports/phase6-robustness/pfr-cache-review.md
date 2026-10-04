# PFR shared physical-record maintenance (reconstructed)

Recovery status: intended implementation and eight deterministic tests recovered
from conversation context after workspace loss. Not compiled or executed during
recovery. The previous 17-test/Clippy pass is historical; original logs/hashes
are unavailable. Final long ASAN validation remains unperformed and locally
deferred with the user's agreement.

## Design

The old reader reparsed the complete physical character table for every logical
reference. The reconstructed change validates logical descriptors once, sorts
and deduplicates their `(offset, size)` keys, and lazily parses each unique span
at its first logical encounter. Cache lookup is binary search. Input, header
and explicit dialect are fixed within the decode call; there is no cross-call
cache.

Logical descriptors and cache slots use fixed-capacity vectors, charged before
fallible exact reservation. Full reserved cache capacity remains charged after
deduplication. Physical character allocation is fallible and charged once per
unique record. Cached allocations stay alive and are not refunded. This avoids
hidden tree-node allocations and repeated linear cache searches.

For L logical entries, U unique physical spans and C characters per physical
span, indexing/parsing becomes O(L log L + L log U + U C), plus actual bitmap
lookup, metadata and output work. This is source-derived complexity, not a
measured performance result.

The cache stores outline counts as facts; losses are emitted and charged once
per logical encounter in the original logical order and location. Missing-strike
losses remain per logical encounter. Every output font, glyph, raster, metadata
copy and diagnostic remains charged. Logical/font-strike/character/output-glyph
limits remain enforced. Equal offsets with different sizes are separate keys.

The upfront logical-descriptor pass can change which error is returned first
for inputs with multiple independent defects. Extra bounded indexing storage
can reject tight-budget unique-record inputs that previously fit. Native
profiles, explicit dialect selection, Gray8 choices and strict/loss semantics
are unchanged.

## Recovered deterministic cases

The original MIT fixture `tests/pfr/pfr-ft-0-0-1.pfr` remains the input artwork;
the new test reconstructs native layout with independent character records.

1. Shared/distinct tables yield identical models and reports for 1/2/7/16 fonts,
   both dialects and automatic/explicit matching dialect selection.
2. Sixteen references to one 2,048-character table fit a 256 KiB budget;
   distinct tables still fail after receiving their extra input-byte allowance.
3. Outline losses retain each logical location and order.
4. Equal offsets with different sizes do not share the cached result.
5. Cache state is call-local and explicit incompatible dialect is rejected.
6. Eighty real output glyphs exceed a 66-glyph cap although the character table
   itself contains exactly 66 entries.
7. Cached absent-strike records preserve separate logical loss locations.
8. Sorted cache storage preserves nonmonotone logical order and distinct family
   identities.

These are ordinary deterministic integration tests. They are not fuzz inputs,
mutation campaigns, instrumented coverage, or timing acceptance. The original
nine PFR tests remain unmodified.

See `historical-validation-summary.json` for previously observed results,
`recovery-status.md` for evidence limitations, and `reader-inventory.md` for
local continuation. Final aggregate gates must include all later shared changes.
