# Unit A acceptance: WOFF1/WOFF2 full-lifecycle ledger and codec bounds

Scope: `crates/fontpixel/src/woff/**`, additive helpers in
`crates/fontpixel/src/working.rs`, `crates/fontpixel/tests/woff*.rs`, and a
dedicated allocation-measurement test package. Base: main `6297998`.

Design sources: `WORK/owners/w64/NEXT-LEDGER-DESIGN.md` (native WOFF stages 1-6,
exact dependency blocker, WOFF1 backend caveat), `brotli-decoder-alloc-audit.md`,
`brotli-allocation-site-inventory.json`,
`brotli-encoder-helper-allocation-inventory.json`,
`brotli-encoder-fallible-proposal.md` (the vendored/fallible route is **not**
implemented), `woff-mod-batch04.PROPOSED.patch`, `woff-zlib.PROPOSED.rs`.
The phase-6 limitation being closed is the first two bullets of
`docs/w6.1-writer-budget.md` "Known limitations accepted for phase 6".

Hard constraints carried into every item: no default limit or cap is raised; no
strict/loss/report behaviour is weakened and no report entry is removed; no
`#[allow]`; no guessed source credit; resource failures stay
`Error::ResourceLimit`; default output bytes stay identical.

## A1. One ledger per public WOFF operation (goal 1)

Done means:

- `woff1::{encode, decode}` and `woff2::{encode, decode, encode_many}` (and the
  WOFF2 collection paths behind them) create exactly one
  `Budget::new(l.sfnt.resources.max_total_bytes, ...)` per font operation and pass
  it to every stage. No stage creates its own full-size budget any more
  (today `woff1::compress/decompress`, `woff2::compress/decompress`,
  `Sfnt::parse`, `SfntBuilder::build` each start a fresh full ledger).
- Every fontpixel-owned allocation is admitted **before** it is made and its
  lease lives until the storage drops (`Owned`/`Vector`/`Buffer`/`Nested`):
  - WOFF1 decode: directory records, span list, every decompressed/copied table,
    the borrowed table list, assembled sfnt (including the DSIG re-assembly,
    where the old sfnt is dropped before its lease), metadata/private blocks.
  - WOFF1 encode: parsed source directory (`Sfnt::parse_with_budget`), table
    list, canonical sfnt (old canonical dropped before its charge on DSIG
    re-assembly), re-parse of canonical, sorted records, per-table compressed
    output, growable final output (header written through mutable slices only).
  - WOFF2 decode: record directory, tag set, decompressed raw stream (kept
    live while tables copy from it; no credit from `raw.len()`), table map
    (tree nodes + values), glyf/loca/hmtx reconstruction (old table content
    stays charged until the replacement is inserted), builder/table ownership,
    final sfnt.
  - WOFF2 encode: parsed directory, table map, normalized glyf/loca, all seven
    transform streams, bbox bitmap and overlap arrays, source loca offsets,
    per-glyph lengths/points/flags/instructions, assembled transform output,
    hmtx transform, order list, header, raw stream, parallel input clone, the
    `BrotliEncoderMaxCompressedSizeMulti` output buffer and allocator vector,
    final output.
  - Collections: record list, per-face index lists and tag sets, raw stream,
    per-face table maps and outputs, payload list and shared-table map, TTC
    output. A table shared by several faces is charged once (its bytes move into
    one payload); per-face transformed copies remain charged.
- `sfnt/collection.rs` is outside this unit; its `encode` is called with the
  ledger's remaining allowance as its byte cap and its computable output size is
  reserved in the WOFF2 ledger (documented as the interface boundary).

Tests: `woff::ledger_tests` (unit) and `tests/woff_budget.rs` (integration).

## A2. Codec errors keep their class (goal 3)

Done means: any `fontpixel::Error` that travels through `std::io::Error`
(Brotli writer into a ledger `Buffer`, `io::Write for Buffer`) is unwrapped with
`io::Error::downcast::<crate::Error>()`; a ledger failure inside the Brotli
writer is returned as the original `Error::ResourceLimit`, never as
`InvalidInput`. Metadata decode resource failures remain terminal (existing
`read_aux` rule) while invalid streams remain a reported loss.

