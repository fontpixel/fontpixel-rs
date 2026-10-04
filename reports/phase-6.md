# Phase 6 reconstruction checkpoint — NOT ACCEPTED

Snapshot status: 2026-10-04, following the 18:33 UTC workspace incident.

This is an unvalidated reconstruction checkpoint. It is neither a completed phase
nor a release. The original phase-5 handoff was restored from the verified Library
ZIP to main `12710057e9787318d1db70db41a60fca016ad53c`, preserving 54 refs. Missing
later phase-6 Git objects and raw validation logs have not been recovered.

## Work present

Recovered or reconstructed source covers merge library and CLI, parallel batch,
diff/validate/charset commands, native metadata overrides, explicit embedded
metrics, PFR shared-table caching, resource accounting, compatibility lookup,
BDF/YAFF/OTB optimizations, transaction protection, documentation and performance
harness work. Some files match historical hashes exactly; others are rewrites.
Per-worker recovery checkpoints retain provenance, patches and incomplete items.
Source presence does not prove successful compilation, correctness or performance.

## Current blockers and remaining acceptance

- The authorized Rust toolchain restoration is incomplete. Cargo download from
  the official host was network-policy blocked, including an elevated retry.
- No reconstructed Rust project has been compiled, formatted or tested. Static
  text/hash checks and standalone Python tool tests do not replace Rust gates.
- Shared integration, complete conversion helpers, catalogue rename composition,
  output/report/resource regressions and documentation consistency remain under
  review. Consult TODO.md and the current recovery status for exact gaps.
- Historical pre-incident complete tests and parity results are not evidence for
  this rewritten source; their original raw logs are unavailable.
- Last historical five-run medians still missed OTB/YAFF targets; full CLI Tong
  export still exceeded 40 seconds. No performance target has been waived.
- Long final-source ASAN checks remain explicitly deferred to the user's own
  computer. They were not performed and must not be marked passed.

## Rules for continuation

Rebuild and validate every restored component; run the whole feature matrix,
warning-free rustdoc, independent output/compatibility oracles, real five-run
performance suite and archive audits on frozen final source. Preserve all
strict/loss, resource, Gray8, explicit-format and source-protection contracts.
Original references/samples/validation inputs remain read-only. Do not alter an
oracle to make reconstructed outputs pass. Record remaining naming/metric/default
policy differences explicitly; glyph equality is not complete font equivalence.

Each coherent reconstruction checkpoint is saved externally before subsequent
major changes. Local Git is not the external backup. Final delivery still requires
an all-ref bundle, verified restore instructions and one complete ZIP.

The owner authorized reconstruction and rewriting, with six concurrent workers
requested as gpt-6-astra/xhigh/fork_turns=none. Fast mode cannot be set through the
native worker interface; actual backend identity remains unverified. After phase-6
delivery, continue phases 7 then 8 with full native Python/WASM/JS APIs plus separate
compatibility APIs. No phase 9, remote push, publication or deployment is authorized.
