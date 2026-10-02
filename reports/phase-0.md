# Phase 0 acceptance: infrastructure and exact charset tables

Date: 2026-10-02 UTC.
Status: **Phase 0 complete. The full project is not complete.**

W0.1 and W0.2 have been reviewed, accepted, and merged into local `main`.
This report adds documentation only; it does not implement a font format or
begin phase 1. All 46 planned font formats remain explicitly unimplemented.

## Accepted work

- **W0.1 — Workspace and infrastructure**, commit
  `cd7e6e37e33bc863c775eff811c3d8e92a302f3b`, branch `w0.1-skeleton`:
  four Rust crates, edition 2024, pinned toolchain and lockfile, format feature
  slots, reports and typed errors, generic detector/reader/writer contracts,
  scoped resource limits, validated multi-asset output, strict-loss handling,
  CLI dispatch skeleton, tests, license/provenance records and local/CI checks
- **W0.2 — Exact generated charsets**, commit
  `b35c048dc8a4530a6fe039402bc967f3fed1d7bc`, branch `w0.2-charsets`:
  all 62 SPEC-v3 canonical keys (9 Unicode identities and 53 other keys),
  aliases and metadata, 35 unique core plus 12 unique CJK tables, strict
  Python-derived forward/reverse maps, all 207 GB18030 four-byte ranges,
  FNT metadata resolution, HBF/HZK helpers, feature gating, reproducible
  generation, independent comparison tools and charset regression tests

Implementation `main` was `b35c048dc8a4530a6fe039402bc967f3fed1d7bc` before
this report-only commit. The delivered `REFS.txt` records the exact final main
and work-item branch tips. Full work-item detail is in `reports/w0.1.md` and
`reports/w0.2.md`.

## Verification and evidence ownership

### Full checks rerun by the coordinator

The coordinator independently reran the complete `scripts/check.sh` after
reviewing each work item; both runs passed. The final W0.2 run covers the
merged implementation source now on main:

1. Checked-in charset generation drift check: CPython 3.12.14, 53 non-Unicode
   keys, 35 core and 12 CJK unique tables, 207 GB18030 ranges, 1,087,996 valid
   four-byte mappings
2. `cargo fmt --all -- --check`
3. Clippy for the entire workspace/all targets/all features, and separately
   core/no-default-features, both with warnings denied
4. Workspace all-features: 11 charset tests, 21 contract tests, 8 CLI tests,
   and the charset documentation example passed
5. Workspace defaults: 9 charset tests, 21 contract tests, 8 CLI tests,
   and the documentation example passed
6. Core/no-default and core/BDF+PCF-only: each passed 9 charset tests,
   20 contract tests and the documentation example
7. Core/CJK-only: 11 charset tests, 20 contract tests and the documentation
   example passed
8. `cargo deny --locked --all-features check`: advisories, bans, licenses and
   sources all OK, without exclusions or ignored failures

There were no failed or ignored tests in those runs. Reserved Python/WASM
crates compile but have zero binding tests and no functional binding API.
The complete coordinator logs accompany this delivery under `checks/`:

- `w0.1-parent-check.log` SHA-256:
  `22d0e5e26e8e86d87a9050787b4a56ac1bce769c5d4c9d2e1af732332adb4a23`
- `w0.2-parent-check.log` SHA-256:
  `becc56f9ae2d977a610cd2637f84860b312c3efda20a09b4c76866891cec3526`

The delivery verifier reviewed those logs and reused these completed aggregate
checks rather than claiming another full run. No implementation files were
changed after the final coordinator check; only this report was added.

### Exhaustive charset comparisons run by W0.2

The W0.2 worker actually ran the independent Rust/Python comparator, as
recorded in its committed work-item report. These are **worker-run results**,
not a fresh run by the delivery verifier:

- CJK enabled: **66 key/GL variants, 13,308,657 exact mapping pairs, zero
  differences**; 10.13 seconds for reference/probe comparisons, excluding
  release compilation
- CJK disabled: **45 variants, 10,634,186 exact mapping pairs, zero
  differences, 17 explicit feature errors**; 3.52 seconds, excluding compilation
- Compared all 65,536 short codes and the complete applicable reverse domain,
  including surrogate behavior; Unicode/GB18030 use the full Unicode domain,
  and GB18030 additionally checks every one of 1,587,600 four-byte candidates
- Results are byte-for-byte mapping comparisons, not just matching digests

Reproduction with the original supplied task package:

```sh
python3 tools/check_charsets.py --reference ../reference/bdfparser
python3 tools/check_charsets.py --reference ../reference/bdfparser --no-cjk
```

Both workers also report warning-free library rustdoc builds. W0.2 verified
that CJK-only/no-default production dependency trees contain only `fontpixel`.
These are distinguished from the coordinator's aggregate check above.

### Delivery verifier's additional checks

- Started on a clean `main` at the accepted W0.2 commit
- Executed the actual CLI in default and all-features builds; each JSON output
  contains exactly the same **46 unique IDs** as `FORMATS.md`, with every
  `detect`, `read`, and `write` capability **false**. Default feature selection
  has 3 enabled entries; all-features has 46. Selection never claims support