Test: public-API binary search of `max_total_bytes` for WOFF1/WOFF2 encode and
decode asserting every failure below the boundary is `ResourceLimit`.

## A3. Brotli encoder heap bound (goal 2)

Done means:

- `fontpixel::woff::woff2::brotli_encoder_heap_bound(quality, lgwin, input_len,
  threads) -> Option<usize>` derives a conservative upper bound of the locked
  `brotli 9.0.0` encoder's total heap for fontpixel's call shape
  (`BROTLI_MODE_FONT`, `size_hint = input_len`, all other parameters default,
  single-stream `BrotliCompress` when `threads == 1 || input_len < 128 KiB`,
  otherwise `compress_multi` with `threads` `StandardAlloc` workers). The
  derivation is a doc comment with a per-term table citing `file:line` of each
  allocation site (sum over simultaneously live state, max over strictly
  sequential phases).
- Before calling the encoder, the WOFF2 ledger reserves that bound as a lease
  that lives for the whole encoder call (all threads joined). The dependency is
  neither patched nor vendored; quality, threads, transforms and output bytes
  are unchanged.
- Empirical proof in the dedicated package `crates/fontpixel-heap-check`
  (see A6): counting `#[global_allocator]`, peak bytes during the encoder call,
  qualities 0-11, lgwin 22 (plus 10/16/24 for the formula), threads 1 and 2,
  inputs: empty/tiny, 64 KiB, ~1 MiB, and an optional multi-MB sfnt from
  `FONTPIXEL_HEAP_CHECK_FONT` (skipped with a message when unset). Assert
  observed peak <= bound for every case; print bound/observed table.

**Decision point for the coordinator (expected outcome, not an implementation
choice).** From the locked source, at quality >= 10 the encoder's
`BrotliBuildMetaBlock` -> `BrotliSplitBlock` -> `ClusterBlocks`
(`brotli-9.0.0/src/enc/block_splitter.rs:419-660`) allocates histogram arrays
and pair queues proportional to `num_blocks`, and `FindBlocks`
(`block_splitter.rs:225-330`) does not bound `num_blocks` below the symbol count
of the metablock. At quality 11 `BrotliCreateHqZopfliBackwardReferences`
(`backward_references/hq.rs:1259-1315`) additionally grows a `u64` match array to
about 100 entries per input position of a 2^18-byte block with old/new overlap.
A sound bound therefore exceeds the 256 MiB default for metablocks of a few tens
of KiB at q >= 10. Admitting it makes default (q11) WOFF2 encodes of real fonts
fail with `ResourceLimit`. This unit does **not** shrink the bound, add a
toggle, or change test budgets to hide that; it reports every affected default
call site/test and the full-Tong number (A7).

## A4. Brotli decoder and deflate bounds (goal 2, continued)

- Brotli decoder: every decoder cell already goes through the ledger allocator
  (`woff2/brotli_decode.rs`). Done means the empirical package confirms that the
  heap peak of `woff2::decode` is <= the smallest `max_total_bytes` that
  succeeds plus a small documented constant for unledgered report strings.
- Deflate (WOFF1): `woff/zlib.rs` calls `miniz_oxide =0.9.1` directly, so flate2
  feature unification (`zlib-rs`/C backends) cannot select a different backend
  for WOFF1. Done means `cargo tree -e features -i flate2` and
  `-i miniz_oxide` for the default and all-features profiles are recorded below
  and the empirical package shows WOFF1 encode/decode heap peak <= admitted
  ledger capacity (+ the same constant) for levels 0-9.

## A5. Exact-capacity regressions (goal 4)

For each of: WOFF1 encode, WOFF1 decode, WOFF2 encode with transforms on, WOFF2
encode with transforms off, WOFF2 decode, WOFF2 collection encode and decode:
measure the ledger high-water mark `P` of a successful run (additive
`Budget::peak()` helper), then assert success with `Budget::new(P)` and
`Error::ResourceLimit` with `Budget::new(P - 1)`, and that the budget is fully
released after both. WOFF2 cases use quality <= 9 so the exact boundary is not
dominated by the A3 bound; one additional case covers q11 to show the bound is
part of the admitted peak.

Default bytes: existing byte-identity tests (`tests/woff.rs`, zlib flate2
equality, fixture round trips) keep passing for every case whose bound fits;
WOFF2 output for any case that still succeeds is byte-identical to `main`
(checked against encodings produced with an unlimited ledger).

## A6. Measurement package

`crates/fontpixel-heap-check` (publish = false), a workspace member that is
**not** in `default-members`. Reason: the workspace lint table sets
`unsafe_code = "forbid"`, which cannot be relaxed inside `crates/fontpixel`
without `#[allow]`; a `GlobalAlloc` implementation requires `unsafe impl`. The
package keeps its own lint table (missing_docs, clippy::all) and contains only
the counting allocator and its tests. Run it by name:
`cargo test --locked --release -p fontpixel-heap-check`. Cargo.lock gains one
path-package entry and no registry packages (same `brotli` pin). Its two
measurement tests are `#[ignore]`d so the debug `cargo test --workspace` gate
stays fast; run them with
`cargo test --locked --release -p fontpixel-heap-check -- --ignored --nocapture`
(optionally `FONTPIXEL_HEAP_CHECK_FONT=<large sfnt>`).

## A7. Default workload report (goal 5)

Report the bound and the observed heap for the default full-Tong square TTF ->
WOFF2 path (`full-correctness-v2/vector-native/font.ttf`, release build), the
extra budget needed beyond the 256 MiB default, and the list of default-limit
tests/call sites that now fail.

## Verification commands

- `cargo test --locked -p fontpixel --features woff,woff2,ttc --lib woff`
- `cargo test --locked -p fontpixel --features woff,woff2,ttc --test woff --test woff_budget`
- `cargo test --locked --release -p fontpixel-heap-check -- --ignored --nocapture`
- `cargo clippy --locked -p fontpixel --all-targets --all-features -- -D warnings`
- `cargo clippy --locked -p fontpixel-heap-check --all-targets -- -D warnings`
- `cargo fmt --all --check`

## Status (2026-10-05, branch of the unit-A worktree)

| Item | Status | Evidence |
|---|---|---|
| A1 ledger lifecycle | done | `woff/{mod,woff1,woff2}.rs`, `woff2/{transform,collection}.rs`; `woff::ledger_tests` |
| A1 shared tables once | done | `ledger_tests::shared_collection_tables_are_charged_once` (4 identical faces with a 64 KiB table add < 32 KiB of peak) |
| A1 sfnt::collection boundary | done as reservation | `collection::{parse_bound, assemble_bound}` reserve a source-derived bound around `Collection::parse` / `sfnt::collection::encode` (that file is outside this unit and keeps its own ledgers). Both bounds were derived from `sfnt/collection.rs` at `6297998`; any later change to that file's allocations must re-check them |
| A2 io::Error unwrapping | done | `woff::io_error`; `woff2::tests::encoder_bound_is_admitted_and_writer_failures_stay_resource_limits`; `tests/woff_budget.rs` sweeps every `max_total_bytes` below each boundary (512-value window + geometric sample) and sees only `ResourceLimit` |
| A3 encoder bound function | done | `woff::woff2::brotli_encoder_heap_bound`, derivation in `woff2/brotli_encode.rs` module docs |
| A3 admission before encoding | done | `woff2::compress` reserves the bound for the whole call (all workers joined) |
| A3 empirical proof | done | `fontpixel-heap-check/tests/encoder_bound.rs`: 264 cases, q0-11 x threads 1/2 x 8 inputs (+lgwin 10/16/24); max observed/bound 0.929; Tong sfnt included via env |
| A3 defaults unchanged | **conflict, see below** | bytes identical wherever the encode still succeeds; q >= 10 now fails at 256 MiB for Brotli inputs above ~40 KiB |
| A4 Brotli decoder | done | `fontpixel-heap-check/tests/codecs.rs`: decode heap <= minimal ledger + 48 bytes on all cases |
| A4 deflate backend | done | WOFF1 calls `miniz_oxide =0.9.1` directly (`woff/zlib.rs`); flate2 is only the zlib test oracle. `cargo tree -e features -i flate2` (workspace default, workspace all-features, fontpixel all-features): only `default`, `rust_backend`, `miniz_oxide`, `any_impl`, `runtime_detection`; no `zlib`/`zlib-rs`/C feature is active. WOFF1 encode/decode heap <= minimal ledger + 48 bytes at levels 0/6/9 incl. Tong |
| A5 exact capacity | done | `ledger_tests`: WOFF1 encode/decode, WOFF2 encode q2/q5 transforms on/off, WOFF2 decode on/off, WOFF2 collection encode/decode; exact capacity succeeds with identical bytes, one byte short is `ResourceLimit`, all charges released |
| A6 measurement package | done | `crates/fontpixel-heap-check` (not a default member); Cargo.lock gains only its path entry |
| A7 default workload report | done | below |