- Inspected the human-readable CLI output: one header plus 46 rows, each
  reader/writer marked `not implemented`
- Verified all **690 protected files** (631 reference files and 59 samples)
  by SHA-256 against the original input archive; **zero changed, missing or
  extra files**. The sorted manifest accompanies the delivery
- Original task archive SHA-256:
  `c3c46654818ae828ddf5f40a2d4ac8cb2412d78fde08c5dbaac4a8b84c493040`
- Protected-files manifest SHA-256:
  `2e91f1883c5e61353ecaeeb6a565c2cddec5144fa7b96c8a4d56208d349ae7c1`
- Repository PLAN.md and FORMATS.md are byte-for-byte identical to the originals
- Verified the original archive's ZIP CRCs; checked repository whitespace
- Delivery packaging additionally verifies the full-history bundle, a fresh
  clone and explicit restoration of every local branch, repository object
  integrity, ordinary ZIP CRCs, per-file SHA-256 and exact archive coverage

Exact packaging results, final refs and file hashes are supplied beside this
report, avoiding a self-referential report/commit or archive hash.

## Decisions made in phase 0

The dated rationale remains in `docs/decisions.md`. In summary:

- Treat phase 0 as two work items despite the heading's count of one
- Preserve final full parity/performance requirements while accepting each
  phase's implemented capabilities; report later-codec checks as deferred
- Keep registry contracts generic until W1.1 supplies the actual shared model
- Represent sidecar input and multi-asset output explicitly; enforce resource
  and path contracts without claiming a filesystem sandbox or atomic writes
- Track information loss independently from severity, including read-time loss
- Separate catalogue/feature visibility from implemented capabilities; prefer
  structural evidence and do not guess ambiguous filename extensions
- Pin verified tooling, minimize core dependencies, reserve bindings honestly,
  retain the unchanged original specification, avoid duplicate CLI rustdoc paths
- Use exact checked-in Python charset tables and explicit missing-CJK errors;
  use compact ordered structures and deterministic, allocation-free lookups
- Interpret SPEC's `iso2022cn` as per-glyph GB2312 GL codes, not a stateful
  ISO-2022 stream implementation; represent GB18030 four-byte mappings exactly
- Preserve numeric surrogate identity while keeping Rust `char` valid
- Keep contextual FNT charset values unresolved rather than guessing locale

## Measurements, limitations and unimplemented work

W0.2's numeric table payload totals 1,624,360 bytes, excluding descriptors/code.
Its illustrative unstripped release parity probe measured 546,272 bytes for
core and 2,121,472 bytes with CJK (difference 1,575,200 bytes). These are build
size observations, not final library, CLI or WASM delivery-size promises.

No phase-1 font performance benchmark has run. Comparator timings above are
not PLAN §10's five-run-median font parsing/conversion/rendering benchmarks.
No completed font codec, Font/Glyph/Bitmap/FontSet model, renderer, gzip support,
real-font roundtrip, full bdfparser parity suite, font-reader fuzz campaign,
monobit/fontTools/OTS/FreeType/HarfBuzz crosscheck, Tong/catalogue equivalence,
Python binding, WASM binding, or final migration document is claimed.

Further boundaries:

- `info` and `convert` parse/dispatch but fail explicitly for unimplemented
  formats; they do not produce a successful empty font or overwrite output
- Synthetic registry roundtrips exercise contracts, not real font formats
- Filesystem sidecar resolution, symlink-safe roots and output persistence
  still need their later implementations
- ISO-2022 escape/shift streams, HZ streams, rejected multicharacter mappings,
  and EUC-JP three-byte glyph codes are outside the exact SPEC glyph-code maps
- Unknown charset warning-once integration belongs to W1.5
- Charset parity is pinned to CPython 3.12.14 mappings, not a claim covering all
  later editions of the underlying standards; changes require reviewed regeneration
- CI workflow definitions exist, but no remote CI run or remote push is claimed
- Original reference and sample assets, downloaded tools and `target/` are
  intentionally excluded from the delivery. Keep the original task package
  for optional reference comparisons and later font work

No user decision or unresolved external blocker is required for phase 0.

## Tools and worker configuration

Delivery verification observed Rust 1.99.0, Cargo 1.99.0, cargo-deny 0.20.2,
and CPython 3.12.14 on Linux. The locally isolated toolchain is not a repository
or delivery dependency; a restored repository uses `rust-toolchain.toml`.

For W0.1, W0.2 and this delivery-verification work item, requested worker
parameters were `gpt-6-astra`, reasoning `xhigh`, and `fork_turns="none"`.
Actual service-reported model/effort are not independently visible:
**无法核实 (cannot independently verify)**. Fast mode was requested for the
project, but the native spawn interface did not expose a fast/service-tier
parameter; actual fast mode is also **无法核实**. No worker created child workers.

## Next phase

After this phase-0 delivery, begin **W1.1: the core data model and Bitmap**,
including the complete bitmap operations and performance benchmarks from
PLAN §§4.1–4.4. Phase 1 subsequently integrates BDF, PCF, rendering, the bdfparser
compatibility/parity layer, and YAFF in the specified dependency order.
This delivery verifier does not start those items.