Byte identity: a probe encoding/decoding every `tests/data` ttf/otf (45 files, plus a
two-face and a shared-face TTC) with WOFF1 levels 0/6/9 and WOFF2 q0/1/2/4/5/9/10/11,
threads 1/2, transform combinations and auxiliary blocks produced identical digests
and identical error lines to `6297998` (9512 lines). The full Tong sfnt at q5/q11,
threads 1/2 is identical with a 64 GiB ledger; at the 256 MiB default q11 now fails.

### Bound versus default (superseded by the resumed decision below)

`brotli_encoder_heap_bound(q, 22, n, threads)`, MiB:

| n | q9 t1/t2 | q10 = q11 t1 | q10 = q11 t2 |
|---|---|---|---|
| 1 KiB | 43.4 | 149.5 | 149.5 |
| 32 KiB | 45.2 | 210.5 | 210.5 |
| 48 KiB | 46.1 | 294.6 | 294.6 |
| 64 KiB | 47.1 | 379.3 | 379.3 |
| 160 KiB | 52.6 / 96.3 | 885.4 | 927.5 |
| 1 MiB | 83.7 / 134.3 | 5423.5 | 5472.5 |
| 8 MiB | 97.7 / 187.4 | 6771.0 | 13534.0 |

At q >= 10 the bound passes 256 MiB for Brotli inputs between 32 and 48 KiB, so
default-quality (11) WOFF2 encoding of essentially every real font now fails with
`ResourceLimit { resource: "WOFF working bytes", limit: 268435456 }`. Observed
encoder heap is 1-12% of the q >= 10 bound (it is 50-93% at q <= 9).

Full Tong square TTF (`full-correctness-v2/vector-native/font.ttf`, 7,550,360
bytes) with default `woff2::Options` (q11, 2 threads), release build:
smallest succeeding `max_total_bytes` is about 13,544 MiB, i.e. about 13,288 MiB
(~13 GiB) more than the 256 MiB default. Observed encoder heap for that input is
about 163 MB (q11, 2 threads). At q9/2 threads it needs 206.4 MB of ledger
(observed 133 MB) and fits the default.

Default-limit tests in the focused set that now fail:
`crates/fontpixel/tests/woff.rs::large_parallel_streams_are_deterministic_and_preserve_all_tables`
(q11 on a 160 KiB table). All other `tests/woff.rs`, `vector_raster`, `contracts`,
`embedded_bitmap`, `model`, `bdf` and the CLI `cli`/`merge_phase6`/
`phase6_cli_recovery` tests pass (their WOFF2 inputs are small). Expected to fail
outside this unit's test set: every default-quality WOFF2 export of a full font
(Tong examples, full Tong/catalogue CLI conversions to `.woff2`).

Choices open to the coordinator, none taken here: accept the gate as is; exclude
q >= 10 from the strict claim and document it; tighten the q >= 10 terms with a
further proof (the dominant terms are `ClusterBlocks` histogram arrays and pair
queues sized by block count, bounded today only by symbol count); or reopen the
fallible-allocator route that the design sources rule out for this phase.

## Resumed after the coordinator decision (2026-10-05)

The ledger commits (`bc024c8`, `5146bde`) were merged into main (`417bee2`);
the first bound commits (`73eb8bc`, `1facfb7`) were not, because they made
default q11 WOFF2 fail. Main was merged back into this branch without
conflicts (`working.rs` keeps one `Budget::peak`; B's `vec_into`/`split` are
untouched; `sfnt/collection.rs` is unchanged, so the TTC reservations still
match it).

**Route: empirical for quality 10-11; proven for quality 0-9.**

- Tighter proof attempt (timeboxed): the dominant q >= 10 terms are
  `ClusterBlocks` histogram arrays and pair queues sized by the block count.
  The block-switch cost argument in `FindBlocks`/`update_cost_and_signal`
  only proves consecutive switches are `ceil(S / D)` symbols apart, with switch
  cost `S` = 28.1/13.5/14.6 bits and per-symbol cost spread `D` up to
  `log2(total) + 2` bits, i.e. one or two symbols. Nothing in
  `block_splitter.rs`/`cluster.rs`/`metablock.rs` (max 256 block types, batch
  size 64, 100/50 split histograms) caps the block count of a metablock below
  that, so a proven bound stays near 14 GB for a full Tong stream. Not
  provable within the timebox.
- Admission now: `brotli_encoder_heap_bound` = proven bound for q0-9
  (unchanged, `brotli_encoder_proven_heap_bound`), and for q10-11 the exact
  persistent tables (H10 hasher, ring buffer + first cell, Huffman trees,
  bookkeeping) plus
  `1.5 * (1 MiB + 80 * min(n, 2^lgblock) + 7 * min(n, max_length))` per
  encoder instance, plus each parallel worker's output cell. Documented as
  **empirical, not proven** in the `brotli_encode.rs` module docs, here, and in
  `docs/w6.1-writer-budget.md`.
- Evidence: `fontpixel-heap-check/tests/encoder_bound.rs`, 816 cases over 31
  inputs (synthetic, adversarial repeat structures: recombined vocabularies,
  chunk-mutation chains, Fibonacci words; all 16 full Tong TTF/OTF exports;
  the Tong sfnt), q0-11, lgwin 10/16/22/24, 1 and 2 threads. Observed peak <=
  admitted in every case. q0-9: max 0.929. q10-11: the data-dependent heap is
  at most 0.65 of the envelope; total observed/admitted at most 0.90 at
  lgwin 22.
- Full Tong q11 / 2 threads (admitted vs observed encoder heap): square TTF
  9.06 MB sfnt 256.8 vs 168.9 MB; dot TTF 6.47 MB 227.0 vs 154.8 MB; the
  `vector-native` sfnt 239.5 vs 162.1 MB.
- Default CLI path: the WOFF2 ledger of each default Tong task has 254.8-256.1
  MB remaining at the encoder gate for raw Brotli streams of 6.05-6.83 MB; the
  largest stream needs about 231 MB of encoder admission plus 13.7 MB of
  fontpixel buffers, leaving about 10 MB. Estimated limit: raw streams up to
  about 7.5 MB at q11/2 threads fit the 255 MiB CLI working allowance; larger
  ones return `ResourceLimit` (q <= 9 or a larger limit then applies).
- Default Tong batch (`benches/phase6_tong.py batch`, release build of this
  branch): exit 0, 64/64 tasks, 84 artifacts. All 84 hashes equal both main
  `417bee2` and `6297998` run the same way. Against
  `WORK/gates/100-v2-tong-default-cli/stdout.log` (frozen-binaries-v2), 60
  BDF/PCF/OTB artifacts match and 24 TTF/OTF/WOFF2 differ for main and
  `6297998` alike: the sfnt `OS/2.ulCodePageRange1` byte at offset 79 and the
  derived `head.checkSumAdjustment` differ. That source change predates this
  unit; WOFF2 bytes are unchanged by it.
- Existing tests: `tests/woff.rs` (including
  `large_parallel_streams_are_deterministic_and_preserve_all_tables` with its
  original budget), `woff_budget`, the lib unit tests, `vector_raster`,
  `contracts`, `embedded_bitmap`, `model`, `bdf`, and the CLI `cli`,
  `merge_phase6`, `phase6_cli_recovery` tests pass.
